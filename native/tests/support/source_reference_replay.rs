//! Relocation provenance uses the shared pinned corpus and explicit original Git identities only.
use _native::{source_git_inventory::Source, source_reference_behavior};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

fn text(root: &Path, relative: &str) -> String {
    let path = root.join(relative);
    _native::tooling_environment::reject_links(&path, "reference evidence cannot follow links")
        .unwrap();
    let file = std::fs::File::open(&path).unwrap();
    let metadata = file.metadata().unwrap();
    assert!(metadata.is_file() && metadata.len() <= 2 * 1024 * 1024);
    let mut bytes = Vec::new();
    file.take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(bytes.len() <= 2 * 1024 * 1024);
    String::from_utf8(bytes).unwrap().replace("\r\n", "\n")
}

pub fn check(root: &Path, runtime: &tokio::runtime::Runtime, corpus: &[Source]) {
    let manifest_text = text(root, "tests/reference/behavior.json");
    use sha2::{Digest, Sha256};
    let digest = |bytes: &[u8]| {
        Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    };
    assert_eq!(
        digest(manifest_text.as_bytes()),
        "ec5aef68ea2a1b33379be87fb4c14c16da9b6a3fff8056a2ec5a80fd47752ab6"
    );
    let manifest: Value = serde_json::from_str(&manifest_text).unwrap();
    let entries = manifest["sources"].as_object().unwrap();
    assert_eq!(entries.len(), 38);
    let actual = corpus
        .iter()
        .filter_map(|source| {
            source
                .path
                .strip_prefix("tests/reference/")
                .filter(|name| !name.contains('/') && *name != "__init__.py")
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(actual, entries.keys().map(String::as_str).collect());
    let current = std::fs::read_dir(root.join("tests/reference"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension().is_some_and(|extension| extension == "py")
                && path.file_name().unwrap() != "__init__.py"
        })
        .map(|path| path.file_name().unwrap().to_str().unwrap().to_owned())
        .collect::<BTreeSet<_>>();
    assert_eq!(current, entries.keys().cloned().collect());
    let mut revisions = BTreeMap::<&str, Vec<&str>>::new();
    for entry in entries.values() {
        revisions
            .entry(entry["source_revision"].as_str().unwrap())
            .or_default()
            .push(entry["source"].as_str().unwrap());
    }
    assert_eq!(revisions.len(), 3);
    let mut history = BTreeMap::new();
    for (revision, paths) in revisions {
        let sources = runtime
            .block_on(_native::source_git_inventory::read_sources(
                root, revision, &paths,
            ))
            .unwrap();
        assert_eq!(sources.len(), paths.len());
        for source in sources {
            assert!(
                history
                    .insert((revision.to_owned(), source.path), source.sha256)
                    .is_none()
            );
        }
    }
    for (name, entry) in entries {
        let path = format!("tests/reference/{name}");
        let source = corpus.iter().find(|source| source.path == path).unwrap();
        assert_eq!(
            text(root, &path),
            source.text,
            "current reference bytes drift: {name}"
        );
        assert_eq!(
            source_reference_behavior::digest(&source.text).unwrap(),
            entry["relocated_behavior_sha256"],
            "{name}"
        );
        let revision = entry["source_revision"].as_str().unwrap();
        let path = entry["source"].as_str().unwrap();
        assert_eq!(
            history[&(revision.to_owned(), path.to_owned())],
            entry["source_sha256"],
            "{name}"
        );
    }
    let ledger = text(root, "tests/oracles/README.md");
    let mut snapshots = BTreeMap::new();
    for line in ledger.lines() {
        let fields = line.split('|').map(str::trim).collect::<Vec<_>>();
        if fields.len() == 4
            && fields[1].ends_with(".py")
            && fields[2].len() == 64
            && fields[2]
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            assert!(!fields[1].contains(['/', '\\', ':']));
            assert!(snapshots.insert(fields[1], fields[2]).is_none());
        }
    }
    assert_eq!(snapshots.len(), 20);
    for (name, expected) in snapshots {
        let path = format!("tests/oracles/{name}");
        let source = corpus.iter().find(|source| source.path == path).unwrap();
        assert_eq!(source.sha256, expected, "original oracle: {name}");
        let path = root.join(&path);
        _native::tooling_environment::reject_links(&path, "oracle snapshot cannot follow links")
            .unwrap();
        let file = std::fs::File::open(path).unwrap();
        let metadata = file.metadata().unwrap();
        assert!(metadata.is_file() && metadata.len() <= 2 * 1024 * 1024);
        let mut bytes = Vec::new();
        file.take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .unwrap();
        assert!(bytes.len() <= 2 * 1024 * 1024);
        assert_eq!(digest(&bytes), expected, "current oracle bytes: {name}");
    }
}
