//! Owned SQLite operations. Identity/media decisions are made before inserting a row.
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde_json::{Value, json};
use std::path::Path;
use std::time::Duration;

type Result<T> = std::result::Result<T, String>;
fn sql<T>(result: rusqlite::Result<T>) -> Result<T> {
    result.map_err(|e| e.to_string())
}
fn text<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("missing string {field}"))
}
fn optional<'a>(value: &'a Value, field: &str) -> Result<Option<&'a str>> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_str()
            .map(Some)
            .ok_or_else(|| format!("invalid {field}")),
    }
}
fn labels(value: &Value, field: &str) -> Result<Vec<String>> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(v) => v
            .as_array()
            .ok_or_else(|| format!("{field} must be a list"))?
            .iter()
            .map(|s| {
                s.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| format!("{field} must contain strings"))
            })
            .collect(),
    }
}
fn encode(values: &[String]) -> Result<String> {
    serde_json::to_string(values).map_err(|e| e.to_string())
}
fn item(row: &Row<'_>) -> rusqlite::Result<Value> {
    let parse = |name: &str| -> rusqlite::Result<Value> {
        let s: String = row.get(name)?;
        serde_json::from_str(&s).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
        })
    };
    let p: Option<i64> = row.get("phash")?;
    Ok(
        json!({"content_key":row.get::<_,String>("content_key")?,"fmt":row.get::<_,String>("format")?,
        "file_path":row.get::<_,String>("file_path")?,"emojis":parse("emojis")?,"keywords":parse("keywords")?,
        "sources":parse("sources")?,"phash":p.map(|n|n as u64),
        "custom_emoji_id":row.get::<_,Option<String>>("custom_emoji_id")?,
        "uploaded":row.get::<_,i64>("uploaded")?!=0,"included":row.get::<_,i64>("included")?!=0}),
    )
}

