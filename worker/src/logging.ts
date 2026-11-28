/**
 * Worker log sinks: a row-capped D1 table for durable history and a Telegram
 * HTML log channel modeled on Numera Group Bot's structured six-orb layout.
 *
 * Logging never breaks bot replies or pack announcements. Every sink failure is
 * caught and reported as a redacted console line so webhooks still return 2xx.
 */

import { errText, redact as sanitise } from "./redact";
import { escapeHtml, Telegram } from "./telegram";
import type { BotName, Env } from "./types";

/** Row cap for the D1 log table, oldest evicted first. */
export const LOG_ROWS_CAP = 5_000;
const EVICT_EVERY = 250;
const DETAIL_LIMIT = 2_000;

export type LogLevel = "INFO" | "WARNING" | "ERROR" | "CRITICAL";

export interface LogEntry {
  bot: BotName;
  level: LogLevel;
  event: string;
  detail?: string;
  /** Structured fields rendered as `key: <code>value</code>` in the channel. */
  fields?: Record<string, unknown>;
  /** Override channel delivery. Default: `isImportantLogEvent(level, event)`. */
  toChannel?: boolean;
  /** Override D1 storage. Default: warnings, errors, and critical entries. */
  store?: boolean;
}

const INSERT_SQL =
  "INSERT INTO logs (ts, bot, level, event, detail) VALUES (?1, ?2, ?3, ?4, ?5)";
const EVICT_SQL = "DELETE FROM logs WHERE id <= ?1";

/**
 * Business events routed to the Telegram log channel at INFO level so the
 * operator sees pack announcements, bot usage, and test probes on their phone
 * without flooding D1.
 */
export const IMPORTANT_LOG_EVENTS: ReadonlySet<string> = new Set([
  "publish",
  "access.denied",
  "emoji.lookup",
  "emoji.reverse_lookup",
  "emoji.partial_lookup",
  "bot.greeted",
  "channel.forwarded",
  "log_channel.test",
  "bot.started",
  "worker.configured",
  "settings.changed",
]);

/** Human-readable titles for important INFO events in the log channel. */
export const CHANNEL_EVENT_TITLES: Readonly<Record<string, string>> = {
  publish: "Pack announced",
  "access.denied": "Access denied",
  "emoji.lookup": "Emoji IDs extracted",
  "emoji.reverse_lookup": "Reverse ID lookup",
  "emoji.partial_lookup": "Partial ID lookup",
  "bot.greeted": "Bot greeted",
  "channel.forwarded": "Channel emoji forwarded",
  "log_channel.test": "Log channel test",
  "bot.started": "Bot started",
  "worker.configured": "Worker configured",
  "settings.changed": "Settings changed",
};

const LEVEL_MARKER: Record<"WARNING" | "ERROR" | "CRITICAL", string> = {
  WARNING: "🟠 WARNING",
  ERROR: "🔴 ERROR",
  CRITICAL: "🔴 CRITICAL",
};

/**
 * Pick one of the six circle orbs (`🔴🟠🟡🟢🔵🟣`) matching severity and action.
 * Never emits satellite glyphs such as ❌, ⚠️, or ℹ️.
 */
export function logChannelEmoji(level: LogLevel, event: string): string {
  if (level === "ERROR" || level === "CRITICAL") return "🔴";
  if (level === "WARNING") return "🟠";
  const e = event.trim().toLowerCase();
  if (/denied|blocked|throttled|unauth/.test(e)) return "🟠";
  if (e.startsWith("channel.") || e.startsWith("settings.") || e.startsWith("worker.")) {
    return "🟣";
  }
  if (e.includes("greet") || e.includes("partial") || e.includes("empty") || e.includes("review")) {
    return "🟡";
  }
  if (e === "publish" || e === "log_channel.test" || e === "bot.started"
      || e.includes("recovered") || e.includes("completed") || e.endsWith(".ok")) {
    return "🟢";
  }
  return "🔵";
}

/** True when an entry merits a Telegram log-channel message. */
export function isImportantLogEvent(level: LogLevel, event: string): boolean {
  if (level === "WARNING" || level === "ERROR" || level === "CRITICAL") return true;
  return IMPORTANT_LOG_EVENTS.has(event);
}

