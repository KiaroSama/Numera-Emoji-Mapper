/**
 * The slice of the Bot API this Worker needs.
 *
 * Deliberately tiny. The Python engine keeps the hard part -- creating and
 * adding to sticker sets, with the verified-retry machinery that stops an
 * ambiguous upload becoming a duplicate. Nothing here mutates a pack; this
 * Worker only reads updates and sends messages, which are cheap to get right.
 */

import { redact } from "./redact";

const DEFAULT_API = "https://api.telegram.org";

/**
 * Telegram rejects a sendMessage over this many characters -- counted AFTER
 * entity parsing, so `&amp;` is one character and `<b>x</b>` is one, not five
 * and eight. It lives here with the rest of the Bot API's facts because both
 * the renderer and the /publish validator have to agree on it.
 */
export const TEXT_LIMIT = 4096;

function isRecord(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}

export class BotApiError extends Error {
  constructor(readonly method: string, readonly description: string,
              readonly code?: number) {
    // The token is never part of the message: this string ends up in logs.
    super(`${method} failed (${code ?? "?"}): ${description}`);
    this.name = "BotApiError";
  }
}

export class Telegram {
  constructor(private readonly token: string,
              private readonly apiBase: string = DEFAULT_API) {}

  /**
   * POST one Bot API method.
   *
   * NOT retried. Every call this Worker makes is a sendMessage, which is not
   * idempotent and carries no dedup key, so a timeout after Telegram accepted
   * the message is indistinguishable from one before it -- retrying turns a
   * blip into two identical posts in the channel. The same reasoning is
   * written into the Python client, and this is the one place it could
   * silently diverge.
   */
  async call<T = unknown>(method: string, payload: Record<string, unknown>): Promise<T> {
    let res: Response;
    try {
      res = await fetch(`${this.apiBase}/bot${this.token}/${method}`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
      });
    } catch (err) {
      // The URL embeds the token and a transport failure names the URL it was
      // attempting, so this message reached the HTTP response body and every
      // log sink with the credential still in it. The cause is dropped rather
      // than attached: nothing downstream needs it, and an unsanitised cause
      // is exactly what a thorough serialiser would surface again.
      throw new BotApiError(method, `transport failure: ${redact(String(
        err instanceof Error ? err.message : err))}`, 0);
    }
    let body: unknown;
    try {
      body = await res.json();
    } catch {
      throw new BotApiError(method, `non-JSON response (HTTP ${res.status})`, res.status);
    }
    if (!isRecord(body) || typeof body.ok !== "boolean"
        || (body.ok && !("result" in body))) {
      throw new BotApiError(method, `invalid response (HTTP ${res.status})`, res.status);
    }
    if (!res.ok || !body.ok) {
      // Telegram's own description is upstream text: redacted too, because it
      // can quote back something we sent it.
      const description = typeof body.description === "string" ? body.description : `HTTP ${res.status}`;
      const code = typeof body.error_code === "number" && Number.isSafeInteger(body.error_code)
        ? body.error_code : res.status;
      throw new BotApiError(method, redact(description), code);
    }
    return body.result as T;
  }

  /**
   * Resolve custom-emoji ids to their stickers.
   *
   * The reverse lookup needs this rather than rendering an id straight into a
   * <tg-emoji> tag: Telegram falls back to the placeholder glyph for an id
   * that does not exist, so a typo would come back looking exactly like a
   * success. It also yields each sticker's own emoji, which is a far better
   * fallback than a fixed star wherever the tag cannot render -- notification
   * previews, copied-out text, older clients.
   *
   * Telegram caps one call at 200 ids; callers chunk.
   */
  async getCustomEmojiStickers(ids: string[]): Promise<TgCustomEmojiSticker[]> {
    const result = await this.call("getCustomEmojiStickers", { custom_emoji_ids: ids });
    if (!Array.isArray(result) || !result.every(sticker => isRecord(sticker)
        && typeof sticker.custom_emoji_id === "string"
        && (sticker.emoji === undefined || typeof sticker.emoji === "string"))) {
      throw new BotApiError("getCustomEmojiStickers", "invalid getCustomEmojiStickers result");
    }
    return result as TgCustomEmojiSticker[];
  }

  async sendMessage(chatId: number | string, text: string,
                    opts: Record<string, unknown> = {}): Promise<TgSentMessage> {
    const result = await this.call("sendMessage", {
      chat_id: chatId,
      text,
      // Long id lists are the point of this bot; a link preview per message
      // would bury them.
      disable_web_page_preview: true,
      ...opts,
    });
    // A TypeScript cast cannot prove a message was sent. An absent identifier
    // used to become null in /publish's successful message_ids response.
    // Zero remains valid for messages Telegram has scheduled for delivery.
    if (!isRecord(result) || typeof result.message_id !== "number"
        || !Number.isSafeInteger(result.message_id) || result.message_id < 0
        || !isRecord(result.chat) || !Number.isSafeInteger(result.chat.id)) {
      throw new BotApiError("sendMessage", "invalid sendMessage result");
    }
    return result as unknown as TgSentMessage;
  }
}

export interface TgCustomEmojiSticker {
  custom_emoji_id: string;
  emoji?: string;
}

export interface TgSentMessage {
  message_id: number;
  chat: { id: number };
}

/** Escape text before it goes into a parse_mode=HTML message. */
export function escapeHtml(s: string): string {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
}
