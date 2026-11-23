/**
 * The log store against the REAL schema: both migrations applied in order to
 * an in-memory SQLite, then written through `log()` itself.
 *
 * Every other test hands `log()` a fake that records SQL strings, so nothing
 * ever proved the INSERT matches the table the migrations build -- and 0002
 * removed a column (`bytes`) that the INSERT must leave out. Node's built-in
 * `node:sqlite` stands in for D1 behind a three-method adapter.
 */

import { afterEach, describe, expect, it, vi } from "vitest";
import { LOG_ROWS_CAP, log } from "../src/logging";
import type { Env } from "../src/types";

// Node built-ins, loaded untyped: this package declares only the Workers types.
const node = (spec: string): Promise<any> => import(spec);
const databases: any[] = [];

afterEach(() => {
  for (const db of databases.splice(0)) db.close();
  vi.restoreAllMocks();
});

async function migratedDb(beforeUpgrade?: (db: any) => void): Promise<any> {
  const fs = await node("node:fs");
  const { DatabaseSync } = await node("node:sqlite");
  const db = new DatabaseSync(":memory:");
  databases.push(db);
  for (const file of ["0001_logs.sql", "0002_lean_logs.sql"]) {
    if (file === "0002_lean_logs.sql") beforeUpgrade?.(db);
    const url = new URL(`../migrations/${file}`, (import.meta as any).url);
    db.exec(fs.readFileSync(url, "utf8"));
  }
  return db;
}

/** D1's prepare().bind().run() surface, the part `log()` uses. */
function d1(db: any, beforeRun?: (sql: string, args: unknown[]) => Promise<void>): D1Database {
  return {
    prepare: (sql: string) => ({
      bind: (...args: unknown[]) => ({
        run: async () => {
          await beforeRun?.(sql, args);
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

  it("preserves populated legacy rows and continues above their newest identifier", async () => {
    const db = await migratedDb(legacy => {
      legacy.prepare("INSERT INTO logs (id, ts, bot, level, event, detail, bytes) VALUES (?,?,?,?,?,?,?)")
        .run(7, 100, "general", "WARNING", "legacy", "سلام 😀", 30);
      legacy.prepare("INSERT INTO logs (id, ts, bot, level, event, detail, bytes) VALUES (?,?,?,?,?,?,?)")
        .run(9000, 200, "coin", "ERROR", "legacy", null, 15);
    });

    await log({ ...ENV, DB: d1(db) }, { bot: "coin", level: "WARNING", event: "after-upgrade" });

    expect(db.prepare("SELECT id, ts, bot, level, event, detail FROM logs WHERE id <= 9000 ORDER BY id").all())
      .toEqual([
        { id: 7, ts: 100, bot: "general", level: "WARNING", event: "legacy", detail: "سلام 😀" },
        { id: 9000, ts: 200, bot: "coin", level: "ERROR", event: "legacy", detail: null },
      ]);
    expect(db.prepare("SELECT id, event FROM logs WHERE id > 9000").all())
      .toEqual([{ id: 9001, event: "after-upgrade" }]);
  });

  it("keeps the newest rows when an older eviction finishes after a newer one", async () => {
    vi.spyOn(console, "log").mockImplementation(() => {});
    const db = await migratedDb();
    db.exec(`WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 5249)
             INSERT INTO logs (id, ts, bot, level, event) SELECT i, 0, 'coin', 'INFO', 'seed' FROM n`);
    let release!: () => void;
    let reached!: () => void;
    const gate = new Promise<void>(resolve => { release = resolve; });
    const started = new Promise<void>(resolve => { reached = resolve; });
    const DB = d1(db, async (sql, args) => {
      if (sql.startsWith("DELETE") && args[0] === 250) {
        reached();
        await gate;
      }
    });
    const env = { ...ENV, DB };
    const delayed = log(env, { bot: "coin", level: "WARNING", event: "delayed" });
    await started;
    try {
      for (let i = 0; i < 250; i++) {
        await log(env, { bot: "coin", level: "WARNING", event: "concurrent" });
      }
    } finally {
      release();
      await delayed;
    }

    expect(db.prepare("SELECT min(id) lo, max(id) hi, count(*) n FROM logs").get())
      .toEqual({ lo: 501, hi: 5500, n: 5000 });
    expect(db.prepare("SELECT event FROM logs WHERE id = 5250").get()).toEqual({ event: "delayed" });
  });

  it("retains the inserted log on eviction failure and recovers at the next boundary", async () => {
    vi.spyOn(console, "log").mockImplementation(() => {});
    const error = vi.spyOn(console, "error").mockImplementation(() => {});
    const db = await migratedDb();
    db.exec(`WITH RECURSIVE n(i) AS (SELECT 1 UNION ALL SELECT i + 1 FROM n WHERE i < 5249)
             INSERT INTO logs (id, ts, bot, level, event) SELECT i, 0, 'coin', 'INFO', 'seed' FROM n`);
    let failEviction = true;
    const DB = d1(db, async sql => {
      if (sql.startsWith("DELETE") && failEviction) {
        failEviction = false;
        throw new Error("synthetic eviction failure");
      }
    });
    const env = { ...ENV, DB };
    await expect(log(env, { bot: "coin", level: "WARNING", event: "retained" }))
      .resolves.toBeUndefined();
    expect(db.prepare("SELECT event FROM logs WHERE id = 5250").get()).toEqual({ event: "retained" });
    expect(error.mock.calls.flat().join("\n")).toContain("synthetic eviction failure");

    for (let i = 0; i < 250; i++) {
      await log(env, { bot: "coin", level: "WARNING", event: "recovered" });
    }
    expect(db.prepare("SELECT min(id) lo, max(id) hi, count(*) n FROM logs").get())
      .toEqual({ lo: 501, hi: 5500, n: 5000 });
    expect(db.prepare("SELECT event FROM logs WHERE id = 5250").get()).toEqual({ event: "retained" });
  });
});
