//! Quoted UTF-8 CSV keyword fields use the installed CSV parser, never comma splitting.
use std::collections::BTreeMap;
use std::path::Path;
pub fn load(path: &Path) -> Result<BTreeMap<String, String>, String> {
    if !path.is_file() {
        return Ok(BTreeMap::new());
    }
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut reader = csv::ReaderBuilder::new().flexible(true).from_reader(file);
    let headers = reader.headers().map_err(|e| e.to_string())?;
    let ticker = headers
        .iter()
        .position(|s| s == "ticker")
        .ok_or("keyword CSV has no ticker header")?;
    let keywords = headers.iter().position(|s| s == "keywords");
    let mut out = BTreeMap::new();
    for row in reader.records() {
        let row = row.map_err(|e| e.to_string())?;
        let key = row.get(ticker).ok_or("keyword CSV row has no ticker")?;
        let labels = keywords
            .and_then(|i| row.get(i))
            .filter(|s| !s.is_empty())
            .unwrap_or(key);
        out.insert(key.to_lowercase(), labels.into());
    }
    Ok(out)
}
