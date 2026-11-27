/** Channel recipients must be isolated without retrying non-idempotent sends. */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import worker from "../src/index";
import type { Env } from "../src/types";

const ENV: Env = {
  GENERAL_BOT_TOKEN: "111:general-fixture", COIN_BOT_TOKEN: "222:coin-fixture",
  GENERAL_WEBHOOK_SECRET: "general-fixture", COIN_WEBHOOK_SECRET: "coin-fixture",
  PUBLISH_SECRET: "publish-fixture", ADMIN_USER_IDS: "42,77,99",
  PACK_LINKS_CHAT_ID: "@FixturePacks", BOT_ALLOWED_CHANNEL_IDS: "-100777",
  LOG_CHAT_ID: "@FixtureLogs", TELEGRAM_API_BASE: "https://telegram.invalid",
};
const pending: Promise<unknown>[] = [];
const ctx = { waitUntil: (promise: Promise<unknown>) => pending.push(promise) } as unknown as ExecutionContext;
let testClock = Date.UTC(2026, 10, 27, 18, 0, 0);

type AuditLevel = "INFO" | "WARNING" | "ERROR" | "DEBUG";
function auditLog(level: AuditLevel, message: string): void {
  console.log(`${new Date().toISOString()} [${level}] ${message}`);
}

beforeEach(() => {
  // Every case gets a fresh production log-channel rate-limit window.
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(testClock += 61_000);
});
afterEach(async () => {
  await Promise.allSettled(pending.splice(0));
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  vi.useRealTimers();
});

function sinks(failedAdmins: number[], failPart = 1, transport = false) {
  const sent: Array<Record<string, unknown>> = [];
  const stored: unknown[][] = [];
  const attempts = new Map<unknown, number>();
  vi.stubGlobal("fetch", async (_url: string, init: RequestInit) => {
    const body = JSON.parse(String(init.body)) as Record<string, unknown>;
    sent.push(body);
    const part = (attempts.get(body.chat_id) ?? 0) + 1;
    attempts.set(body.chat_id, part);
    if (failedAdmins.includes(Number(body.chat_id)) && part === failPart) {
      if (transport) throw new Error(`Synthetic transport failure ${ENV.PUBLISH_SECRET}`);
      return Response.json({ ok: false, error_code: 403,
        description: `Forbidden: bot was blocked by the user ${ENV.PUBLISH_SECRET}` });
    }
    return Response.json({ ok: true, result: {
      message_id: sent.length, date: 1795802400,
      chat: typeof body.chat_id === "number"
        ? { id: body.chat_id, type: "private", first_name: "Fixture" }
        : { id: -100888, type: "channel", title: "Fixture logs" },
      text: body.text,
    } });
  });
  const db = { prepare: () => ({ bind: (...values: unknown[]) => ({
    run: async () => { stored.push(values); return { meta: { last_row_id: 1 } }; },
  }) }) } as unknown as D1Database;
  return { sent, stored, db };
}

async function deliver(bot: string, db: D1Database, emojiCount = 1): Promise<Response> {
  const response = await worker.fetch(new Request(`https://worker.invalid/tg/${bot}`, {
    method: "POST",
    headers: { "X-Telegram-Bot-Api-Secret-Token": `${bot}-fixture` },
    body: JSON.stringify({ update_id: 20, channel_post: {
      message_id: 9, date: 1795802400,
      chat: { id: -100777, type: "channel", title: "Fixture channel" },
      text: "⭐".repeat(emojiCount), entities: Array.from({ length: emojiCount }, (_, i) => ({
        type: "custom_emoji", offset: i, length: 1,
        custom_emoji_id: String(5893098741073715400n + BigInt(i)),
      })),
    } }),
  }), { ...ENV, DB: db }, ctx);
  await Promise.allSettled(pending.splice(0));
  return response;
}

describe.each(["general", "coin"])("%s channel delivery", bot => {
  it.each([
    { label: "first", failures: [42] },
    { label: "middle", failures: [77] },
    { label: "all", failures: [42, 77, 99] },
  ])("attempts every administrator once when $label recipients refuse delivery", async ({ failures }) => {
    auditLog("INFO", `Checking ${bot} channel delivery with ${failures.length} refused recipient(s)`);
    const output = vi.spyOn(console, "log").mockImplementation(() => {});
    const { sent, stored, db } = sinks(failures);
    const response = await deliver(bot, db);

    expect(response.status).toBe(200);
    expect(sent.map(body => body.chat_id)).toEqual([42, 77, 99, "@FixtureLogs"]);
    expect(stored).toHaveLength(1);
    expect(stored[0][2]).toBe("ERROR");
    expect(stored[0][3]).toBe("webhook");
    expect(String(stored[0][4])).toContain(`${failures.length}/3`);
    const diagnostics = JSON.stringify({ sent, stored, console: output.mock.calls });
    expect(diagnostics).toContain("[REDACTED]");
    expect(diagnostics).not.toContain(ENV.PUBLISH_SECRET);
    expect(diagnostics).not.toContain("Channel emoji forwarded");
  });

  it("continues after an ambiguous transport error without resending that recipient", async () => {
    auditLog("INFO", `Checking ${bot} channel delivery after a transport failure`);
    vi.spyOn(console, "log").mockImplementation(() => {});
    const { sent, stored, db } = sinks([42], 1, true);
    const response = await deliver(bot, db);

    expect(response.status).toBe(200);
    expect(sent.map(body => body.chat_id)).toEqual([42, 77, 99, "@FixtureLogs"]);
    expect(stored).toHaveLength(1);
    expect(JSON.stringify(stored)).toContain("transport failure");
    expect(JSON.stringify({ sent, stored })).not.toContain(ENV.PUBLISH_SECRET);
  });

  it("does not replay a delivered chunk when a later chunk fails for one administrator", async () => {
    auditLog("INFO", `Checking ${bot} partial multi-message channel delivery`);
    vi.spyOn(console, "log").mockImplementation(() => {});
    const { sent, stored, db } = sinks([42], 2);
    const response = await deliver(bot, db, 60);

    expect(response.status).toBe(200);
    expect(sent.map(body => body.chat_id)).toEqual([42, 42, 77, 77, 99, 99, "@FixtureLogs"]);
    expect(stored).toHaveLength(1);
    expect(String(stored[0][4])).toContain("1/3");
  });

  it("keeps a successful fan-out as one business event with no D1 error", async () => {
    auditLog("INFO", `Checking ${bot} successful channel delivery`);
    vi.spyOn(console, "log").mockImplementation(() => {});
    const { sent, stored, db } = sinks([]);
    const response = await deliver(bot, db);

    expect(response.status).toBe(200);
    expect(sent.map(body => body.chat_id)).toEqual([42, 77, 99, "@FixtureLogs"]);
    expect(stored).toEqual([]);
    expect(String(sent[3].text)).toContain("Channel emoji forwarded");
  });
});
