//! Original executable discovery over admitted source bytes, not import or runtime certification.
use crate::source_git_inventory::Source;
use std::collections::BTreeSet;

const REFERENCE: &[(&str, &str)] = &[
    ("add_media", "add-media"),
    ("build_collection", "build-collection"),
    ("build_pack", "build-pack"),
    ("emoji_bot", "emoji-bot"),
    ("fetch_emoji_ids", "fetch-emoji-ids"),
    ("fetch_pack", "fetch-pack"),
    ("identity_repair", "identity-repair"),
    ("make_emoji_pngs", "make-emoji-pngs"),
    ("pack_archive", "pack-archive"),
    ("pack_manifest", "pack-manifest"),
    ("panel", "panel"),
    ("plan_apply", "plan-apply"),
    ("plan_status", "plan-status"),
    ("status", "status"),
    ("sync_order", "sync-order"),
];
const COINS: &[(&str, &str)] = &[
    ("alias_map", "coin-alias-map"),
    ("build_keywords", "coin-build-keywords"),
    ("check_all_packs", "coin-check-packs"),
    ("enhance_map", "coin-enhance-map"),
    ("fetch_cmc", "coin-fetch-cmc"),
    ("fetch_logos", "coin-fetch-logos"),
    ("fetch_paprika", "coin-fetch-paprika"),
    ("rebuild_dedup", "coin-rebuild"),
    ("remap_ids", "coin-remap-ids"),
    ("verify_logos", "coin-verify-logos"),
    ("write_manifests", "coin-write-manifests"),
];
const SUPPORT: &[&str] = &[
    "_dedup_map",
    "_dedup_plan",
    "_env",
    "_http",
    "_inventory",
    "_keywords",
    "_paprika_api",
    "_provider_publish",
];

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub modules: BTreeSet<String>,
    pub commands: BTreeSet<String>,
    pub support: BTreeSet<String>,
    pub root_python: BTreeSet<String>,
}

pub fn inspect(sources: &[Source]) -> Result<Report, String> {
    let mut admitted = BTreeSet::new();
    for source in sources {
        crate::source_test_inventory::source_path(&source.path)?;
        if source.text.len() > 2 * 1024 * 1024
            || source.sha256 != crate::codec_raster::digest_hex(source.text.as_bytes())
            || !admitted.insert(source.path.as_str())
        {
            return Err("entry-point inventory requires unique admitted source bytes".into());
        }
    }
    let mut report = Report::default();
    let mut bridge = false;
    for source in sources {
        if source.path == "emojikit/media_bridge.py" {
            bridge = true;
            report.modules.insert("emojikit.media_bridge".into());
            continue;
        }
        let Some((directory, file)) = source.path.rsplit_once('/') else {
            report.root_python.insert(source.path.clone());
            continue;
        };
        let name = file.strip_suffix(".py").expect("admitted Python path");
        let (module, commands) = match directory {
            "tests/reference" if source.text.contains("\nif __name__ == \"__main__\":") => {
                (format!("tests.reference.{name}"), REFERENCE)
            }
            "coins" if name != "__init__" => (format!("coins.{name}"), COINS),
            _ => continue,
        };
        report.modules.insert(module.clone());
        if directory == "coins" && SUPPORT.contains(&name) {
            report.support.insert(module);
        } else {
            let (_, command) = commands
                .iter()
                .find(|(original, _)| *original == name)
                .ok_or_else(|| format!("original entry point has no native mapping: {module}"))?;
            report.commands.insert((*command).into());
        }
    }
    if !bridge {
        return Err("entry-point inventory is missing emojikit.media_bridge".into());
    }
    Ok(report)
}

pub fn check(report: &Report) -> Result<(), String> {
    if report.modules.len() < 15 {
        return Err("entry-point discovery must find at least fifteen original tools".into());
    }
    if !report.root_python.is_empty() {
        return Err("Python modules belong in their component, not the project root".into());
    }
    for expected in [
        "tests.reference.build_pack",
        "tests.reference.emoji_bot",
        "tests.reference.panel",
        "coins.verify_logos",
        "coins.rebuild_dedup",
    ] {
        if !report.modules.contains(expected) {
            return Err(format!(
                "entry-point discovery missed required tool: {expected}"
            ));
        }
    }
    Ok(())
}
