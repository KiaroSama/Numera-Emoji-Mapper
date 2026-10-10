//! Asset cache keys retain the existing SHA-1 contract; not a security digest.
use sha1::{Digest, Sha1};
use std::path::Path;

pub fn versions(root: &Path) -> std::io::Result<(String, String)> {
    let assets = root.join("assets");
    let scripts = [
        "panel-grid.js",
        "panel-motion.js",
        "panel-save.js",
        "panel-drag.js",
        "panel-actions.js",
        "panel-holding.js",
        "panel-draft.js",
    ];
    let text = scripts
        .iter()
        .map(|name| {
            std::fs::read_to_string(assets.join(name))
                .map(|text| text.replace("\r\n", "\n").replace('\r', "\n"))
        })
        .collect::<std::io::Result<Vec<_>>>()?
        .join("\n");
    let script = Sha1::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let icon = Sha1::digest(std::fs::read(assets.join("logo-128.png"))?)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    Ok((script[..12].into(), icon[..12].into()))
}
