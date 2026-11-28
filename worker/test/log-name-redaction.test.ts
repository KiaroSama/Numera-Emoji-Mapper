/** Display-name presentation must never shorten or normalize a configured credential before redaction. */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import worker from "../src/index";
import { formatLine } from "../src/logging";
import type { BotName, Env } from "../src/types";

const CREDENTIALS = [
  "GENERAL_BOT_TOKEN", "COIN_BOT_TOKEN", "GENERAL_WEBHOOK_SECRET",
  "COIN_WEBHOOK_SECRET", "PUBLISH_SECRET",
] as const;
const ENV: Env = {
  GENERAL_BOT_TOKEN: "111:general-fixture",
  COIN_BOT_TOKEN: "222:coin-fixture",
  GENERAL_WEBHOOK_SECRET: "general-fixture-hook",
  COIN_WEBHOOK_SECRET: "coin-fixture-hook",
  PUBLISH_SECRET: "publish-fixture",
  ADMIN_USER_IDS: "42",
  PACK_LINKS_CHAT_ID: "-100901",
  LOG_CHAT_ID: "-100900",
  TELEGRAM_API_BASE: "https://telegram.invalid",
};
const boundaryCases = CREDENTIALS.flatMap(binding => [
  {
    binding, position: "long name", secret: `${binding}_${"0123456789abcdef".repeat(4)}`,
    prefix: "",
  },
  {
    binding, position: "48-character boundary", secret: "BoundaryCredential0123456789abcdef",
    prefix: "Visible name before secret: ",
  },
]);
const normalizationCases: Array<{ name: string; secret: string; first_name: string; last_name?: string }> = [
  { name: "internal tab", secret: "Fixture\tPublishSecret", first_name: "Fixture\tPublishSecret" },
  { name: "surrounding spaces", secret: " FixturePublishSecret ", first_name: " FixturePublishSecret " },
  { name: "joined name parts", secret: "Fixture\tFirst Last", first_name: "Fixture\tFirst", last_name: "Last" },
];
let clock = Date.UTC(2026, 10, 28);

beforeEach(() => {
  // Advance the real logger's channel window without replacing its behavior.
  clock += 61_000;
  vi.spyOn(Date, "now").mockReturnValue(clock);
});
afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

function renderName(user_name: string, env: Partial<Env>, fields: Record<string, unknown> = {}): string {
  return formatLine({
    bot: "general", level: "WARNING", event: "access.denied",
    fields: { user_id: 99, user_name, no_username: true, ...fields },
  }, Date.UTC(2026, 10, 28), 0, env);
}

async function webhook(bot: BotName, env: Env, name: { first_name: string; last_name?: string; username?: string }) {
  const sent: Array<Record<string, unknown>> = [];
  vi.stubGlobal("fetch", async (_url: string, init: RequestInit) => {
    sent.push(JSON.parse(String(init.body)));
    return Response.json({ ok: true, result: { message_id: sent.length, chat: { id: 99 } } });
  });
  const consoleLog = vi.spyOn(console, "log").mockImplementation(() => {});
  const pending: Promise<unknown>[] = [];
  const ctx = { waitUntil: (task: Promise<unknown>) => pending.push(task) } as unknown as ExecutionContext;
  const response = await worker.fetch(new Request(`https://worker.invalid/tg/${bot}`, {
    method: "POST",
    headers: { "X-Telegram-Bot-Api-Secret-Token": env[bot === "general" ? "GENERAL_WEBHOOK_SECRET" : "COIN_WEBHOOK_SECRET"] },
    body: JSON.stringify({ update_id: 1, message: {
      message_id: 5, from: { id: 99, ...name }, chat: { id: 99, type: "private" },
      text: "Message content must remain absent from operational logs",
    } }),
  }), env, ctx);
  await Promise.all(pending);
  expect(response.status).toBe(200);
  expect(await response.text()).toBe("ok");
  expect(sent.filter(payload => payload.chat_id === 99)).toHaveLength(1);
  const channel = sent.filter(payload => payload.chat_id === "-100900");
  expect(channel).toHaveLength(1);
  expect(channel[0].parse_mode).toBe("HTML");
  const outputs = [consoleLog.mock.calls.map(args => args.join(" ")).join("\n"), String(channel[0].text)];
  for (const output of outputs) {
    expect(output.includes("Message content must remain absent")).toBe(false);
    expect(output.length).toBeLessThanOrEqual(1500);
    expect(output).not.toMatch(/\p{Surrogate}/u);
  }
  return outputs;
}

describe("redaction before display-name presentation", () => {
  it.each(boundaryCases)("masks $binding before the $position is shortened", ({ binding, secret, prefix }) => {
    const output = renderName(prefix + secret, { [binding]: secret });
    expect(output.includes(`(${prefix}[REDACTED] · no @username)`)).toBe(true);
    expect(output.includes(secret.slice(0, 16))).toBe(false);
  });

  it.each(normalizationCases)("masks a credential before normalizing $name", ({ secret, first_name, last_name }) => {
    const name = [first_name, last_name].filter(Boolean).join(" ");
    const output = renderName(name, { PUBLISH_SECRET: secret });
    expect(output.includes("([REDACTED] · no @username)")).toBe(true);
    expect(output.includes("Fixture")).toBe(false);
  });

  it("preserves ordinary whitespace normalization and HTML escaping", () => {
    const output = renderName("  Alice\r\n<测试>&\tSmith  ", ENV);
    expect(output).toContain("user_id: <code>99</code> (Alice &lt;测试&gt;&amp; Smith · no @username)");
    expect(output).not.toContain("user_name:");
    expect(output).not.toContain("no_username:");
  });

  it("keeps Unicode intact at the 48-character display boundary", () => {
    const output = renderName("x".repeat(47) + "😀", ENV);
    expect(output).toContain(`(${"x".repeat(47)}… · no @username)`);
    expect(output).not.toMatch(/\p{Surrogate}/u);
  });

  it("preserves valid username precedence and redacts matching companion handles", () => {
    const output = renderName("Ignored Name", { PUBLISH_SECRET: "SensitiveHandle" }, {
      username: "SensitiveHandle", chat_id: 99,
    });
    expect(output).toContain("user_id: <code>99</code> (@[REDACTED])");
    expect(output).toContain("chat_id: <code>99</code> (@[REDACTED])");
    expect(output).not.toContain("Ignored Name");
    expect(output).not.toContain("no @username");
  });

  it("preserves the no-username annotation when the display name is blank", () => {
    expect(renderName(" \t\n ", ENV)).toContain("user_id: <code>99</code> (no @username)");
  });
});

describe.each(["general", "coin"] as const)("%s webhook log sinks", bot => {
  it.each(boundaryCases)("masks $binding across the $position in console and channel", async ({ binding, secret, prefix }) => {
    const outputs = await webhook(bot, { ...ENV, [binding]: secret }, { first_name: prefix + secret });
    for (const output of outputs) {
      expect(output.includes(`(${prefix}[REDACTED] · no @username)`)).toBe(true);
      expect(output.includes(secret.slice(0, 16))).toBe(false);
    }
  });

  it.each(normalizationCases)("redacts raw name parts before $name is normalized", async ({ secret, first_name, last_name }) => {
    const outputs = await webhook(bot, { ...ENV, PUBLISH_SECRET: secret }, { first_name, last_name });
    for (const output of outputs) {
      expect(output.includes("([REDACTED] · no @username)")).toBe(true);
      expect(output.includes("Fixture")).toBe(false);
    }
  });
});
