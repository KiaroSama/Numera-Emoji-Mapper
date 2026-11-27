/**
 * One Worker, both bots.
 *
 * Routes:
 *   POST /tg/general   Telegram webhook for @YourEmojiBot
 *   POST /tg/coin      Telegram webhook for @YourCoinEmojiBot
 *   POST /publish      Local publisher announcing finished packs
 *   POST /log-test     Authenticated log-channel connectivity check
 *   GET  /health       Liveness and boolean sink readiness (no secrets)
 */

import { parseAdmins, parseChannels, verifyBearer, verifyWebhook } from "./auth";
import { announce, handleUpdateDetailed, LOG_ECHO } from "./handle";
import { log, normalizeChatId, sendLogChannelDirect } from "./logging";
import { Telegram } from "./telegram";
import { errText } from "./redact";
import { validatePublishRequest } from "./validate";
import type { BotName, Env, TgUpdate } from "./types";

/** Telegram retries any non-2xx, so failures must be deliberate. */
const OK = () => new Response("ok");

function botConfig(env: Env, bot: BotName): { token: string; secret: string } {
  return bot === "general"
    ? { token: env.GENERAL_BOT_TOKEN, secret: env.GENERAL_WEBHOOK_SECRET }
    : { token: env.COIN_BOT_TOKEN, secret: env.COIN_WEBHOOK_SECRET };
}

function isUpdateEnvelope(value: unknown): value is TgUpdate {
  if (typeof value !== "object" || value === null || Array.isArray(value)) return false;
  const id = (value as Record<string, unknown>).update_id;
  return typeof id === "number" && Number.isSafeInteger(id) && id >= 0;
}

async function onWebhook(request: Request, env: Env, ctx: ExecutionContext,
                         bot: BotName): Promise<Response> {
  const { token, secret } = botConfig(env, bot);
  if (!verifyWebhook(request, secret)) {
    // Keep external webhook-secret mismatches out of D1 and the Telegram channel
    // so internet scanners cannot flood the channel or spend daily D1 writes.
    ctx.waitUntil(log(env, {
      bot,
      level: "WARNING",
      event: "unauthorized",
      detail: "webhook secret did not match",
      store: false,
      toChannel: false,
    }));
    return new Response("unauthorized", { status: 401 });
  }
  if (!token) {
    ctx.waitUntil(log(env, {
      bot,
      level: "ERROR",
      event: "misconfigured",
      detail: "no bot token bound",
    }));
    return OK();
  }

  let update: unknown;
  try {
    update = await request.json();
  } catch {
    return new Response("bad request", { status: 400 });
  }

  if (!isUpdateEnvelope(update)) {
    return new Response("bad request", { status: 400 });
  }

  const tg = new Telegram(token, env.TELEGRAM_API_BASE);
  const admins = parseAdmins(env.ADMIN_USER_IDS);
  try {
    const res = await handleUpdateDetailed(
      tg,
      update,
      admins,
      env.LOG_CHAT_ID,
      parseChannels(env.BOT_ALLOWED_CHANNEL_IDS),
    );
    if (res.outcome !== LOG_ECHO) {
      // Routine webhook handling stays out of D1 to protect daily write quotas,
      // while important events (denials, lookups, greetings, channel forwards)
      // flow to the Telegram log channel through `isImportantLogEvent`.
      ctx.waitUntil(log(env, {
        bot,
        level: res.level,
        event: res.event,
        detail: `update ${update.update_id}: ${res.outcome}`,
        fields: res.fields,
        store: false,
      }));
    }
  } catch (err) {
    ctx.waitUntil(log(env, {
      bot,
      level: "ERROR",
      event: "webhook",
      detail: `update ${update.update_id}: ${errText(err, env)}`,
      fields: { update_id: update.update_id },
    }));
  }
  return OK();
}