pub struct Catalog {
    db: Connection,
    _ownership: crate::ownership::Ownership,
    media_base: std::path::PathBuf,
    project_root: std::path::PathBuf,
}
impl Catalog {
    pub fn open(path: &Path, utc: &str, project_root: &Path) -> Result<Self> {
        let parent = path.parent().ok_or("catalog path has no parent")?;
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        let canonical = parent.canonicalize().map_err(|e| e.to_string())?;
        let ownership =
            crate::ownership::Ownership::acquire(&canonical, crate::ownership::Mode::Writer, utc)?;
        let db = sql(Connection::open(path))?;
        sql(db.busy_timeout(Duration::from_secs(5)))?;
        sql(db.pragma_update(None, "journal_mode", "WAL"))?;
        sql(db.pragma_update(None, "synchronous", "NORMAL"))?;
        sql(db.execute_batch("CREATE TABLE IF NOT EXISTS meta(key TEXT PRIMARY KEY,value TEXT);
            CREATE TABLE IF NOT EXISTS items(content_key TEXT PRIMARY KEY,format TEXT NOT NULL,file_path TEXT NOT NULL,
            emojis TEXT NOT NULL DEFAULT '[]',keywords TEXT NOT NULL DEFAULT '[]',sources TEXT NOT NULL DEFAULT '[]',
            phash INTEGER,custom_emoji_id TEXT,uploaded INTEGER NOT NULL DEFAULT 0,included INTEGER NOT NULL DEFAULT 1,created_utc TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS idx_items_format ON items(format);
            CREATE INDEX IF NOT EXISTS idx_items_uploaded ON items(uploaded);
            CREATE TABLE IF NOT EXISTS seen_files(file_unique_id TEXT PRIMARY KEY,content_key TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS publications(base TEXT NOT NULL,content_key TEXT NOT NULL,set_name TEXT,
            custom_emoji_id TEXT,uploaded_utc TEXT NOT NULL,PRIMARY KEY(base,content_key));
            CREATE INDEX IF NOT EXISTS idx_pub_base ON publications(base);"))?;
        let mut this = Self {
            db,
            _ownership: ownership,
            media_base: parent.to_path_buf(),
            project_root: project_root.to_path_buf(),
        };
        if this.meta("publications_migrated")?.as_deref() != Some("1") {
            let tx = sql(this.db.transaction())?;
            sql(tx.execute("INSERT OR IGNORE INTO publications(base,content_key,set_name,custom_emoji_id,uploaded_utc)
                SELECT '__legacy__',content_key,NULL,custom_emoji_id,? FROM items WHERE uploaded=1",[utc]))?;
            sql(tx.execute(
                "INSERT OR REPLACE INTO meta(key,value) VALUES('publications_migrated','1')",
                [],
            ))?;
            sql(tx.commit())?;
        }
        let columns = {
            let mut stmt = sql(this.db.prepare("PRAGMA table_info(items)"))?;
            sql(stmt.query_map([], |r| r.get::<_, String>(1)))?
                .collect::<rusqlite::Result<Vec<_>>>()
                .map_err(|e| e.to_string())?
        };
        if !columns.iter().any(|c| c == "included") {
            sql(this.db.execute(
                "ALTER TABLE items ADD COLUMN included INTEGER NOT NULL DEFAULT 1",
                [],
            ))?;
        }
        if !columns.iter().any(|c| c == "position") {
            sql(this.db.execute_batch("ALTER TABLE items ADD COLUMN position INTEGER; UPDATE items SET position=rowid WHERE position IS NULL;"))?;
        }
        sql(this.db.execute(
            "INSERT OR IGNORE INTO meta(key,value) VALUES('schema_version','1')",
            [],
        ))?;
        if this.meta("media_paths")?.as_deref() != Some("data-relative-2") {
            let rows = {
                let mut stmt = sql(this.db.prepare("SELECT content_key,file_path FROM items"))?;
                sql(stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))))?
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(|e| e.to_string())?
            };
            let tx = sql(this.db.transaction())?;
            for (key, stored) in rows {
                let resolved = crate::paths::resolve(&this.media_base, &stored, &this.project_root);
                let value =
                    crate::paths::store(&this.media_base, &resolved).map_err(|e| e.to_string())?;
                if value != stored {
                    sql(tx.execute(
                        "UPDATE items SET file_path=? WHERE content_key=?",
                        params![value, key],
                    ))?;
                }
            }
            sql(tx.execute(
                "INSERT OR REPLACE INTO meta(key,value) VALUES('media_paths','data-relative-2')",
                [],
            ))?;
            sql(tx.commit())?;
        }
        Ok(this)
    }
    fn resolve_item(&self, mut value: Value) -> Result<Value> {
        let stored = text(&value, "file_path")?;
        let path = crate::paths::resolve(&self.media_base, stored, &self.project_root);
        value["file_path"] = json!(path.to_str().ok_or("media path is not UTF-8")?);
        Ok(value)
    }
    pub fn meta(&self, key: &str) -> Result<Option<String>> {
        sql(self
            .db
            .query_row("SELECT value FROM meta WHERE key=?", [key], |r| r.get(0))
            .optional())
    }
    pub fn get(&self, key: &str) -> Result<Option<Value>> {
        sql(self
            .db
            .query_row("SELECT * FROM items WHERE content_key=?", [key], item)
            .optional())?
        .map(|v| self.resolve_item(v))
        .transpose()
    }
    pub fn all(&self, fmt: Option<&str>) -> Result<Vec<Value>> {
        let mut stmt = sql(self.db.prepare(
            "SELECT * FROM items WHERE (? IS NULL OR format=?) ORDER BY position,content_key",
        ))?;
        sql(stmt.query_map(params![fmt, fmt], item))?
            .map(|r| sql(r).and_then(|v| self.resolve_item(v)))
            .collect()
    }
    pub fn insert(&mut self, value: &Value, utc: &str) -> Result<bool> {
        let key = text(value, "content_key")?;
        let fmt = text(value, "fmt")?;
        if !["static", "video", "animated"].contains(&fmt) {
            return Err("invalid media format".into());
        }
        let path = crate::paths::store(&self.media_base, Path::new(text(value, "file_path")?))
            .map_err(|e| e.to_string())?;
        let emojis = encode(&labels(value, "emojis")?)?;
        let keywords = encode(&labels(value, "keywords")?)?;
        let sources = encode(&labels(value, "sources")?)?;
        let phash = match value.get("phash") {
            None | Some(Value::Null) => None,
            Some(v) => Some(v.as_u64().ok_or("invalid phash")? as i64),
        };
        let tx = sql(self.db.transaction())?;
        let n=sql(tx.execute("INSERT OR IGNORE INTO items(content_key,format,file_path,emojis,keywords,sources,phash,uploaded,created_utc,position)
            VALUES(?,?,?,?,?,?,?,0,?,(SELECT COALESCE(MAX(position),0)+1 FROM items))",params![key,fmt,path,emojis,keywords,sources,phash,utc]))?;
        sql(tx.commit())?;
        Ok(n != 0)
    }
    pub fn order(&mut self, keys: &[String]) -> Result<usize> {
        if keys.is_empty() {
            return Ok(0);
        }
        let tx = sql(self.db.transaction())?;
        sql(tx.execute("UPDATE items SET position=position+?", [keys.len() as i64]))?;
        let mut n = 0;
        for (i, k) in keys.iter().enumerate() {
            n += sql(tx.execute(
                "UPDATE items SET position=? WHERE content_key=?",
                params![i as i64, k],
            ))?;
        }
        sql(tx.commit())?;
        Ok(n)
    }
    pub fn inclusion(&mut self, keys: &[String]) -> Result<Value> {
        let tx = sql(self.db.transaction())?;
        sql(tx.execute("UPDATE items SET included=1", []))?;
        for key in keys {
            sql(tx.execute("UPDATE items SET included=0 WHERE content_key=?", [key]))?;
        }
        let inc: i64 = sql(
            tx.query_row("SELECT COUNT(*) FROM items WHERE included=1", [], |r| {
                r.get(0)
            }),
        )?;
        let exc: i64 = sql(
            tx.query_row("SELECT COUNT(*) FROM items WHERE included=0", [], |r| {
                r.get(0)
            }),
        )?;
        sql(tx.commit())?;
        Ok(json!([inc, exc]))
    }
    pub fn uploaded(&mut self, value: &Value, utc: &str) -> Result<()> {
        let key = text(value, "content_key")?;
        let id = optional(value, "custom_emoji_id")?;
        let base = optional(value, "base")?.filter(|s| !s.is_empty());
        let set = optional(value, "set_name")?;
        let tx = sql(self.db.transaction())?;
        sql(tx.execute(
            "UPDATE items SET uploaded=1,custom_emoji_id=? WHERE content_key=?",
            params![id, key],
        ))?;
        if let Some(base) = base {
            sql(tx.execute("INSERT INTO publications(base,content_key,set_name,custom_emoji_id,uploaded_utc) VALUES(?,?,?,?,?)
                ON CONFLICT(base,content_key) DO UPDATE SET set_name=COALESCE(excluded.set_name,set_name),custom_emoji_id=COALESCE(excluded.custom_emoji_id,custom_emoji_id)",params![base,key,set,id,utc]))?;
        }
        sql(tx.commit())?;
        Ok(())
    }
    pub fn pending(&self, fmt: Option<&str>, base: Option<&str>) -> Result<Vec<Value>> {
        let mut stmt=sql(self.db.prepare("SELECT * FROM items WHERE included=1 AND (? IS NULL OR format=?) AND
            ((? IS NULL AND uploaded=0) OR (? IS NOT NULL AND content_key NOT IN(SELECT content_key FROM publications WHERE base=?))) ORDER BY position,content_key"))?;
        sql(stmt.query_map(params![fmt, fmt, base, base, base], item))?
            .map(|r| sql(r).and_then(|v| self.resolve_item(v)))
            .collect()
    }
    pub fn record_seen(&mut self, fuid: &str, key: &str) -> Result<()> {
        if fuid.is_empty() || self.get(key)?.is_none() {
            return Ok(());
        }
        sql(self.db.execute(
            "INSERT OR IGNORE INTO seen_files(file_unique_id,content_key) VALUES(?,?)",
            params![fuid, key],
        ))?;
        Ok(())
    }
    pub fn merge_labels(&mut self, value: &Value) -> Result<()> {
        let key = text(value, "content_key")?;
        let Some(existing) = self.get(key)? else {
            return Ok(());
        };
        let mut merged = Vec::new();
        for field in ["emojis", "keywords", "sources"] {
            let mut current = labels(&existing, field)?;
            for label in labels(value, field)? {
                if !label.is_empty() && !current.contains(&label) {
                    current.push(label);
                }
            }
            merged.push(encode(&current)?);
        }
        let tx = sql(self.db.transaction())?;
        sql(tx.execute(
            "UPDATE items SET emojis=?,keywords=?,sources=? WHERE content_key=?",
            params![merged[0], merged[1], merged[2], key],
        ))?;
        if let Some(fuid) = optional(value, "file_unique_id")?.filter(|s| !s.is_empty()) {
            sql(tx.execute(
                "INSERT OR IGNORE INTO seen_files(file_unique_id,content_key) VALUES(?,?)",
                params![fuid, key],
            ))?;
        }
        sql(tx.commit())?;
        Ok(())
    }
    pub fn stats(&self) -> Result<Value> {
        let mut stmt = sql(self.db.prepare("SELECT format,COUNT(*),SUM(uploaded),SUM(CASE WHEN included=0 THEN 1 ELSE 0 END),SUM(CASE WHEN uploaded=0 AND included=1 THEN 1 ELSE 0 END) FROM items GROUP BY format"))?;
        let rows = sql(stmt.query_map([], |r| Ok((r.get::<_,String>(0)?,json!({"total":r.get::<_,i64>(1)?,"uploaded":r.get::<_,i64>(2)?,"excluded":r.get::<_,i64>(3)?,"pending":r.get::<_,i64>(4)?})))))?;
        let mut out = serde_json::Map::new();
        for row in rows {
            let (key, value) = sql(row)?;
            out.insert(key, value);
        }
        Ok(Value::Object(out))
    }
    pub fn dispatch(&mut self, request: Value) -> Result<Value> {
        let utc = optional(&request, "utc")?.unwrap_or("");
        match text(&request, "operation")? {
            "all" => Ok(json!(self.all(optional(&request, "fmt")?)?)),
            "get" => Ok(self
                .get(text(&request, "content_key")?)?
                .unwrap_or(Value::Null)),
            "insert" => Ok(json!(
                self.insert(request.get("item").ok_or("missing item")?, utc)?
            )),
            "order" => Ok(json!(self.order(&labels(&request, "keys")?)?)),
            "inclusion" => self.inclusion(&labels(&request, "keys")?),
            "uploaded" => {
                self.uploaded(&request, utc)?;
                Ok(Value::Null)
            }
            "pending" => Ok(json!(
                self.pending(optional(&request, "fmt")?, optional(&request, "base")?)?
            )),
            "meta" => Ok(json!(self.meta(text(&request, "key")?)?)),
            "set_meta" => {
                sql(self.db.execute(
                    "INSERT OR REPLACE INTO meta(key,value) VALUES(?,?)",
                    params![text(&request, "key")?, text(&request, "value")?],
                ))?;
                Ok(Value::Null)
            }
            "stats" => self.stats(),
            "merge_labels" => {
                self.merge_labels(&request)?;
                Ok(Value::Null)
            }
            "record_seen" => {
                self.record_seen(
                    text(&request, "file_unique_id")?,
                    text(&request, "content_key")?,
                )?;
                Ok(Value::Null)
            }
            "seen" => Ok(json!(sql(self
                .db
                .query_row(
                    "SELECT content_key FROM seen_files WHERE file_unique_id=?",
                    [text(&request, "file_unique_id")?],
                    |r| r.get::<_, String>(0)
                )
                .optional())?)),
            "published" => Ok(json!(
                sql(self
                    .db
                    .query_row(
                        "SELECT 1 FROM publications WHERE base=? AND content_key=?",
                        params![text(&request, "base")?, text(&request, "content_key")?],
                        |r| r.get::<_, i64>(0)
                    )
                    .optional())?
                .is_some()
            )),
            "custom_id" => Ok(json!(
                sql(self
                    .db
                    .query_row(
                        "SELECT custom_emoji_id FROM publications WHERE base=? AND content_key=?",
                        params![text(&request, "base")?, text(&request, "content_key")?],
                        |r| r.get::<_, Option<String>>(0)
                    )
                    .optional())?
                .flatten()
            )),
            "forget" => Ok(json!(sql(self.db.execute(
                "DELETE FROM publications WHERE base=?",
                [text(&request, "base")?]
            ))?)),
            "unpublish" => Ok(json!(
                sql(self.db.execute(
                    "DELETE FROM publications WHERE base=? AND content_key=?",
                    params![text(&request, "base")?, text(&request, "content_key")?]
                ))? != 0
            )),
            "adopt" => {
                let base = text(&request, "base")?;
                let has = sql(self
                    .db
                    .query_row(
                        "SELECT 1 FROM publications WHERE base=? LIMIT 1",
                        [base],
                        |r| r.get::<_, i64>(0),
                    )
                    .optional())?
                .is_some();
                Ok(json!(if has {
                    0
                } else {
                    sql(self.db.execute(
                        "UPDATE publications SET base=? WHERE base='__legacy__'",
                        [base],
                    ))?
                }))
            }
            "bases" | "published_keys" => {
                let query = if request["operation"] == "bases" {
                    "SELECT DISTINCT base FROM publications ORDER BY base"
                } else {
                    "SELECT DISTINCT content_key FROM publications ORDER BY content_key"
                };
                let mut stmt = sql(self.db.prepare(query))?;
                let rows = sql(stmt.query_map([], |r| r.get::<_, String>(0)))?
                    .collect::<rusqlite::Result<Vec<_>>>()
                    .map_err(|e| e.to_string())?;
                Ok(json!(rows))
            }
            "published_sets" => {
                let mut stmt = sql(self.db.prepare(
                    "SELECT content_key,set_name FROM publications WHERE set_name IS NOT NULL",
                ))?;
                let mut out = serde_json::Map::new();
                for row in
                    sql(stmt
                        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))))?
                {
                    let (k, n) = sql(row)?;
                    out.insert(k, json!(n));
                }
                Ok(json!(out))
            }
            _ => Err("unknown catalog operation".into()),
        }
    }
}
