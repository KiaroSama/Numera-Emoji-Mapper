/**
 * The log store against the REAL schema: both migrations applied in order to
 * an in-memory SQLite, then written through `log()` itself.
 *
 * Every other test hands `log()` a fake that records SQL strings, so nothing
 * ever proved the INSERT matches the table the migrations build -- and 0002
 * removed a column (`bytes`) that the INSERT must leave out. Node's built-in
 * `node:sqlite` stands in for D1 behind a three-method adapter.
 */

import { describe, expect, it } from "vitest";
import { LOG_ROWS_CAP, log } from "../src/logging";
import type { Env } from "../src/types";

// Node built-ins, loaded untyped: this package declares only the Workers types.
const node = (spec: string): Promise<any> => import(spec);

async function migratedDb(): Promise<any> {
  const fs = await node("node:fs");
  const { DatabaseSync } = await node("node:sqlite");
  const db = new DatabaseSync(":memory:");
  for (const file of ["0001_logs.sql", "0002_lean_logs.sql"]) {
    const url = new URL(`../migrations/${file}`, (import.meta as any).url);
    db.exec(fs.readFileSync(url, "utf8"));
  }
  return db;
}

/** D1's prepare().bind().run() surface, the part `log()` uses. */
function d1(db: any): D1Database {
  return {
    prepare: (sql: string) => ({
      bind: (...args: unknown[]) => ({
        run: async () => {
          const r = db.prepare(sql).run(...args);
          return { meta: { last_row_id: Number(r.lastInsertRowid) } };
        },
      }),
    }),
  } as unknown as D1Database;
}

const ENV = { GENERAL_BOT_TOKEN: "111:general", COIN_BOT_TOKEN: "222:coin" } as Env;

describe("the log table the migrations build", () => {
  it("takes the row log() writes", async () => {
    const db = await migratedDb();
    await log({ ...ENV, DB: d1(db) },
              { bot: "general", level: "WARNING", event: "webhook", detail: "slow" });
    const rows = db.prepare("SELECT bot, level, event, detail FROM logs").all();
    expect(rows).toEqual([{ bot: "general", level: "WARNING", event: "webhook", detail: "slow" }]);
  });

  it("eviction keeps the newest rows", async () => {
    const db = await migratedDb();
    // Seed up to one below an eviction boundary, so the next insert crosses it.
    const seeded = LOG_ROWS_CAP + 249;
    db.exec(`WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < ${seeded})
             INSERT INTO logs (id, ts, bot, level, event) SELECT i, 0, 'coin', 'INFO', 'seed' FROM n`);
    await log({ ...ENV, DB: d1(db) }, { bot: "coin", level: "ERROR", event: "x", toChannel: false });
    const { lo, hi, n } = db.prepare("SELECT min(id) lo, max(id) hi, count(*) n FROM logs").get();
    expect(hi).toBe(seeded + 1);
    expect(n).toBe(LOG_ROWS_CAP);
    expect(lo).toBe(seeded + 1 - LOG_ROWS_CAP + 1);
  });
});
