/** Log shortening must preserve diagnostics and valid Unicode at every sink. */
import { afterEach, describe, expect, it, vi } from "vitest";
import { formatLine, log, redact } from "../src/logging";
import type { Env } from "../src/types";

afterEach(() => vi.restoreAllMocks());

describe("bounded log diagnostics", () => {
  it("keeps the timestamp and suppressed count when a detail is shortened", () => {
    const line = formatLine({
      bot: "coin", level: "ERROR", event: "publish", detail: "x".repeat(2000),
    }, Date.UTC(2026, 10, 22, 12, 30, 15), 7);

    expect(line.length).toBeLessThanOrEqual(700);
    expect(line).toContain("[coin]\n❌ ERROR publish\n");
    expect(line).toContain("…");
    expect(line).toContain("2026-11-22 12:30:15 UTC");
    expect(line.endsWith("(+7 suppressed by rate limit)")).toBe(true);
  });

  it("never cuts an astral character at the stored-detail boundary", () => {
    const detail = redact("x".repeat(1999) + "😀");
    expect(detail).not.toMatch(/\p{Surrogate}/u);
    expect(detail).toContain("…");
  });

  it.each(["", "x"])("keeps a shortened channel detail well formed with prefix %j", prefix => {
    const line = formatLine({
      bot: "general", level: "ERROR", event: "webhook", detail: prefix + "😀".repeat(1000),
    }, Date.UTC(2026, 10, 22));

    expect(line.length).toBeLessThanOrEqual(700);
    expect(line).not.toMatch(/\p{Surrogate}/u);
    expect(line.endsWith("2026-11-22 00:00:00 UTC")).toBe(true);
  });

  it("timestamps a failed sink without losing the bot, severity, or error", async () => {
    vi.spyOn(console, "log").mockImplementation(() => {});
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    const DB = { prepare: () => { throw new Error("D1 is unavailable"); } } as unknown as D1Database;

    await expect(log({ DB } as Env, {
      bot: "coin", level: "WARNING", event: "webhook", detail: "request failed",
    })).resolves.toBeUndefined();

    const output = error.mock.calls.map(args => args.join(" ")).join("\n");
    expect(output).toContain("[coin]");
    expect(output).toContain("ERROR log-sink");
    expect(output).toContain("D1 is unavailable");
    expect(output).toMatch(/\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2} UTC/);
  });
});
