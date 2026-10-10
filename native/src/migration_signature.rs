//! Original version2 logical SQL signature: Python repr sort and exact spaced UTF8 JSON.
use rusqlite::{Connection, types::Value as SqlValue};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;
fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
fn json_value(value: &SqlValue) -> Result<Value, String> {
    match value {
        SqlValue::Null => Ok(Value::Null),
        SqlValue::Integer(n) => Ok(json!(n)),
        SqlValue::Text(text) => Ok(json!(text)),
        SqlValue::Blob(bytes) => Ok(json!(
            bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()
        )),
        SqlValue::Real(_) => {
            Err("version2 signature REAL formatting is not verified; refusing before writes".into())
        }
    }
}
fn spaced(value: &Value) -> Result<String, String> {
    match value {
        Value::Array(values) => Ok(format!(
            "[{}]",
            values
                .iter()
                .map(spaced)
                .collect::<Result<Vec<_>, _>>()?
                .join(", ")
        )),
        Value::Object(_) => Err("unexpected object in SQL signature payload".into()),
        _ => serde_json::to_string(value).map_err(|e| e.to_string()),
    }
}
pub fn database(db: &Connection, files: &[Value]) -> Result<String, String> {
    let paths = files
        .iter()
        .map(|f| {
            Ok((
                f["destination"]
                    .as_str()
                    .ok_or("signature destination missing")?
                    .to_owned(),
                f["source"]
                    .as_str()
                    .ok_or("signature source missing")?
                    .to_owned(),
            ))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    let mut query=db.prepare("SELECT name,sql FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%' ORDER BY name").map_err(|e|e.to_string())?;
    let schema = query
        .query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?))
        })
        .map_err(|e| e.to_string())?
        .map(|r| r.map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let mut digest = Sha256::new();
    for (name, sql) in schema {
        let mut query = db
            .prepare(&format!("SELECT * FROM {}", quote(&name)))
            .map_err(|e| e.to_string())?;
        let columns = query
            .column_names()
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        let path_index = if name == "items" {
            columns.iter().position(|c| c == "file_path")
        } else {
            None
        };
        let mut cursor = query.query([]).map_err(|e| e.to_string())?;
        let mut rows = Vec::new();
        while let Some(row) = cursor.next().map_err(|e| e.to_string())? {
            let mut values = (0..columns.len())
                .map(|i| row.get::<_, SqlValue>(i).map_err(|e| e.to_string()))
                .collect::<Result<Vec<_>, _>>()?;
            if let Some(index) = path_index
                && let SqlValue::Text(path) = &values[index]
                && let Some(source) = paths.get(path)
            {
                values[index] = SqlValue::Text(source.clone());
            }
            rows.push((crate::python_repr::row(&values)?, values));
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0));
        let values = rows
            .iter()
            .map(|(_, row)| {
                row.iter()
                    .map(json_value)
                    .collect::<Result<Vec<_>, _>>()
                    .map(Value::Array)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let payload = json!([name, sql, columns, values]);
        digest.update(spaced(&payload)?.as_bytes());
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}
pub fn signature(path: &Path, files: &[Value]) -> Result<String, String> {
    let db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;
    database(&db, files)
}
