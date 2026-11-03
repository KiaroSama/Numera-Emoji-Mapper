//! GUIDE contracts reuse captured command help; they never execute retained Python entry points.
use std::collections::{BTreeMap, BTreeSet};

const NATIVE: &[&str] = &[
    "make-emoji-pngs",
    "build-pack",
    "fetch-pack",
    "fetch-emoji-ids",
    "add-media",
    "build-collection",
    "sync-order",
    "pack-manifest",
    "pack-archive",
    "status",
    "panel",
];
const COINS: &[(&str, &str)] = &[
    ("fetch_logos", "coin-fetch-logos"),
    ("fetch_paprika", "coin-fetch-paprika"),
    ("fetch_cmc", "coin-fetch-cmc"),
    ("build_keywords", "coin-build-keywords"),
    ("rebuild_dedup", "coin-rebuild"),
    ("remap_ids", "coin-remap-ids"),
    ("verify_logos", "coin-verify-logos"),
    ("check_all_packs", "coin-check-packs"),
    ("write_manifests", "coin-write-manifests"),
    ("enhance_map", "coin-enhance-map"),
    ("alias_map", "coin-alias-map"),
];

pub struct Section {
    body: String,
    commands: BTreeSet<String>,
    coin_sources: BTreeSet<String>,
}

pub fn flags(text: &str) -> BTreeSet<String> {
    text.match_indices("--")
        .filter_map(|(index, _)| {
            if text[..index]
                .chars()
                .next_back()
                .is_some_and(|ch| ch == '-' || ch == '_' || ch.is_alphanumeric())
            {
                return None;
            }
            let tail = &text[index + 2..];
            if !tail.as_bytes().first().is_some_and(u8::is_ascii_lowercase) {
                return None;
            }
            let length = tail
                .bytes()
                .take_while(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || *ch == b'-')
                .count();
            Some(text[index..index + 2 + length].into())
        })
        .collect()
}

pub fn sections(text: &str) -> Result<BTreeMap<String, Section>, String> {
    let mut out = BTreeMap::<String, Section>::new();
    let mut current = None;
    for line in text.split_inclusive('\n') {
        let header = line.strip_prefix("### ").and_then(|rest| {
            let (number, title) = rest.split_once(' ')?;
            let suffix = number.strip_prefix("12.")?;
            (!suffix.is_empty()
                && suffix
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_'))
            .then_some((number, title))
        });
        if let Some((number, title)) = header {
            let commands = title
                .split('`')
                .skip(1)
                .step_by(2)
                .filter(|name| NATIVE.contains(name))
                .map(str::to_owned)
                .collect();
            if out
                .insert(
                    number.into(),
                    Section {
                        body: title.into(),
                        commands,
                        coin_sources: BTreeSet::new(),
                    },
                )
                .is_some()
            {
                return Err(format!("duplicate CLI reference section {number}"));
            }
            current = Some(number.to_owned());
        } else if let Some(number) = &current {
            out.get_mut(number).unwrap().body.push_str(line);
        }
    }
    for section in out.values_mut().filter(|section| {
        section
            .body
            .lines()
            .next()
            .unwrap_or("")
            .contains("`coins/`")
    }) {
        for prefix in ["coins/", "coins\\"] {
            for (index, _) in section.body.match_indices(prefix) {
                let tail = &section.body[index + prefix.len()..];
                let length = tail
                    .bytes()
                    .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == b'_')
                    .count();
                if length == 0 || !tail[length..].starts_with(".py") {
                    continue;
                }
                let name = &tail[..length];
                let (_, command) = COINS
                    .iter()
                    .find(|(source, _)| *source == name)
                    .ok_or_else(|| format!("unmapped documented coin tool: {name}"))?;
                section.coin_sources.insert(name.into());
                section.commands.insert((*command).into());
            }
        }
        // This additive command has no separate Python entry point in the original inventory.
        if section.body.contains("`coin-rebuild-map ") {
            section.commands.insert("coin-rebuild-map".into());
        }
    }
    Ok(out)
}

