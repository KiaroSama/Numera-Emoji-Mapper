/** HTTP auth-scheme syntax is independent of the case-sensitive secret. */
import { afterEach, describe, expect, it, vi } from "vitest";
import worker from "../src/index";
import type { Env } from "../src/types";

const ENV: Env = {
  GENERAL_BOT_TOKEN: "111:fixture", COIN_BOT_TOKEN: "222:fixture",
  GENERAL_WEBHOOK_SECRET: "general-fixture", COIN_WEBHOOK_SECRET: "coin-fixture",
  PUBLISH_SECRET: "Fixture-Publish", ADMIN_USER_IDS: "42", PACK_LINKS_CHAT_ID: "@fixture",
  TELEGRAM_API_BASE: "https://telegram.invalid",
};
const pending: Promise<unknown>[] = [];
const ctx = { waitUntil: (p: Promise<unknown>) => pending.push(p) } as unknown as ExecutionContext;

function request(authorization?: string): Request {
  return new Request("https://worker.invalid/publish", {
    method: "POST", headers: authorization ? { Authorization: authorization } : {},
    body: JSON.stringify({ packs: [{ name: "fixture_by_bot" }] }),
  });
}

afterEach(async () => {
  await Promise.allSettled(pending.splice(0));
  vi.unstubAllGlobals();
});

describe("publish HTTP bearer authentication", () => {
  it.each(["bearer", "BEARER", "bEaReR", "Bearer   "])
    ("accepts the valid auth scheme %j with the exact secret", async scheme => {
      const sends: Record<string, unknown>[] = [];
      vi.stubGlobal("fetch", async (_url: string, init: RequestInit) => {
        sends.push(JSON.parse(String(init.body)));
        return Response.json({ ok: true, result: { message_id: 7, chat: { id: -1004211401345 } } });
      });
      const response = await worker.fetch(request(`${scheme} ${ENV.PUBLISH_SECRET}`), ENV, ctx);

      expect(response.status).toBe(200);
      expect(await response.json()).toEqual({ ok: true, message_ids: [7] });
      expect(sends).toHaveLength(1);
      expect(sends[0].text).toContain("https://t.me/addemoji/fixture_by_bot");
    });

  it.each([undefined, "bearer fixture-publish", "Bearer Fixture-Publish-extra",
    "Basic Fixture-Publish", "BearerFixture-Publish", "Bearer\tFixture-Publish"])
    ("refuses invalid credentials %j and advertises the required scheme", async header => {
      const outgoing = vi.fn(() => { throw new Error("unexpected outbound call"); });
      vi.stubGlobal("fetch", outgoing);
      const response = await worker.fetch(request(header), ENV, ctx);

      expect(response.status).toBe(401);
      expect(response.headers.get("WWW-Authenticate")).toBe("Bearer");
      expect(outgoing).not.toHaveBeenCalled();
    });
});