/** Fields that may carry user-authored text and must never reach the log channel. */
const TEXT_FIELD_RE = /(^|_)(text|caption|preview|content|body|snippet|message)$/i;
const NOISE_FIELDS: ReadonlySet<string> = new Set([
  "stack", "runtime", "request_id", "utc", "level", "event", "bot",
]);
const DETAIL_FIELD_RE = /^(?:error|detail|reason)/i;

const MAX_FIELDS = 6;
const MAX_FIELD_CHARS = 80;
const MAX_DETAIL_FIELD_CHARS = 400;
const MAX_MESSAGE_CHARS = 1_500;

const HANDLE_FIELDS = [
  ["chat_id", "chat_username"],
  ["user_id", "user_username", "username"],
  ["target_user_id", "target_username"],
  ["actor_id", "actor_username"],
  ["sender_id", "sender_username"],
  ["owner_id", "owner_username"],
] as const;

const NON_ID_FIELD_RE =
  /message_?ids?$|count|^(?:utc|date|time|ts|at)$|(?:duration|timestamp|_at$|_epoch$)|(?:time|date|epoch|until)(?:_?ms|_?seconds)?$/i;
const TELEGRAM_HANDLE_RE = /^[A-Za-z0-9_]{3,64}$/;

function clampText(text: string, max: number): string {
  if (text.length <= max) return text;
  let end = Math.max(0, max);
  if (end > 0 && /[\uD800-\uDBFF]/.test(text[end - 1])) end -= 1;
  return text.slice(0, end);
}

function truncatePlain(value: string, maxChars: number): string {
  if (value.length <= maxChars) return value;
  return `${clampText(value, maxChars)}…`;
}

function normalizeTelegramHandle(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  const bare = trimmed.startsWith("@") ? trimmed.slice(1) : trimmed;
  return TELEGRAM_HANDLE_RE.test(bare) ? `@${bare}` : null;
}

export function redact(s: string, env?: Partial<Env>): string {
  const out = sanitise(s, env);
  if (out.length <= DETAIL_LIMIT) return out;
  const head = clampText(out, DETAIL_LIMIT);
  return `${head}… (+${out.length - head.length} chars)`;
}

/**
 * Normalize a configured Telegram channel ID into the form the Bot API accepts.
 * Bare positive channel IDs copied from Telegram UI gain the `-100` prefix;
 * `@username` and already-negative IDs pass through. Returns null when disabled
 * or invalid.
 */
export function normalizeChatId(value: string | undefined | null): string | null {
  const text = String(value ?? "").trim();
  if (!text || text === "0") return null;
  if (text.startsWith("@")) return text.length > 1 ? text : null;
  if (!/^-?\d+$/.test(text)) return null;
  if (text.startsWith("-")) return text;
  return `-100${text}`;
}

function utcStamp(atMs: number): string {
  return new Date(atMs).toISOString().replace("T", " ").replace(/\.\d{3}Z$/, " UTC");
}

/**
 * Render one log entry in Numera Group Bot's Telegram HTML layout:
 * - Header: `<b>🔴 ERROR · event</b>` / `<b>🟠 WARNING · event</b>` / `<b>{orb} {Title}</b>`
 * - Fields: `bot: <code>general</code>` followed by redacted, HTML-escaped `key: <code>value</code>`
 *   lines with companion `(@username)` annotations folded onto matching numeric IDs
 * - Footer: `🕒 YYYY-MM-DD HH:mm:ss UTC` (+ rate-limit suppression notice when applicable)
 */
