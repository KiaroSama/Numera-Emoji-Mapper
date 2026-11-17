//! One transaction moves every key-bearing SQL reference, including swaps, without row merges.
use rusqlite::{Connection, params};
use std::collections::BTreeMap;
pub const REFERENCES: [(&str, &str); 3] = [
    ("items", "content_key"),
    ("publications", "content_key"),
    ("seen_files", "content_key"),
];
pub fn rewrite(
    db: &mut Connection,
    map: &BTreeMap<String, String>,
    hashes: &BTreeMap<String, Option<u64>>,
) -> Result<u64, String> {
    let transaction = db.transaction().map_err(|e| e.to_string())?;
    let mut staged = Vec::new();
    let mut moved = 0;
    for (i, (old, new)) in map.iter().enumerate() {
        if old == new {
            continue;
        }
        let temporary = format!("__identity_migration_{i}__");
        for (table, column) in REFERENCES {
            let count: i64 = transaction
                .query_row(
                    &format!("SELECT COUNT(*) FROM {table} WHERE {column}=?"),
                    [&temporary],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            if count > 0 {
                return Err("catalog contains an unresolved temporary identity".into());
            }
            let changed = transaction
                .execute(
                    &format!("UPDATE {table} SET {column}=? WHERE {column}=?"),
                    params![temporary, old],
                )
                .map_err(|e| e.to_string())?;
            if table == "items" {
                moved += changed as u64;
            }
        }
        staged.push((temporary, new));
    }
    for (temporary, new) in staged {
        for (table, column) in REFERENCES {
            transaction
                .execute(
                    &format!("UPDATE {table} SET {column}=? WHERE {column}=?"),
                    params![new, temporary],
                )
                .map_err(|e| e.to_string())?;
        }
    }
    for (key, hash) in hashes {
        transaction
            .execute(
                "UPDATE items SET phash=? WHERE content_key=?",
                params![hash.map(|h| h as i64), key],
            )
            .map_err(|e| e.to_string())?;
    }
    transaction.commit().map_err(|e| e.to_string())?;
    Ok(moved)
}
