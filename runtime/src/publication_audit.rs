use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

const USAGE: &str = "Usage: numera-emoji audit-publication --base BASE [--data-dir DIR] [--json]";

#[derive(Serialize)]
struct Finding {
    code: &'static str,
    message: String,
    set_index: Option<usize>,
    content_key: Option<String>,
}

fn finding(
    code: &'static str,
    message: String,
    set_index: Option<usize>,
    key: Option<&str>,
) -> Finding {
    Finding {
        code,
        message: crate::logging::redact(&message, &[]),
        set_index,
        content_key: key.map(|key| crate::logging::redact(key, &[])),
    }
}

#[derive(Serialize)]
struct AuditError {
    code: &'static str,
    message: String,
}

fn error(code: &'static str, message: impl Into<String>) -> AuditError {
    AuditError {
        code,
        message: crate::logging::redact(&message.into(), &[]),
    }
}

pub fn run(args: &[String], root: impl FnOnce() -> Result<PathBuf, String>) -> u8 {
    if args == ["--help"] || args == ["-h"] {
        println!("{USAGE}");
        return 0;
    }
    let count = args.iter().filter(|arg| *arg == "--json").count();
    let json_mode = count > 0;
    let mut base = None;
    let mut data_dir = None;
    let result = if count > 1 {
        Err(error("INVALID_ARGUMENTS", USAGE))
    } else {
        options(args, root, &mut base).and_then(|data| {
            data_dir = Some(crate::logging::redact(&data.to_string_lossy(), &[]));
            inspect(
                &data,
                base.as_deref()
                    .ok_or_else(|| error("INVALID_ARGUMENTS", USAGE))?,
            )
        })
    };
    if !json_mode {
        println!(
            "Local read-only publication report. No live Telegram verification or catalog/state mutation."
        );
    }
    let (status, code, findings, failure) = match result {
        Ok(findings) => {
            if !json_mode {
                for finding in &findings {
                    println!("{}: {}", finding.code, finding.message);
                }
                println!("Findings: {}", findings.len());
            }
            let code = u8::from(!findings.is_empty());
            (
                if code == 0 { "clean" } else { "findings" },
                code,
                findings,
                None,
            )
        }
        Err(failure) => {
            if !json_mode {
                eprintln!("UNKNOWN: {}", failure.message);
            }
            ("unknown", 2, Vec::new(), Some(failure))
        }
    };
    if json_mode {
        println!(
            "{}",
            json!({"schema_version":1,"status":status,"base":base,"data_dir":data_dir,"findings":findings,"error":failure})
        );
    }
    code
}

fn options(
    args: &[String],
    root: impl FnOnce() -> Result<PathBuf, String>,
    resolved_base: &mut Option<String>,
) -> Result<PathBuf, AuditError> {
    let mut base = None;
    let mut data = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        let slot = match arg.as_str() {
            "--json" => continue,
            "--base" if base.is_none() => &mut base,
            "--data-dir" if data.is_none() => &mut data,
            _ => return Err(error("INVALID_ARGUMENTS", USAGE)),
        };
        *slot = Some(
            args.next()
                .filter(|v| !v.is_empty() && !v.starts_with("--"))
                .ok_or_else(|| error("INVALID_ARGUMENTS", USAGE))?,
        );
    }
    let base = base.ok_or_else(|| error("INVALID_ARGUMENTS", USAGE))?;
    crate::collection_names::valid_base(base).map_err(|e| error("INVALID_ARGUMENTS", e))?;
    *resolved_base = Some(base.clone());
    let data = data.map_or_else(|| PathBuf::from("collection"), PathBuf::from);
    let data = if data.is_absolute() {
        data
    } else {
        root().map_err(|e| error("ROOT_UNRESOLVED", e))?.join(data)
    };
    Ok(data)
}

fn text(value: &Value) -> Option<&str> {
    value.as_str().filter(|s| !s.is_empty())
}

fn inspect(data: &Path, base: &str) -> Result<Vec<Finding>, AuditError> {
    let path = data.join(format!("publish_{base}.json"));
    let bytes = std::fs::read(&path)
        .map_err(|e| error("STATE_UNREADABLE", format!("state {}: {e}", path.display())))?;
    let state: Value = serde_json::from_slice(&bytes)
        .map_err(|e| error("STATE_MALFORMED", format!("malformed state: {e}")))?;
    let document = state
        .as_object()
        .ok_or_else(|| error("STATE_MALFORMED", "malformed state: expected an object"))?;
    if document.get("sets").is_some_and(|sets| !sets.is_array()) {
        return Err(error(
            "STATE_MALFORMED",
            "malformed state: sets must be an array",
        ));
    }
    let mut findings = Vec::new();
    let state = match crate::resume::collection(state.clone(), base) {
        Ok(normalized) => normalized,
        Err(error) => {
            findings.push(finding("MALFORMED_STATE", error, None, None));
            state
        }
    };
    let sets = state.get("sets").and_then(Value::as_array);
    let path = data.join("catalog.db");
    let catalog = |e: rusqlite::Error| {
        error(
            "CATALOG_UNREADABLE",
            format!("catalog {}: {e}", path.display()),
        )
    };
    let db =
        Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(catalog)?;
    let mut query = db
        .prepare(
            "SELECT i.uploaded, p.custom_emoji_id, p.set_name, p.content_key
        FROM items i LEFT JOIN publications p ON p.content_key=i.content_key AND p.base=?1
        WHERE i.content_key=?2",
        )
        .map_err(catalog)?;
    let mut seen = BTreeSet::new();
    for (index, set) in sets.into_iter().flatten().enumerate() {
        let name = text(&set["name"]);
        let keys = set.get("keys");
        if name.is_none()
            || keys.is_some_and(|v| !v.is_array())
            || set.get("live").is_some_and(|v| v.as_u64().is_none())
            || set.get("logo").is_some_and(|v| !v.is_boolean())
        {
            findings.push(finding("MALFORMED_SET", format!("sets[{index}] has a missing or invalid name or an invalid keys, live or logo field"), Some(index), None));
        }
        for value in keys.and_then(Value::as_array).into_iter().flatten() {
            let Some(key) = text(value) else {
                findings.push(finding(
                    "MALFORMED_SET",
                    format!("sets[{index}] contains an invalid key"),
                    Some(index),
                    None,
                ));
                continue;
            };
            if !seen.insert(key) {
                findings.push(finding(
                    "DUPLICATE_KEY",
                    format!("{key:?}"),
                    Some(index),
                    Some(key),
                ));
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
                findings.push(finding(
                    "MISSING_CATALOG_KEY",
                    format!("{key:?}"),
                    Some(index),
                    Some(key),
                ));
                continue;
            };
            if publication.is_none() {
                findings.push(finding(
                    "MISSING_PUBLICATION",
                    format!("{key:?}"),
                    Some(index),
                    Some(key),
                ));
            }
            if uploaded != 1 {
                findings.push(finding(
                    "NOT_UPLOADED",
                    format!("{key:?}"),
                    Some(index),
                    Some(key),
                ));
            }
            if cid.as_deref().is_none_or(|v| v.trim().is_empty()) {
                findings.push(finding(
                    "MISSING_CUSTOM_EMOJI_ID",
                    format!("{key:?}"),
                    Some(index),
                    Some(key),
                ));
            }
            if let Some(name) = name
                && actual_set.as_deref() != Some(name)
            {
                findings.push(finding(
                    "SET_NAME_CONFLICT",
                    format!("{key:?}, state={name:?}, catalog={actual_set:?}"),
                    Some(index),
                    Some(key),
                ));
            }
        }
    }
    Ok(findings)
}