export function formatLine(e: LogEntry, atMs: number = Date.now(),
                           suppressed = 0, env?: Partial<Env>): string {
  const titleText = e.level === "INFO"
    ? `${logChannelEmoji(e.level, e.event)} ${CHANNEL_EVENT_TITLES[e.event] ?? e.event}`
    : `${LEVEL_MARKER[e.level]} · ${e.event}`;
  const headerLine = `<b>${escapeHtml(truncatePlain(titleText, 120))}</b>`;
  const footerLine = escapeHtml(`🕒 ${utcStamp(atMs)}`);
  const suppressedLine = suppressed > 0
    ? escapeHtml(`(+${suppressed} suppressed by rate limit)`)
    : null;

  const rawFields: Record<string, unknown> = { ...(e.fields ?? {}) };
  if (e.detail && rawFields.detail === undefined) rawFields.detail = e.detail;

  const knownHandles = new Map<string, string>();
  const directHandles = new Map<string, string>();
  const foldedFields = new Set<string>();
  for (const [idField, ...companions] of HANDLE_FIELDS) {
    const id = String(rawFields[idField] ?? "").trim();
    if (!/^-?\d+$/.test(id)) continue;
    for (const field of companions) {
      foldedFields.add(field);
      const handle = normalizeTelegramHandle(rawFields[field]);
      if (!handle) continue;
      if (!knownHandles.has(id)) knownHandles.set(id, handle);
      if (!directHandles.has(idField)) directHandles.set(idField, handle);
    }
  }
  foldedFields.add("user_name");
  foldedFields.add("no_username");
  if (!directHandles.has("user_id") && /^-?\d+$/.test(String(rawFields.user_id ?? "").trim())) {
    const rawName = typeof rawFields.user_name === "string"
      ? truncatePlain(sanitise(rawFields.user_name, env).replace(/[\r\n\t]+/g, " ").trim(), 48)
      : "";
    if (rawName) {
      directHandles.set("user_id", `${rawName} · no @username`);
    } else if (rawFields.no_username === true) {
      directHandles.set("user_id", "no @username");
    }
  }

  const entries: Array<[string, string, string[]]> = [["bot", sanitise(e.bot, env), []]];
  let omitted = false;
  for (const [key, value] of Object.entries(rawFields)) {
    if (NOISE_FIELDS.has(key) || foldedFields.has(key)
        || (TEXT_FIELD_RE.test(key) && key !== "error_message")) continue;
    if (value === null || value === undefined || value === "") continue;
    if (entries.length >= MAX_FIELDS + 1) {
      omitted = true;
      break;
    }
    const raw = sanitise(typeof value === "object" ? JSON.stringify(value) : String(value), env);
    const cap = DETAIL_FIELD_RE.test(key) ? MAX_DETAIL_FIELD_CHARS : MAX_FIELD_CHARS;
    const handles = new Map<string, string>();
    if (!NON_ID_FIELD_RE.test(key)) {
      const id = raw.trim();
      const direct = directHandles.get(key) ?? knownHandles.get(id);
      if (direct) handles.set(direct.toLowerCase(), direct);
      else if (typeof value === "string") {
        for (const token of raw.matchAll(/-?\d+/g)) {
          const handle = knownHandles.get(token[0]);
          if (handle && !handles.has(handle.toLowerCase())) {
            handles.set(handle.toLowerCase(), handle);
          }
        }
      }
    }
    entries.push([key, truncatePlain(raw, cap), [...handles.values()]]);
  }

  const lines = [headerLine];
  const tailBudget = 1 + footerLine.length + (suppressedLine ? 1 + suppressedLine.length : 0) + 2;
  let used = headerLine.length;
  for (const [key, rendered, handles] of entries) {
    const escapedKey = escapeHtml(key);
    const prefix = `${escapedKey}: <code>`;
    const suffix = "</code>"
      + handles.map((h) => ` (${escapeHtml(sanitise(h, env))})`).join("");
    const candidate = `${prefix}${escapeHtml(rendered)}${suffix}`;
    if (used + 1 + candidate.length + tailBudget <= MAX_MESSAGE_CHARS) {
      lines.push(candidate);
      used += 1 + candidate.length;
      continue;
    }
    const avail = MAX_MESSAGE_CHARS - used - 1 - prefix.length - suffix.length - tailBudget - 1;
    if (avail <= 0) {
      omitted = true;
      break;
    }
    let trimmed = truncatePlain(rendered, avail);
    while (trimmed.length > 1 && escapeHtml(trimmed).length > avail + 1) {
      trimmed = truncatePlain(rendered, Math.max(0, trimmed.length - 2));
    }
    const partial = `${prefix}${escapeHtml(trimmed)}${suffix}`;
    if (used + 1 + partial.length + tailBudget <= MAX_MESSAGE_CHARS) {
      lines.push(partial);
      used += 1 + partial.length;
    }
    omitted = true;
    break;
  }
  if (omitted) lines.push("…");
  lines.push(footerLine);
  if (suppressedLine) lines.push(suppressedLine);
  return lines.join("\n");
}

