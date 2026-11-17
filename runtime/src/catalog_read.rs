//! Reporting opens SQLite read-only: no schema migration, WAL pragma or ownership metadata write.
use rusqlite::{Connection, OpenFlags};
use serde_json::{Value, json};
use std::path::Path;
pub fn plan_rows(path: &Path, base: &str) -> Result<(Value, Value), String> {
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    db.busy_timeout(std::time::Duration::from_secs(5))
        .map_err(|e| e.to_string())?;
    let mut statement = db
        .prepare("SELECT content_key,included,position FROM items ORDER BY position,content_key")
        .map_err(|e| e.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<i64>>(2)?,
            ))
        })
        .map_err(|e| e.to_string())?;
    let mut items = serde_json::Map::new();
    for (pos, row) in rows.enumerate() {
        let (key, included, _) = row.map_err(|e| e.to_string())?;
        items.insert(key, json!({"included":included!=0,"pos":pos}));
    }
    let mut statement=db.prepare("SELECT content_key,custom_emoji_id FROM publications WHERE base=? AND custom_emoji_id IS NOT NULL").map_err(|e|e.to_string())?;
    let mut ids = serde_json::Map::new();
    for row in statement
        .query_map([base], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|e| e.to_string())?
    {
        let (key, id) = row.map_err(|e| e.to_string())?;
        if !id.is_empty() {
            ids.insert(key, json!(id));
        }
    }
    Ok((json!(items), json!(ids)))
}
pub fn pending(path: &Path, base: &str) -> Result<u64, String> {
    let db = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    db.query_row("SELECT COUNT(*) FROM items WHERE included=1 AND content_key NOT IN(SELECT content_key FROM publications WHERE base=?)",[base],|row|row.get::<_,i64>(0)).map_err(|e|e.to_string()).and_then(|n|u64::try_from(n).map_err(|_|"invalid pending count".into()))
}