pub fn inventory(sections: &BTreeMap<String, Section>) -> Result<BTreeSet<String>, String> {
    if sections.get("12.2").map(|section| &section.commands)
        != Some(&BTreeSet::from(["build-pack".into()]))
    {
        return Err("CLI reference must discover section12.2 build-pack".into());
    }
    if sections
        .get("12.8")
        .is_none_or(|section| section.coin_sources.len() < 5)
    {
        return Err("CLI reference must discover at least five coin source tools".into());
    }
    Ok(sections
        .values()
        .flat_map(|section| section.commands.iter().cloned())
        .collect())
}

pub fn check(
    sections: &BTreeMap<String, Section>,
    help: &BTreeMap<String, String>,
) -> Result<(), String> {
    inventory(sections)?;
    for (number, section) in sections {
        if section.commands.is_empty() {
            continue;
        }
        let mut real = BTreeSet::new();
        for command in &section.commands {
            let text = help
                .get(command)
                .filter(|text| !text.trim().is_empty())
                .ok_or_else(|| format!("missing captured help for {command}"))?;
            real.extend(flags(text));
        }
        real.remove("--help");
        let documented = flags(&section.body);
        let missing = real.difference(&documented).cloned().collect::<Vec<_>>();
        let mentioned: &[&str] = match number.as_str() {
            "12.6" => &["--new-set", "--per-set", "--source"],
            "12.5e" => &["--check"],
            "12.5d" => &["--refresh"],
            _ => &[],
        };
        let stale = documented
            .difference(&real)
            .filter(|flag| !mentioned.contains(&flag.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() || !stale.is_empty() {
            return Err(format!(
                "section{number}: undocumented={missing:?}, unsupported={stale:?}"
            ));
        }
    }
    Ok(())
}

pub fn canaries() {
    let guide = "### 12.2 `build-pack` — build\n--base --title\n\
        ### 12.6 `panel` — curate\n--port --new-set --per-set --source\n\
        ### 12.8 `coins/` commands\ncoins/fetch_logos.py coins/build_keywords.py \
        coins/alias_map.py coins/enhance_map.py coins/write_manifests.py\n\
        --coin-dir --out-dir --state --map --token-env\n";
    let parsed = sections(guide).unwrap();
    assert_eq!(inventory(&parsed).unwrap().len(), 7);
    let helps = [
        ("build-pack", "--base --title --help"),
        ("panel", "--port --help"),
        ("coin-fetch-logos", "--coin-dir"),
        ("coin-build-keywords", "--coin-dir"),
        ("coin-alias-map", "--coin-dir"),
        ("coin-enhance-map", "--coin-dir"),
        (
            "coin-write-manifests",
            "--out-dir --state --map --token-env",
        ),
    ]
    .map(|(name, text)| (name.into(), text.into()))
    .into_iter()
    .collect::<BTreeMap<String, String>>();
    assert!(check(&parsed, &helps).is_ok());
    assert!(inventory(&sections("").unwrap()).is_err());
    assert!(inventory(&sections(&guide.replace("12.2", "12.20")).unwrap()).is_err());
    assert!(inventory(&sections(&guide.replace("coins/enhance_map.py", "")).unwrap()).is_err());
    assert!(sections(&format!("{guide}\n### 12.2 duplicate\n")).is_err());
    assert!(sections(&guide.replace("coins/alias_map.py", "coins/unknown.py")).is_err());
    for (replacement, expected) in [
        (
            "--base --title --new-option",
            "undocumented=[\"--new-option\"]",
        ),
        ("--base", "unsupported=[\"--title\"]"),
        ("", "missing captured help for build-pack"),
    ] {
        let mut changed = helps.clone();
        changed.insert("build-pack".into(), replacement.into());
        assert!(check(&parsed, &changed).unwrap_err().contains(expected));
    }
    assert!(check(&parsed, &BTreeMap::new()).is_err());
    assert_eq!(
        flags("--alpha --alpha (--beta2) a--no _--no ---no Ω--no --UPPER --9bad --help"),
        BTreeSet::from(["--alpha".into(), "--beta2".into(), "--help".into()])
    );
    assert!(!inventory(&parsed).unwrap().contains("emoji-bot"));
}