/** Telegram allows roughly 20 messages/minute to a channel; stay well under it. */
const CHANNEL_MAX_PER_WINDOW = 12;
const CHANNEL_WINDOW_MS = 60_000;
const rate = { windowStartMs: 0, sentInWindow: 0, suppressed: 0 };

async function writeToD1(env: Env, e: LogEntry): Promise<void> {
  if (!env.DB) return;
  const detail = e.detail ? redact(e.detail, env) : null;
  const written = await env.DB.prepare(INSERT_SQL)
    .bind(Date.now(), e.bot, e.level, e.event, detail)
    .run();
  const id = Number(written?.meta?.last_row_id ?? 0);
  if (id > LOG_ROWS_CAP && id % EVICT_EVERY === 0) {
    await env.DB.prepare(EVICT_SQL).bind(id - LOG_ROWS_CAP).run();
  }
}

async function writeToChannel(env: Env, e: LogEntry, atMs: number): Promise<void> {
  const chat = normalizeChatId(env.LOG_CHAT_ID);
  if (!chat) return;
  const token = e.bot === "general" ? env.GENERAL_BOT_TOKEN : env.COIN_BOT_TOKEN;
  if (!token) return;

  if (atMs - rate.windowStartMs >= CHANNEL_WINDOW_MS) {
    rate.windowStartMs = atMs;
    rate.sentInWindow = 0;
  }
  if (rate.sentInWindow >= CHANNEL_MAX_PER_WINDOW) {
    rate.suppressed += 1;
    return;
  }
  rate.sentInWindow += 1;
  const carried = rate.suppressed;
  rate.suppressed = 0;
  const tg = new Telegram(token, env.TELEGRAM_API_BASE);
  await tg.sendMessage(chat, formatLine(e, atMs, carried, env), {
    parse_mode: "HTML",
    disable_web_page_preview: true,
  });
}

/**
 * Send one entry directly to the Telegram log channel and return its message ID.
 * Used by authenticated diagnostic probes (`POST /log-test`) that need delivery
 * confirmation instead of best-effort swallowing.
 */
export async function sendLogChannelDirect(env: Env, e: LogEntry): Promise<number> {
  const chat = normalizeChatId(env.LOG_CHAT_ID);
  if (!chat) throw new Error("LOG_CHAT_ID not configured");
  const token = e.bot === "general" ? env.GENERAL_BOT_TOKEN : env.COIN_BOT_TOKEN;
  if (!token) throw new Error(`${e.bot} bot not configured`);
  const tg = new Telegram(token, env.TELEGRAM_API_BASE);
  const sent = await tg.sendMessage(chat, formatLine(e, Date.now(), 0, env), {
    parse_mode: "HTML",
    disable_web_page_preview: true,
  });
  return sent.message_id;
}

/**
 * Record one log event. Hand the returned promise to `ctx.waitUntil()` so
 * webhook responses never block on D1 or Telegram delivery. Never rejects.
 */
export function log(env: Env, e: LogEntry): Promise<void> {
  const atMs = Date.now();
  console.log(formatLine(e, atMs, 0, env));
  const store = e.store ?? e.level !== "INFO";
  const toChannel = e.toChannel ?? isImportantLogEvent(e.level, e.event);
  const jobs: Promise<void>[] = [];
  if (store) jobs.push(writeToD1(env, e));
  if (toChannel) jobs.push(writeToChannel(env, e, atMs));
  return Promise.allSettled(jobs).then((results) => {
    for (const r of results) {
      if (r.status === "rejected") {
        console.error(formatLine({
          bot: e.bot,
          level: "ERROR",
          event: "log-sink",
          detail: errText(r.reason, env),
        }, Date.now(), 0, env));
      }
    }
  });
}
