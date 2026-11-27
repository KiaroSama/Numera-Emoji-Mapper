/**
 * Request authentication and bot allowlists.
 *
 * Two separate gates:
 *   - `verifyWebhook` proves a request came from Telegram.
 *   - `isAdmin` decides whether the sender behind an update gets a reply.
 *
 * Both fail closed. An unset or malformed allowlist means nobody, never
 * everybody, so a configuration typo cannot expose a private bot.
 */

import { normalizeChatId } from "./logging";

/** Constant-time string compare so a wrong secret cannot leak byte by byte. */
function timingSafeEqual(a: string, b: string): boolean {
  const ab = new TextEncoder().encode(a);
  const bb = new TextEncoder().encode(b);
  if (ab.length !== bb.length) return false;
  let diff = 0;
  for (let i = 0; i < ab.length; i++) diff |= ab[i] ^ bb[i];
  return diff === 0;
}

/** True when the request carries the secret Telegram sends on webhook delivery. */
export function verifyWebhook(request: Request, expected: string | undefined): boolean {
  if (!expected) return false;
  const got = request.headers.get("X-Telegram-Bot-Api-Secret-Token");
  return got !== null && timingSafeEqual(got, expected);
}

/** Bearer check for the local publisher and diagnostic endpoints. */
export function verifyBearer(request: Request, expected: string | undefined): boolean {
  if (!expected) return false;
  const header = request.headers.get("Authorization") ?? "";
  const prefix = /^Bearer +/i.exec(header);
  return prefix !== null && timingSafeEqual(header.slice(prefix[0].length), expected);
}

/**
 * Parse the admin allowlist (`ADMIN_USER_IDS`).
 * Accepts commas or semicolons, ignores blanks, and keeps plain positive integers.
 */
export function parseAdmins(raw: string | undefined): Set<number> {
  const out = new Set<number>();
  for (const part of (raw ?? "").replace(/;/g, ",").split(",")) {
    const t = part.trim();
    if (!t) continue;
    if (!/^\d+$/.test(t)) continue;
    const n = Number(t);
    if (Number.isSafeInteger(n) && n > 0) out.add(n);
  }
  return out;
}

/**
 * Parse the channel allowlist (`BOT_ALLOWED_CHANNEL_IDS`).
 * Accepts `-100...` IDs and bare positive channel IDs (normalized to `-100...`),
 * keeping them as strings so long IDs never lose precision. Unset or malformed
 * values resolve to an empty set.
 */
export function parseChannels(raw: string | undefined): Set<string> {
  const out = new Set<string>();
  for (const part of (raw ?? "").replace(/;/g, ",").split(",")) {
    const t = part.trim();
    if (!t || t.startsWith("@")) continue;
    const normalized = normalizeChatId(t);
    if (normalized) out.add(normalized);
  }
  return out;
}

export function isAdmin(userId: number | undefined, admins: Set<number>): boolean {
  return userId !== undefined && admins.has(userId);
}
