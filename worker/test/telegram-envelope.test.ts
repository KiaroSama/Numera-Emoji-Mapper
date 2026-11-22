/** API JSON must prove success before a publish can be recorded as sent. */
import { afterEach, describe, expect, it, vi } from "vitest";
import worker from "../src/index";
import { BotApiError, Telegram } from "../src/telegram";
import type { Env } from "../src/types";

const ENV: Env = {
  GENERAL_BOT_TOKEN: "111:fixture", COIN_BOT_TOKEN: "222:fixture",
  GENERAL_WEBHOOK_SECRET: "general-fixture", COIN_WEBHOOK_SECRET: "coin-fixture",
  PUBLISH_SECRET: "publish-fixture", ADMIN_USER_IDS: "42", PACK_LINKS_CHAT_ID: "@fixture",
  TELEGRAM_API_BASE: "https://telegram.invalid",
};
const pending: Promise<unknown>[] = [];
const ctx = { waitUntil: (p: Promise<unknown>) => pending.push(p) } as unknown as ExecutionContext;

afterEach(async () => {
  await Promise.allSettled(pending.splice(0));
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe("Telegram response envelopes", () => {
  it.each([null, [], "unavailable", true, {}, { ok: "true" }, { ok: true }].map(body => ({ body })))
    ("reports malformed JSON shape $body as an API error", async ({ body }) => {
      const transport = vi.fn(async () => Response.json(body));
      vi.stubGlobal("fetch", transport);
      const error = await new Telegram("111:fixture", ENV.TELEGRAM_API_BASE)
        .call("sendMessage", { chat_id: 42, text: "hello" }).catch(err => err);

      expect(error).toBeInstanceOf(BotApiError);
      expect((error as Error).message).toContain("invalid response");
      expect(transport).toHaveBeenCalledTimes(1);
    });

  it.each([
    null, [], true, {}, { message_id: "7", chat: { id: 42 } },
    { message_id: -1, chat: { id: 42 } }, { message_id: 1.5, chat: { id: 42 } },
    { message_id: 9007199254740992, chat: { id: 42 } },
    { message_id: 7 }, { message_id: 7, chat: null }, { message_id: 7, chat: { id: "42" } },
  ].map(result => ({ result })))("never accepts a malformed sent Message result $result", async ({ result }) => {
    const transport = vi.fn(async () => Response.json({ ok: true, result }));
    vi.stubGlobal("fetch", transport);
    const error = await new Telegram("111:fixture", ENV.TELEGRAM_API_BASE)
      .sendMessage(42, "hello").catch(err => err);

    expect(error).toBeInstanceOf(BotApiError);
    expect(error.message).toContain("invalid sendMessage result");
    expect(transport).toHaveBeenCalledTimes(1);
  });

  it("accepts a valid scheduled Message whose identifier is zero", async () => {
    const result = { message_id: 0, date: 1795348800, chat: { id: -1004211401345, type: "channel" } };
    vi.stubGlobal("fetch", async () => Response.json({ ok: true, result }));
    await expect(new Telegram("111:fixture", ENV.TELEGRAM_API_BASE).sendMessage("@fixture", "hello"))
      .resolves.toEqual(result);
  });

  it.each([null, {}, "unavailable", [null], [{}], [{ custom_emoji_id: 123 }],
    [{ custom_emoji_id: "5893098741073715471", emoji: 42 }]].map(result => ({ result })))
    ("does not interpret malformed sticker result $result as unknown emoji", async ({ result }) => {
      vi.stubGlobal("fetch", async () => Response.json({ ok: true, result }));
      await expect(new Telegram("111:fixture", ENV.TELEGRAM_API_BASE)
        .getCustomEmojiStickers(["5893098741073715471"]))
        .rejects.toThrow("invalid getCustomEmojiStickers result");
    });

  it("preserves an empty lookup result and optional emoji fallbacks", async () => {
    const sticker = {
      file_id: "fixture-file", file_unique_id: "fixture-unique", type: "custom_emoji",
      width: 100, height: 100, is_animated: false, is_video: false,
      custom_emoji_id: "5893098741073715471",
    };
    for (const result of [[], [sticker], [{ ...sticker, emoji: "⭐" }]]) {
      vi.stubGlobal("fetch", async () => Response.json({ ok: true, result }));
      await expect(new Telegram("111:fixture", ENV.TELEGRAM_API_BASE)
        .getCustomEmojiStickers(["5893098741073715471"]))
        .resolves.toEqual(result);
    }
  });
});

describe("malformed API success is never published as success", () => {
  it.each(["general", "coin"])("returns 502 for %s and records no successful publish", async bot => {
    const outgoing = vi.fn(async () => Response.json({ ok: true, result: {} }));
    vi.stubGlobal("fetch", outgoing);
    const output = vi.spyOn(console, "log").mockImplementation(() => {});
    const response = await worker.fetch(new Request("https://worker.invalid/publish", {
      method: "POST", headers: { Authorization: `Bearer ${ENV.PUBLISH_SECRET}` },
      body: JSON.stringify({ bot, packs: [{ name: "fixture_by_bot", title: "Fixture" }] }),
    }), ENV, ctx);
    await Promise.allSettled(pending.splice(0));

    expect(response.status).toBe(502);
    expect(await response.json()).toMatchObject({ ok: false });
    expect(outgoing).toHaveBeenCalledTimes(1);
    const logs = output.mock.calls.flat().join("\n");
    expect(logs).toContain("ERROR publish");
    expect(logs).not.toContain("INFO publish");
  });

  it.each(["general", "coin"])("does not tell a %s user that a malformed lookup proves an unknown id", async bot => {
    const calls: string[] = [];
    vi.stubGlobal("fetch", async (url: string) => {
      calls.push(String(url).split("/").pop() ?? "");
      return Response.json({ ok: true, result: null });
    });
    const response = await worker.fetch(new Request(`https://worker.invalid/tg/${bot}`, {
      method: "POST", headers: { "X-Telegram-Bot-Api-Secret-Token": `${bot}-fixture` },
      body: JSON.stringify({ update_id: 12, message: {
        message_id: 5, date: 1795348800, from: { id: 42, is_bot: false, first_name: "Fixture" },
        chat: { id: 42, type: "private" }, text: "5893098741073715471",
      } }),
    }), ENV, ctx);

    expect(response.status).toBe(200);
    expect(calls).toEqual(["getCustomEmojiStickers"]);
  });
});
