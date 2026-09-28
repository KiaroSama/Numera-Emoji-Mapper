/**
 * The Worker half of the Python <-> Worker contracts, read from the fixtures
 * `tests/test_contracts_shared.py` reads too.
 *
 * Each side used to test only against its own hand-written idea of the other:
 * the publisher's body, and the custom-emoji id syntax both bots copy. A change
 * on one side stayed green on both while announcements were lost. With one
 * fixture, changing either side breaks both suites together.
 */

import { describe, expect, it } from "vitest";
import { extractCustomEmojiIds, parseIdList } from "../src/emoji";
import type { TgMessage } from "../src/types";
import { validatePublishRequest } from "../src/validate";

// Node built-ins, loaded untyped: this package declares only the Workers types.
const node = (spec: string): Promise<any> => import(spec);

async function fixture(name: string): Promise<any> {
  const fs = await node("node:fs");
  const url = new URL(`../../tests/fixtures/contracts/${name}`, (import.meta as any).url);
  return JSON.parse(fs.readFileSync(url, "utf8"));
}

describe("shared contract fixtures", () => {
  it("accepts exactly the body the Python publisher sends", async () => {
    const body = await fixture("publish_request.json");
    const checked = validatePublishRequest(body);
    expect(checked.ok).toBe(true);
    if (checked.ok) expect(checked.value).toEqual(body);
  });

  it("reads custom-emoji ids the way the Python bot does", async () => {
    const cases = await fixture("emoji_messages.json");
    expect(cases.length).toBeGreaterThan(0);
    for (const c of cases) {
      expect(extractCustomEmojiIds(c.message as TgMessage), c.name).toEqual(c.ids);
      expect(parseIdList(c.message.text ?? ""), c.name).toEqual(c.typed);
    }
  });
});
