/**
 * Update handling for both bots and pack announcement rendering.
 *
 * Both bots share this handler so a message sent to either bot gets a clear
 * response instead of silence.
 */

import { escapeHtml, Telegram, TEXT_LIMIT } from "./telegram";
import { extractCustomEmojiIds, parseIdList } from "./emoji";
import { isAdmin } from "./auth";
import { type LogLevel, normalizeChatId } from "./logging";
import type { PublishRequest, TgChat, TgMessage, TgUpdate } from "./types";

const DENIED_TEXT =
  "This bot is private and your account is not on its access list.\n" +
  "Ask the owner to add your numeric Telegram user ID if you need access.";

const START_TEXT =
  "Send a message with <b>premium (custom) emoji</b> and I will reply with their IDs.\n\n" +
  "This works with emoji in plain text, in a caption, or inside a quoted reply.\n\n" +
  "You can also <b>send emoji IDs</b> (one per line, comma-separated, or a single ID) " +
  "and I will show the matching emoji.";

/** Our log-channel post returned directly or through its automatic discussion mirror. */
export const LOG_ECHO = "log-echo";

export interface HandleResult {
  outcome: string;
  event: string;
  level: LogLevel;
  fields: Record<string, unknown>;
}

/** Split IDs into HTML messages that stay under Telegram's character limit. */
export function renderIdMessages(ids: string[], header?: string,
                                 glyphs?: Map<string, string>): string[] {
  if (ids.length === 0) {
    return [header ? `${header}\nNo premium emoji found in that message.`
                   : "No premium emoji found in that message."];
  }
  const out: string[] = [];
  let buf = header ? `${header}\n` : "";
  for (const id of ids) {
    const glyph = escapeHtml(glyphs?.get(id) ?? "⭐");
    const line = `<tg-emoji emoji-id="${id}">${glyph}</tg-emoji> <code>${id}</code>\n`;
    if (buf.length + line.length > TEXT_LIMIT) {
      out.push(buf.trimEnd());
      buf = "";
    }
    buf += line;
  }
  if (buf.trim()) out.push(buf.trimEnd());
  return out;
}

async function reply(tg: Telegram, chatId: number | string, ids: string[],
                     opts: { replyTo?: number; header?: string } = {}): Promise<void> {
  const parts = renderIdMessages(ids, opts.header);
  for (const [i, text] of parts.entries()) {
    await tg.sendMessage(chatId, text, {
      parse_mode: "HTML",
      ...(i === 0 && opts.replyTo ? { reply_to_message_id: opts.replyTo } : {}),
    });
  }
}

/** Telegram resolves at most 200 custom-emoji IDs per call. */
const RESOLVE_CHUNK = 200;

async function replyToTypedIds(tg: Telegram, chatId: number | string,
                               ids: string[], replyTo?: number): Promise<{ known: number; total: number }> {
  const glyphs = new Map<string, string>();
  for (let i = 0; i < ids.length; i += RESOLVE_CHUNK) {
    const found = await tg.getCustomEmojiStickers(ids.slice(i, i + RESOLVE_CHUNK));
    for (const st of found ?? []) {
      if (st?.custom_emoji_id) glyphs.set(st.custom_emoji_id, st.emoji ?? "⭐");
    }
  }
  const known = ids.filter((id) => glyphs.has(id));
  const unknown = ids.filter((id) => !glyphs.has(id));
  let header: string | undefined;
  if (unknown.length) {
    const shown = unknown.slice(0, 10).map((id) => `<code>${id}</code>`).join(", ");
    const more = unknown.length > 10 ? ` (+${unknown.length - 10} more)` : "";
    header = `Telegram does not know: ${shown}${more}`;
  }
  if (known.length === 0) {
    await tg.sendMessage(chatId, header ?? "No usable IDs found in that message.", {
      parse_mode: "HTML",
      ...(replyTo ? { reply_to_message_id: replyTo } : {}),
    });
    return { known: 0, total: ids.length };
  }
  const parts = renderIdMessages(known, header, glyphs);
  for (const [i, text] of parts.entries()) {
    await tg.sendMessage(chatId, text, {
      parse_mode: "HTML",
      ...(i === 0 && replyTo ? { reply_to_message_id: replyTo } : {}),
    });
  }
  return { known: known.length, total: ids.length };
}

function userFields(msg: TgMessage): Record<string, unknown> {
  const fields: Record<string, unknown> = { chat_type: msg.chat.type };
  if (msg.from?.id !== undefined) fields.user_id = msg.from.id;
  if (msg.from?.username) fields.username = msg.from.username;
  if (msg.chat.type !== "private") {
    fields.chat_id = msg.chat.id;
    if (msg.chat.username) fields.chat_username = msg.chat.username;
  }
  return fields;
}

/** Match either supported configured address without trusting message text. */
function isLogChannel(chat: TgChat | undefined, logChatId: string | undefined): boolean {
  const logChat = normalizeChatId(logChatId);
  return Boolean(chat && logChat && (String(chat.id) === logChat
    || (chat.username && logChat.toLowerCase() === `@${chat.username.toLowerCase()}`)));
}

/**
 * Handle one update and return structured non-text metadata for logging.
 * Never includes tokens or raw user message text.
 */
