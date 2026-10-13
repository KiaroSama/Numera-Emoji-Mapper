//! One native keyword CSV writer: verified PNGs and retained names survive partial lookups.
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

fn text<'a>(row: &'a Value, field: &str) -> Result<&'a str, String> {
    row.get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| format!("keyword row missing text {field}"))
}

pub fn old_names(path: &Path) -> Result<BTreeMap<String, String>, String> {
    if !path.is_file() {
        return Ok(BTreeMap::new());
    }
    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .from_path(path)
        .map_err(|_| "cannot read previous keyword CSV")?;
    let headers = reader
        .headers()
        .map_err(|_| "invalid keyword CSV headers")?;
    if headers.is_empty() {
        return Ok(BTreeMap::new());
    }
    let ticker = headers
        .iter()
        .position(|field| field == "ticker")
        .ok_or("keyword CSV has no ticker header")?;
    let name = headers.iter().position(|field| field == "name");
    let mut names = BTreeMap::new();
    for row in reader.records() {
        let row = row.map_err(|_| "invalid keyword CSV row")?;
        let ticker = row.get(ticker).ok_or("keyword CSV row has no ticker")?;
        names.insert(
            ticker.to_lowercase(),
            name.and_then(|index| row.get(index)).unwrap_or("").into(),
        );
    }
    Ok(names)
}

pub fn write(rows: &Map<String, Value>, path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("keyword CSV path must be absolute".into());
    }
    let old = old_names(path)?;
    let mut sorted = rows
        .values()
        .map(|row| Ok((text(row, "ticker")?, row)))
        .collect::<Result<Vec<_>, String>>()?;
    sorted.sort_by_key(|(ticker, _)| *ticker);
    let mut writer = csv::WriterBuilder::new()
        .terminator(csv::Terminator::CRLF)
        .from_writer(Vec::new());
    writer
        .write_record(["ticker", "name", "format", "file", "keywords"])
        .map_err(|_| "cannot encode keyword CSV header")?;
    for (ticker, row) in sorted {
        let format = text(row, "format")?;
        let file = text(row, "file")?;
        if format == "png" {
            let image = row
                .get("path")
                .and_then(Value::as_str)
                .filter(|path| !path.is_empty())
                .map(PathBuf::from)
                .unwrap_or_else(|| path.parent().unwrap_or(Path::new("")).join(file));
            if !crate::codec_image::valid_logo(&image) {
                continue;
            }
        }
        let name = text(row, "name")?;
        let name = if name.is_empty() {
            old.get(ticker).map(String::as_str).unwrap_or("")
        } else {
            name
        };
        let keywords = if name.is_empty() {
            ticker.to_owned()
        } else {
            format!("{ticker}, {name}")
        };
        writer
            .write_record([ticker, name, format, file, &keywords])
            .map_err(|_| "cannot encode keyword CSV row")?;
    }
    let bytes = writer
        .into_inner()
        .map_err(|_| "cannot finish keyword CSV")?;
    crate::atomic::write_bytes(path, &bytes).map_err(|_| "cannot persist keyword CSV".into())
}
