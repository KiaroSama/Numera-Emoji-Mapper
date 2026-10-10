//! Publisher manifest derives IDs from the already identity-verified catalog mapping.
use crate::{catalog::Catalog, pack_rows};
use serde_json::{Value, json};
use std::path::Path;
pub fn write(data: &Path, cat: &mut Catalog, set: &Value, base: &str) -> Result<(), String> {
    let keys = set["keys"].as_array().ok_or("set has no keys")?;
    if keys.is_empty() {
        return Ok(());
    }
    let name = set["name"].as_str().ok_or("set has no name")?;
    if !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err("unsafe manifest name".into());
    }
    let title = set["title"].as_str().unwrap_or(name);
    let logo = usize::from(set["logo"].as_bool().unwrap_or(false));
    let mut rows = if logo > 0 {
        vec![json!({"n":1,"name":"brand logo","cid":""})]
    } else {
        vec![]
    };
    for (i, key) in keys.iter().enumerate() {
        let key = key.as_str().ok_or("invalid manifest key")?;
        let item = cat.get(key)?;
        let label = item
            .as_ref()
            .map(pack_rows::keywords)
            .filter(|s| !s.is_empty())
            .or_else(|| {
                item.as_ref().and_then(|v| {
                    v["sources"]
                        .as_array()?
                        .first()?
                        .as_str()
                        .map(str::to_owned)
                })
            })
            .unwrap_or_else(|| key.into());
        let cid = cat.dispatch(json!({"operation":"custom_id","base":base,"content_key":key}))?;
        rows.push(json!({"n":i+1+logo,"name":label,"cid":cid}));
    }
    let format = set["fmt"].as_str().ok_or("invalid manifest format")?;
    let body = format!(
        "# {title}\n\nPack: https://t.me/addemoji/{name}  |  format: {format}  |  {} emoji\n\n{}\n",
        keys.len() + logo,
        pack_rows::markdown(&rows, &[("n", "#"), ("name", "Name"), ("cid", "Emoji ID")])
    );
    let directory = data.join("manifests");
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    std::fs::write(directory.join(format!("{name}.md")), body.as_bytes()).map_err(|e| e.to_string())
}
