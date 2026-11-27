/** Real request routing with synthetic Telegram/D1 sinks only. */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import worker from "../src/index";
import type { Env } from "../src/types";

const LOG_CHAT = { id: -1004211401345, type: "channel", username: "FixtureLogs" };
const ENV: Env = {
  GENERAL_BOT_TOKEN: "111:general-fixture", COIN_BOT_TOKEN: "222:coin-fixture",
  GENERAL_WEBHOOK_SECRET: "general-fixture", COIN_WEBHOOK_SECRET: "coin-fixture",
  PUBLISH_SECRET: "publish-fixture", ADMIN_USER_IDS: "42,77",
  PACK_LINKS_CHAT_ID: "@FixturePacks", LOG_CHAT_ID: "@FixtureLogs",
  TELEGRAM_API_BASE: "https://telegram.invalid",
};
const pending: Promise<unknown>[] = [];
const ctx = { waitUntil: (promise: Promise<unknown>) => pending.push(promise) } as unknown as ExecutionContext;
const sent: Array<{ url: string; body: Record<string, unknown> }> = [];
let testClock = Date.UTC(2026, 10, 27, 16, 0, 0);

beforeEach(() => {
  sent.length = 0;
  // A fresh real rate-limit window keeps independent cases from masking a send.
  vi.useFakeTimers({ toFake: ["Date"] });
  vi.setSystemTime(testClock += 61_000);
  vi.spyOn(console, "log").mockImplementation(() => {});
  vi.spyOn(console, "error").mockImplementation(() => {});
  vi.stubGlobal("fetch", async (url: string, init: RequestInit) => {
    sent.push({ url: String(url), body: JSON.parse(String(init.body)) });
    return Response.json({ ok: true, result: { message_id: sent.length, chat: LOG_CHAT } });
  });
});
afterEach(async () => {
  await Promise.allSettled(pending.splice(0));
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  vi.useRealTimers();
});

async function deliver(bot: string, kind: string, message: Record<string, unknown>, env = ENV) {
  const response = await worker.fetch(new Request(`https://worker.invalid/tg/${bot}`, {
    method: "POST",
    headers: { "X-Telegram-Bot-Api-Secret-Token": `${bot}-fixture` },
    body: JSON.stringify({ update_id: 20, [kind]: message }),
  }), env, ctx);
  await Promise.allSettled(pending.splice(0));
  return response;
}

function automaticForward() {
  return {
    message_id: 9, date: 1795795200,
    chat: { id: -1009876543210, type: "supergroup" },
    from: { id: 136817688, is_bot: true, first_name: "Channel", username: "Channel_Bot" },
    sender_chat: LOG_CHAT,
    is_automatic_forward: true,
    text: "Synthetic log entry",
  };
}

describe.each(["general", "coin"])("%s linked-discussion echo boundary", bot => {
  describe.each(["message", "edited_message"])("%s", kind => {
    it.each(["-1004211401345", " 4211401345 ", "@FixtureLogs", " @fixturelogs "])
      ("suppresses an automatic forward from log channel %j before any sink", async LOG_CHAT_ID => {
        const prepare = vi.fn(() => { throw new Error("Echo must not touch D1"); });
        const response = await deliver(bot, kind, automaticForward(), {
          ...ENV, LOG_CHAT_ID, DB: { prepare } as unknown as D1Database,
        });
        expect(response.status).toBe(200);
        expect(sent).toEqual([]);
        expect(prepare).not.toHaveBeenCalled();
        expect(console.log).not.toHaveBeenCalled();
        expect(console.error).not.toHaveBeenCalled();
      });
  });

  it("preserves a denial for an automatic forward from another channel", async () => {
    const response = await deliver(bot, "message", {
      ...automaticForward(), sender_chat: { ...LOG_CHAT, id: -1002222222222, username: "OtherLogs" },
    });
    expect(response.status).toBe(200);
    expect(sent).toHaveLength(1);
    expect(sent[0].body.chat_id).toBe("@FixtureLogs");
    expect(sent[0].body.text).toContain("access.denied");
  });

  it("does not suppress manual forwards or ordinary messages merely naming the log channel", async () => {
    const response = await deliver(bot, "message", {
      ...automaticForward(), is_automatic_forward: undefined,
      from: { id: 42, is_bot: false },
      forward_origin: { type: "channel", chat: LOG_CHAT, message_id: 9, date: 1795795200 },
      text: "⭐", entities: [{ type: "custom_emoji", offset: 0, length: 1, custom_emoji_id: "5893098741073715471" }],
    });
    expect(response.status).toBe(200);
    expect(sent).toHaveLength(2);
    expect(sent[0].body.chat_id).toBe(-1009876543210);
    expect(sent[0].body.text).toContain("5893098741073715471");
    expect(sent[1].body.chat_id).toBe("@FixtureLogs");
    expect(sent[1].body.text).toContain("Emoji IDs extracted");
  });

  it("does not suppress an automatic forward with no source-chat match", async () => {
    const response = await deliver(bot, "message", { ...automaticForward(), sender_chat: undefined });
    expect(response.status).toBe(200);
    expect(sent).toHaveLength(1);
    expect(sent[0].body.text).toContain("access.denied");
  });
});