async function onPublish(request: Request, env: Env,
                         ctx: ExecutionContext): Promise<Response> {
  if (!verifyBearer(request, env.PUBLISH_SECRET)) {
    return new Response("unauthorized", {
      status: 401,
      headers: { "WWW-Authenticate": "Bearer" },
    });
  }
  let raw: unknown;
  try {
    raw = await request.json();
  } catch {
    return new Response("bad request", { status: 400 });
  }
  const checked = validatePublishRequest(raw);
  if (!checked.ok) {
    return Response.json({ ok: false, error: checked.error }, { status: 400 });
  }
  const body = checked.value;
  const bot: BotName = body.bot ?? "coin";
  const { token } = botConfig(env, bot);
  if (!token) {
    return Response.json({ ok: false, error: `${bot} bot not configured` }, { status: 500 });
  }
  const chat = normalizeChatId(env.PACK_LINKS_CHAT_ID);
  if (!chat) {
    return Response.json({ ok: false, error: "PACK_LINKS_CHAT_ID not set" }, { status: 500 });
  }

  const tg = new Telegram(token, env.TELEGRAM_API_BASE);
  try {
    const messageIds = await announce(tg, chat, body);
    const packNames = body.packs.map((p) => p.name).join(", ");
    ctx.waitUntil(log(env, {
      bot,
      level: "INFO",
      event: "publish",
      store: true,
      toChannel: true,
      detail: `${body.packs.length} pack(s) in ${messageIds.length} message(s): ${packNames}`,
      fields: {
        packs: body.packs.length,
        messages: messageIds.length,
        style: body.style ?? "cards",
        pack_names: packNames,
      },
    }));
    return Response.json({ ok: true, message_ids: messageIds });
  } catch (err) {
    const msg = errText(err, env);
    ctx.waitUntil(log(env, {
      bot,
      level: "ERROR",
      event: "publish",
      detail: msg,
      fields: { packs: body.packs.length, style: body.style ?? "cards" },
    }));
    return Response.json({ ok: false, error: msg }, { status: 502 });
  }
}

async function onLogTest(request: Request, env: Env): Promise<Response> {
  if (!verifyBearer(request, env.PUBLISH_SECRET)) {
    return new Response("unauthorized", {
      status: 401,
      headers: { "WWW-Authenticate": "Bearer" },
    });
  }
  let bot: BotName = "general";
  let note = "Manual connectivity check";
  const text = await request.text();
  if (text.trim()) {
    let parsed: unknown;
    try {
      parsed = JSON.parse(text);
    } catch {
      return new Response("bad request", { status: 400 });
    }
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
      return Response.json({ ok: false, error: "body must be a JSON object" }, { status: 400 });
    }
    const obj = parsed as Record<string, unknown>;
    if (obj.bot !== undefined) {
      if (obj.bot !== "general" && obj.bot !== "coin") {
        return Response.json({ ok: false, error: "bot must be 'general' or 'coin'" }, { status: 400 });
      }
      bot = obj.bot;
    }
    if (obj.note !== undefined) {
      if (typeof obj.note !== "string" || obj.note.length > 200) {
        return Response.json({ ok: false, error: "note must be a string up to 200 chars" }, { status: 400 });
      }
      if (obj.note.trim()) note = obj.note.trim();
    }
  }
  if (!normalizeChatId(env.LOG_CHAT_ID)) {
    return Response.json({ ok: false, error: "LOG_CHAT_ID not set" }, { status: 500 });
  }
  const { token } = botConfig(env, bot);
  if (!token) {
    return Response.json({ ok: false, error: `${bot} bot not configured` }, { status: 500 });
  }
  try {
    const messageId = await sendLogChannelDirect(env, {
      bot,
      level: "INFO",
      event: "log_channel.test",
      fields: {
        status: "ok",
        pack_links: Boolean(normalizeChatId(env.PACK_LINKS_CHAT_ID)),
        detail: note,
      },
    });
    return Response.json({ ok: true, bot, message_id: messageId });
  } catch (err) {
    return Response.json({ ok: false, error: errText(err, env) }, { status: 502 });
  }
}

export default {
  async fetch(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
    const url = new URL(request.url);
    if (request.method === "GET" && url.pathname === "/health") {
      return Response.json({
        ok: true,
        log_db: Boolean(env.DB),
        log_channel: Boolean(normalizeChatId(env.LOG_CHAT_ID)),
        pack_links: Boolean(normalizeChatId(env.PACK_LINKS_CHAT_ID)),
      });
    }
    if (request.method !== "POST") {
      return new Response("method not allowed", { status: 405 });
    }
    switch (url.pathname) {
      case "/tg/general": return onWebhook(request, env, ctx, "general");
      case "/tg/coin":    return onWebhook(request, env, ctx, "coin");
      case "/publish":    return onPublish(request, env, ctx);
      case "/log-test":   return onLogTest(request, env);
      default:            return new Response("not found", { status: 404 });
    }
  },
};