export async function handleUpdateDetailed(
  tg: Telegram,
  update: TgUpdate,
  admins: Set<number>,
  logChatId?: string,
  channels: Set<string> = new Set(),
): Promise<HandleResult> {
  const msg: TgMessage | undefined = update.message ?? update.edited_message;
  if (msg) {
    // Telegram mirrors channel posts into linked discussion groups. Logging a
    // denial for that automatic mirror would send our log back into its source.
    if (msg.is_automatic_forward === true && msg.sender_chat?.type === "channel"
        && isLogChannel(msg.sender_chat, logChatId)) {
      return { outcome: LOG_ECHO, event: "log.echo", level: "INFO", fields: {} };
    }
    const sender = msg.from?.id;
    const base = userFields(msg);
    if (!isAdmin(sender, admins)) {
      if (msg.chat.type === "private") {
        try {
          await tg.sendMessage(msg.chat.id, DENIED_TEXT);
        } catch {
          // Best-effort denial; do not force Telegram to retry the update.
        }
      }
      return {
        outcome: `denied:${msg.chat.type}`,
        event: "access.denied",
        level: "WARNING",
        fields: base,
      };
    }
    const text = msg.text ?? "";
    if (/^\/(start|help|menu)\b/.test(text)) {
      await tg.sendMessage(msg.chat.id, START_TEXT, { parse_mode: "HTML" });
      return {
        outcome: "greeted",
        event: "bot.greeted",
        level: "INFO",
        fields: base,
      };
    }
    const typed = parseIdList(text);
    if (typed.length) {
      const { known, total } = await replyToTypedIds(tg, msg.chat.id, typed, msg.message_id);
      const partial = known < total;
      return {
        outcome: `typed:${known}/${total}`,
        event: partial ? "emoji.partial_lookup" : "emoji.reverse_lookup",
        level: "INFO",
        fields: { ...base, resolved: `${known}/${total}` },
      };
    }
    const ids = extractCustomEmojiIds(msg);
    await reply(tg, msg.chat.id, ids, { replyTo: msg.message_id });
    return {
      outcome: `ids:${ids.length}`,
      event: ids.length > 0 ? "emoji.lookup" : "emoji.empty",
      level: "INFO",
      fields: { ...base, count: ids.length },
    };
  }

  const post = update.channel_post;
  if (post) {
    if (isLogChannel(post.chat, logChatId)) {
      return { outcome: LOG_ECHO, event: "log.echo", level: "INFO", fields: {} };
    }
    const chFields: Record<string, unknown> = { chat_id: post.chat.id };
    if (post.chat.username) chFields.chat_username = post.chat.username;
    if (!channels.has(String(post.chat.id))) {
      return {
        outcome: "channel:not-allowed",
        event: "channel.not_allowed",
        level: "INFO",
        fields: chFields,
      };
    }
    const ids = extractCustomEmojiIds(post);
    if (ids.length === 0) {
      return { outcome: "channel:none", event: "channel.none", level: "INFO", fields: chFields };
    }
    const title = escapeHtml(post.chat.title ?? "");
    const failedAdmins: number[] = [];
    let firstFailure: unknown;
    for (const admin of admins) {
      try {
        await reply(tg, admin, ids, { header: `From channel <b>${title}</b>:` });
      } catch (err) {
        // A blocked or unreachable recipient must not discard later deliveries.
        // Never retry: some chunks may already have reached this administrator.
        if (failedAdmins.length === 0) firstFailure = err;
        failedAdmins.push(admin);
      }
    }
    if (failedAdmins.length) {
      // Preserve the webhook's single redacted error report after every
      // independent recipient has been attempted, without logging false success.
      throw new Error(
        `channel forwarding failed for ${failedAdmins.length}/${admins.size} admin(s): ${failedAdmins.join(", ")}`,
        { cause: firstFailure },
      );
    }
    return {
      outcome: `channel:${ids.length}`,
      event: "channel.forwarded",
      level: "INFO",
      fields: { ...chFields, count: ids.length, admins: admins.size },
    };
  }

  return { outcome: "ignored", event: "webhook.ignored", level: "INFO", fields: {} };
}

/** Backwards-compatible wrapper returning only the short outcome string. */
export async function handleUpdate(tg: Telegram, update: TgUpdate,
                                   admins: Set<number>,
                                   logChatId?: string,
                                   channels: Set<string> = new Set()): Promise<string> {
  const res = await handleUpdateDetailed(tg, update, admins, logChatId, channels);
  return res.outcome;
}

const TITLE_LIMIT = 200;

function clampText(s: string, max: number): string {
  if (s.length <= max) return s;
  return s.slice(0, /[\uD800-\uDBFF]/.test(s[max - 1]) ? max - 1 : max);
}

/** Render one or more HTML announcement messages staying under Telegram's limit. */
export function renderAnnouncement(req: PublishRequest): string[] {
  const blocks: string[] = [];
  const list = req.style === "list";
  const note = clampText(String(req.note ?? ""), TEXT_LIMIT);
  if (note.trim()) blocks.push(escapeHtml(note));
  for (const p of req.packs) {
    const title = escapeHtml(clampText(String(p.title ?? p.name), TITLE_LIMIT));
    const count = p.count !== undefined ? ` (${escapeHtml(String(p.count))})` : "";
    const url = `https://t.me/addemoji/${encodeURIComponent(p.name)}`;
    blocks.push(list ? `${title}. ${url}`
                     : `✅ <b>${title}</b>${count}\n${url}`);
  }

  const out: string[] = [];
  let buf = "";
  for (const block of blocks) {
    const candidate = buf ? `${buf}\n${block}` : block;
    if (buf && candidate.length > TEXT_LIMIT) {
      out.push(buf);
      buf = block;
    } else {
      buf = candidate;
    }
  }
  if (buf) out.push(buf);
  return out;
}

/** Post the announcement to the configured channel and return every message ID. */
export async function announce(tg: Telegram, chatId: number | string,
                               req: PublishRequest): Promise<number[]> {
  const ids: number[] = [];
  for (const text of renderAnnouncement(req)) {
    const sent = await tg.sendMessage(chatId, text, { parse_mode: "HTML" });
    ids.push(sent.message_id);
  }
  return ids;
}
