/** Every configured log-channel address must suppress its incoming echoes. */
import { afterEach, describe, expect, it, vi } from "vitest";
import worker from "../src/index";
import type { Env } from "../src/types";

const ENV: Env = {
  GENERAL_BOT_TOKEN: "111:fixture", COIN_BOT_TOKEN: "222:fixture",
  GENERAL_WEBHOOK_SECRET: "general-fixture", COIN_WEBHOOK_SECRET: "coin-fixture",
  PUBLISH_SECRET: "publish-fixture", ADMIN_USER_IDS: "42,77", PACK_LINKS_CHAT_ID: "@fixture",
  BOT_ALLOWED_CHANNEL_IDS: "-1004211401345",
  TELEGRAM_API_BASE: "https://telegram.invalid",
};
const pending: Promise<unknown>[] = [];
const ctx = { waitUntil: (p: Promise<unknown>) => pending.push(p) } as unknown as ExecutionContext;

afterEach(async () => {
  await Promise.allSettled(pending.splice(0));
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe.each(["general", "coin"])("%s log-channel echoes", bot => {
  it.each(["-1004211401345", " 4211401345 ", "@FixtureLogs", " @fixturelogs "])
    ("ignores the log channel configured as %j even when it is allowlisted", async LOG_CHAT_ID => {
      const outgoing = vi.fn(async () => Response.json({
        ok: true, result: { message_id: 1, chat: { id: 42 } },
      }));
      vi.stubGlobal("fetch", outgoing);
      const consoleLog = vi.spyOn(console, "log").mockImplementation(() => {});
      const response = await worker.fetch(new Request(`https://worker.invalid/tg/${bot}`, {
        method: "POST", headers: { "X-Telegram-Bot-Api-Secret-Token": `${bot}-fixture` },
        body: JSON.stringify({
          update_id: 10,
          channel_post: {
            message_id: 5, date: 1795348800,
            chat: { id: -1004211401345, type: "channel", title: "Logs", username: "FixtureLogs" },
            text: "⭐",
            entities: [{ type: "custom_emoji", offset: 0, length: 1,
                         custom_emoji_id: "5893098741073715471" }],
          },
        }),
      }), { ...ENV, LOG_CHAT_ID }, ctx);
      await Promise.allSettled(pending.splice(0));

      expect(response.status).toBe(200);
      expect(outgoing).not.toHaveBeenCalled();
      expect(consoleLog).not.toHaveBeenCalled();
    });

  it("still answers an allowed channel with a different username", async () => {
    const sent: Record<string, unknown>[] = [];
    vi.stubGlobal("fetch", async (_url: string, init: RequestInit) => {
      sent.push(JSON.parse(String(init.body)));
      return Response.json({ ok: true, result: { message_id: 1, chat: { id: 42 } } });
    });
    const response = await worker.fetch(new Request(`https://worker.invalid/tg/${bot}`, {
      method: "POST", headers: { "X-Telegram-Bot-Api-Secret-Token": `${bot}-fixture` },
      body: JSON.stringify({
        update_id: 11,
        channel_post: {
          message_id: 6, date: 1795348800,
          chat: { id: -1004211401345, type: "channel", title: "Packs", username: "FixturePacks" },
          text: "⭐",
          entities: [{ type: "custom_emoji", offset: 0, length: 1,
                       custom_emoji_id: "5893098741073715471" }],
        },
      }),
    }), { ...ENV, LOG_CHAT_ID: "@FixtureLogs" }, ctx);

    expect(response.status).toBe(200);
    expect(sent.map(message => message.chat_id)).toEqual([42, 77]);
    for (const message of sent) expect(message.text).toContain("5893098741073715471");
  });
});