describe("authenticated log-test boundary", () => {
  function request(body?: string, authorization = `Bearer ${ENV.PUBLISH_SECRET}`) {
    return new Request("https://worker.invalid/log-test", {
      method: "POST", body, headers: { Authorization: authorization },
    });
  }

  it.each(["{", "null", "[]", '"note"', "true", "7", '{"bot":"other"}',
    '{"bot":null}', '{"note":7}', '{"note":null}', JSON.stringify({ note: "x".repeat(201) })])
    ("rejects malformed body %j without any sink", async body => {
      const response = await worker.fetch(request(body), ENV, ctx);
      expect(response.status).toBe(400);
      expect(sent).toEqual([]);
      expect(pending).toEqual([]);
      expect(console.log).not.toHaveBeenCalled();
    });

  it.each([undefined, "", "   ", "{}", '{"note":"   "}'])
    ("uses the documented defaults for %j", async body => {
      const response = await worker.fetch(request(body), ENV, ctx);
      expect(response.status).toBe(200);
      expect(await response.json()).toEqual({ ok: true, bot: "general", message_id: 1 });
      expect(sent).toHaveLength(1);
      expect(sent[0].url).toContain("/bot111:general-fixture/sendMessage");
      expect(sent[0].body.text).toContain("Manual connectivity check");
    });

  it("checks the bearer before attempting to consume a failing request body", async () => {
    const req = request("{}", "Bearer wrong-fixture");
    const text = vi.spyOn(req, "text").mockRejectedValue(new Error("Synthetic body read failure"));
    const response = await worker.fetch(req, ENV, ctx);
    expect(response.status).toBe(401);
    expect(response.headers.get("WWW-Authenticate")).toBe("Bearer");
    expect(text).not.toHaveBeenCalled();
    expect(sent).toEqual([]);
  });

  it("returns a controlled 400 when authenticated request-body consumption fails", async () => {
    const req = request("{}");
    vi.spyOn(req, "text").mockRejectedValue(new Error("Synthetic body read failure"));
    const response = await worker.fetch(req, ENV, ctx);
    expect(response.status).toBe(400);
    expect(await response.text()).toBe("bad request");
    expect(sent).toEqual([]);
    expect(console.log).not.toHaveBeenCalled();
  });

  it.each(["missing-log", "missing-general", "missing-coin"])
    ("fails visibly without sending when configuration is %s", async variant => {
      const env = { ...ENV, ...(variant === "missing-log" ? { LOG_CHAT_ID: "0" }
        : variant === "missing-general" ? { GENERAL_BOT_TOKEN: "" } : { COIN_BOT_TOKEN: "" }) };
      const response = await worker.fetch(request(JSON.stringify({ bot: variant === "missing-coin" ? "coin" : "general" })), env, ctx);
      expect(response.status).toBe(500);
      expect(await response.json()).toMatchObject({ ok: false });
      expect(sent).toEqual([]);
    });

  it.each(["malformed-success", "api-denied", "transport"])
    ("reports a %s delivery failure once without retry", async variant => {
      const transport = vi.fn(async () => {
        if (variant === "transport") throw new Error(`Synthetic error ${ENV.PUBLISH_SECRET}`);
        return Response.json(variant === "api-denied"
          ? { ok: false, description: `Synthetic error ${ENV.PUBLISH_SECRET}`, error_code: 429 }
          : { ok: true, result: {} });
      });
      vi.stubGlobal("fetch", transport);
      const response = await worker.fetch(request('{"bot":"coin"}'), ENV, ctx);
      expect(response.status).toBe(502);
      const body = await response.text();
      expect(body).not.toContain(ENV.PUBLISH_SECRET);
      expect(JSON.parse(body)).toMatchObject({ ok: false });
      expect(transport).toHaveBeenCalledTimes(1);
      expect(pending).toEqual([]);
    });
});
