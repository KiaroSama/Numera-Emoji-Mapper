use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

const USAGE: &str = "Usage: numera-emoji audit-publication --base BASE [--data-dir DIR]";

pub fn run(args: &[String], root: impl FnOnce() -> Result<PathBuf, String>) -> u8 {
    if args == ["--help"] || args == ["-h"] {
        println!("{USAGE}");
        return 0;
    }
    println!(
        "Local read-only publication report. No live Telegram verification or catalog/state mutation."
    );
    match options(args, root).and_then(|(base, data)| inspect(&data, &base)) {
        Ok(findings) => {
            for finding in &findings {
                println!("{finding}");
            }
            println!("Findings: {}", findings.len());
            u8::from(!findings.is_empty())
        }
        Err(error) => {
            eprintln!("UNKNOWN: {error}");
            2
        }
    }
}

fn options(
    args: &[String],
    root: impl FnOnce() -> Result<PathBuf, String>,
) -> Result<(String, PathBuf), String> {
    let mut base = None;
    let mut data = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let slot = match arg.as_str() {
            "--base" if base.is_none() => &mut base,
            "--data-dir" if data.is_none() => &mut data,
            _ => return Err(USAGE.into()),
        };
        *slot = Some(
            args.next()
                .filter(|v| !v.is_empty() && !v.starts_with("--"))
                .ok_or(USAGE)?,
        );
    }
    let base = base.ok_or(USAGE)?;
    crate::collection_names::valid_base(base)?;
    let data = data.map_or_else(|| PathBuf::from("collection"), PathBuf::from);
    let data = if data.is_absolute() {
        data
    } else {
        root()?.join(data)
    };
    Ok((base.clone(), data))
}

fn text(value: &Value) -> Option<&str> {
    value.as_str().filter(|s| !s.trim().is_empty())
}

pub fn inspect(data: &Path, base: &str) -> Result<Vec<String>, String> {
    crate::collection_names::valid_base(base)?;
    let path = data.join(format!("publish_{base}.json"));
    let bytes = std::fs::read(&path).map_err(|e| format!("state {}: {e}", path.display()))?;
    let state: Value =
        serde_json::from_slice(&bytes).map_err(|e| format!("malformed state: {e}"))?;
    let sets = state["sets"]
        .as_array()
        .ok_or("malformed state: sets must be an array")?;
    let path = data.join("catalog.db");
    let catalog = |e: rusqlite::Error| format!("catalog {}: {e}", path.display());
    let db =
        Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(catalog)?;
    let mut query = db
        .prepare(
            "SELECT i.uploaded, p.custom_emoji_id, p.set_name, p.content_key
        FROM items i LEFT JOIN publications p ON p.content_key=i.content_key AND p.base=?1
        WHERE i.content_key=?2",
        )
        .map_err(catalog)?;
    let mut findings = Vec::new();
    let mut seen = BTreeSet::new();
    for (index, set) in sets.iter().enumerate() {
        let name = text(&set["name"]);
        let keys = set["keys"].as_array();
        if name.is_none()
            || keys.is_none()
            || set["live"].as_u64().is_none()
            || !set["logo"].is_boolean()
        {
            findings.push(format!(
                "MALFORMED_SET: sets[{index}] requires name, keys, live and logo"
            ));
        }
        for value in keys.into_iter().flatten() {
            let Some(key) = text(value) else {
                findings.push(format!(
                    "MALFORMED_SET: sets[{index}] contains an invalid key"
                ));
                continue;
            };
            if !seen.insert(key) {
                findings.push(format!("DUPLICATE_KEY: {key:?}"));
            }
            let row = query
                .query_row(params![base, key], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, Option<String>>(3)?,
                    ))
                })
                .optional()
                .map_err(catalog)?;
            let Some((uploaded, cid, actual_set, publication)) = row else {
                findings.push(format!("MISSING_CATALOG_KEY: {key:?}"));
                continue;
            };
            if publication.is_none() {
                findings.push(format!("MISSING_PUBLICATION: {key:?}"));
            }
            if uploaded != 1 {
                findings.push(format!("NOT_UPLOADED: {key:?}"));
            }
            if cid.as_deref().is_none_or(|v| v.trim().is_empty()) {
                findings.push(format!("MISSING_CUSTOM_EMOJI_ID: {key:?}"));
            }
            if let Some(name) = name
                && actual_set.as_deref() != Some(name)
            {
                findings.push(format!(
                    "SET_NAME_CONFLICT: {key:?}, state={name:?}, catalog={actual_set:?}"
                ));
            }
        }
    }
    Ok(findings)
}
