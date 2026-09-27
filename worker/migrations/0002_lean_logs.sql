-- Rebuild the log table for the cheapest possible write and eviction.
--
-- D1 bills rows written and rows read against an account-wide daily allowance.
-- Measured on 0001's table, one INSERT cost 4 rows written and 2 read:
--   * the row, plus one more per secondary index (logs_ts_idx, logs_bot_idx)
--     -- and no code ever read through either index;
--   * AUTOINCREMENT, which reads and updates sqlite_sequence on every insert.
-- Here an insert costs 1 row written and none read.
--
-- A plain INTEGER PRIMARY KEY (rowid alias) still gives rising ids: a new row
-- gets max(rowid)+1, and eviction (src/logging.ts) deletes only the OLDEST
-- rows, never the newest, so max(rowid) never goes back down.
--
-- `bytes` is gone: the cap is a row count now, so nothing needs a stored size.
-- Every existing row is kept, with its id.
CREATE TABLE logs_new (
  id     INTEGER PRIMARY KEY,
  ts     INTEGER NOT NULL,          -- epoch milliseconds, UTC
  bot    TEXT    NOT NULL,          -- 'general' | 'coin'
  level  TEXT    NOT NULL,          -- INFO | WARNING | ERROR
  event  TEXT    NOT NULL,          -- short stable key, e.g. 'webhook'
  detail TEXT                       -- free text, already redacted
);

INSERT INTO logs_new (id, ts, bot, level, event, detail)
  SELECT id, ts, bot, level, event, detail FROM logs;

-- Drops both secondary indexes and the table's sqlite_sequence row with it.
DROP TABLE logs;

ALTER TABLE logs_new RENAME TO logs;
