/**
 * Numera Group Bot HTML log-channel contract:
 * - Six colored orbs (🔴🟠🟡🟢🔵🟣) with zero satellite glyphs
 * - Structured `key: <code>value</code>` lines with folded `(@username)` handles
 * - Raw user message text fields stripped
 * - Active business-event routing, channel ID normalization, and /log-test
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { parseChannels } from "../src/auth";
import worker from "../src/index";
import { formatLine, isImportantLogEvent, logChannelEmoji } from "../src/logging";
import type { Env } from "../src/types";

const ENV: Env = {
  GENERAL_BOT_TOKEN: "111:general-fixture",
  COIN_BOT_TOKEN: "222:coin-fixture",
  GENERAL_WEBHOOK_SECRET: "general-fixture",
  COIN_WEBHOOK_SECRET: "coin-fixture",
  PUBLISH_SECRET: "publish-fixture",
  ADMIN_USER_IDS: "42",
  PACK_LINKS_CHAT_ID: "2052830618",
  LOG_CHAT_ID: "1998461602",
  TELEGRAM_API_BASE: "https://telegram.invalid",
};

const pending: Promise<unknown>[] = [];
const CTX = { waitUntil: (p: Promise<unknown>) => pending.push(p) } as unknown as ExecutionContext;

afterEach(async () => {
  await Promise.allSettled(pending.splice(0));
  vi.restoreAllMocks();
});

describe("Numera-style six-orb log channel", () => {
  it("maps every operational state to one of the six circle orbs without satellite glyphs", () => {
    const orbs = new Set(["🔴", "🟠", "🟡", "🟢", "🔵", "🟣"]);
    const cases: Array<["INFO" | "WARNING" | "ERROR" | "CRITICAL", string, string]> = [
      ["ERROR", "webhook", "🔴"],
      ["CRITICAL", "publish", "🔴"],
      ["WARNING", "access.denied", "🟠"],
      ["INFO", "access.denied", "🟠"],
      ["INFO", "bot.greeted", "🟡"],
      ["INFO", "emoji.partial_lookup", "🟡"],
      ["INFO", "publish", "🟢"],
      ["INFO", "log_channel.test", "🟢"],
      ["INFO", "emoji.lookup", "🔵"],
      ["INFO", "emoji.reverse_lookup", "🔵"],
      ["INFO", "channel.forwarded", "🟣"],
      ["INFO", "settings.changed", "🟣"],
    ];
    for (const [level, event, expected] of cases) {
      const got = logChannelEmoji(level, event);
      expect(orbs.has(got)).toBe(true);
      expect(got).toBe(expected);
    }
    const rendered = cases.map(([level, event]) => formatLine({ bot: "general", level, event }, 0)).join("\n");
    expect(rendered).not.toMatch(/[❌⚠️ℹ️]/u);
  });

  it("folds companion @username annotations onto numeric IDs and strips raw message text", () => {
    const line = formatLine({
      bot: "general",
      level: "WARNING",
      event: "access.denied",
      fields: {
        user_id: 987654321,
        username: "operator_handle",
        chat_type: "private",
        text: "secret user message should never appear",
        caption: "secret caption should never appear",
      },
    }, Date.UTC(2026, 10, 27, 12, 0, 0));

    expect(line).toContain("<b>🟠 WARNING · access.denied</b>");
    expect(line).toContain("bot: <code>general</code>");
    expect(line).toContain("user_id: <code>987654321</code> (@operator_handle)");
    expect(line).not.toContain("username:");
    expect(line).not.toContain("secret user message");
    expect(line).not.toContain("secret caption");
    expect(line.endsWith("🕒 2026-11-27 12:00:00 UTC")).toBe(true);
  });

  it("routes important INFO business events and WARNING+ to the channel, skipping plain noise", () => {
    expect(isImportantLogEvent("INFO", "publish")).toBe(true);
    expect(isImportantLogEvent("INFO", "emoji.lookup")).toBe(true);
    expect(isImportantLogEvent("INFO", "bot.greeted")).toBe(true);
    expect(isImportantLogEvent("INFO", "channel.forwarded")).toBe(true);
    expect(isImportantLogEvent("INFO", "log_channel.test")).toBe(true);
    expect(isImportantLogEvent("WARNING", "access.denied")).toBe(true);
    expect(isImportantLogEvent("ERROR", "webhook")).toBe(true);
    expect(isImportantLogEvent("INFO", "webhook")).toBe(false);
    expect(isImportantLogEvent("INFO", "webhook.ignored")).toBe(false);
  });

  it("normalizes bare positive channel IDs in BOT_ALLOWED_CHANNEL_IDS and PACK_LINKS_CHAT_ID", async () => {
    expect([...parseChannels("2052830618, -1001111111111; 0; bad")]).toEqual([
      "-1002052830618",
      "-1001111111111",
    ]);

    const sent: Array<Record<string, unknown>> = [];
    vi.stubGlobal("fetch", async (_url: string, init: RequestInit) => {
      sent.push(JSON.parse(String(init.body)));
      return new Response(JSON.stringify({ ok: true, result: { message_id: sent.length, chat: { id: 1 } } }),
                          { headers: { "Content-Type": "application/json" } });
    });

    const res = await worker.fetch(new Request("https://w.dev/publish", {
      method: "POST",
      headers: { Authorization: `Bearer ${ENV.PUBLISH_SECRET}` },
      body: JSON.stringify({
        bot: "general",
        packs: [{ name: "sample_by_bot", title: "Sample", count: 5 }],
      }),
    }), ENV, CTX);
    await Promise.allSettled(pending.splice(0));

    expect(res.status).toBe(200);
    // Message 1 -> Pack Links (normalized from bare 2052830618), Message 2 -> Log Channel (normalized from bare 1998461602)
    expect(sent).toHaveLength(2);
    expect(sent[0].chat_id).toBe("-1002052830618");
    expect(sent[1].chat_id).toBe("-1001998461602");
    expect(sent[1].parse_mode).toBe("HTML");
    expect(String(sent[1].text)).toContain("<b>🟢 Pack announced</b>");
  });

  it("exposes authenticated POST /log-test and boolean sink flags on GET /health", async () => {
    const healthRes = await worker.fetch(new Request("https://w.dev/health"), ENV, CTX);
    expect(healthRes.status).toBe(200);
    expect(await healthRes.json()).toEqual({
      ok: true,
      log_db: false,
      log_channel: true,
      pack_links: true,
    });

    const unauth = await worker.fetch(new Request("https://w.dev/log-test", { method: "POST" }), ENV, CTX);
    expect(unauth.status).toBe(401);

    const sent: Array<Record<string, unknown>> = [];
    vi.stubGlobal("fetch", async (_url: string, init: RequestInit) => {
      sent.push(JSON.parse(String(init.body)));
      return new Response(JSON.stringify({ ok: true, result: { message_id: 77, chat: { id: -1001998461602 } } }),
                          { headers: { "Content-Type": "application/json" } });
    });

    const okRes = await worker.fetch(new Request("https://w.dev/log-test", {
      method: "POST",
      headers: { Authorization: `Bearer ${ENV.PUBLISH_SECRET}` },
      body: JSON.stringify({ bot: "coin", note: "Live check" }),
    }), ENV, CTX);
    expect(okRes.status).toBe(200);
    expect(await okRes.json()).toEqual({ ok: true, bot: "coin", message_id: 77 });
    expect(sent).toHaveLength(1);
    expect(sent[0].chat_id).toBe("-1001998461602");
    expect(sent[0].parse_mode).toBe("HTML");
    expect(String(sent[0].text)).toContain("<b>🟢 Log channel test</b>");
    expect(String(sent[0].text)).toContain("bot: <code>coin</code>");
  });
});
