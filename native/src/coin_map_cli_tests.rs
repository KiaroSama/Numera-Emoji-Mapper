//! Original acquisition-time updates and interrupted commits use the actual map command and writer.
use crate::{coin_map_cli, config::Config, locks::Lease, logging::RunLog};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::{Duration, Instant};

mod tooling_fixture {
    use crate as _native;
    include!("../tests/support/tooling_fixture.rs");
}
use tooling_fixture::Root;

fn seed(root: &Path, alias: bool) -> (Config, Value) {
    let directory = root.join("coins");
    std::fs::create_dir(&directory).unwrap();
    let initial = if alias {
        json!({"ddd":"333"})
    } else {
        json!({"btc":"111"})
    };
    crate::atomic::write_json(&directory.join("ticker_to_id.json"), &initial).unwrap();
    let inventory = if alias {
        "## 🟡 — Foo Protocol\n   ticker: ccc\n   premium-id:\n\n## 🟢 — Bar Coin\n   ticker: eee\n   premium-id:\n"
    } else {
        "   ticker: btcbsc\n   premium-id:\n"
    };
    std::fs::write(
        directory.join("currency-emoji-inventory.md"),
        inventory.as_bytes(),
    )
    .unwrap();
    std::fs::write(
        directory.join("keywords.csv"),
        b"ticker,name,format,file,keywords\nddd,Bar Labs,svg,logos/svg/ddd.svg,ddd\n",
    )
    .unwrap();
    let config = Config::load_with_environment(
        root.into(),
        &BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]),
    )
    .unwrap();
    (config, initial)
}

fn released(root: &Path) {
    let path = root.join(".locks/canonical_map.lock");
    assert!(
        path.is_file(),
        "ordinary release must retain the stable lock"
    );
    drop(Lease::acquire(&path, "original map release").unwrap());
    assert!(!crate::process_owner::cleanup_pending(root));
}

#[test]
fn original_alias_and_enhance_reread_the_map_after_actual_lock_acquisition() {
    let deadline = Instant::now() + Duration::from_secs(10);
    for alias in [true, false] {
        let root = Root::new("coin map acquisition-time update");
        let (config, initial) = seed(&root.0, alias);
        let directory = root.0.join("coins");
        let path = directory.join("ticker_to_id.json");
        let inventory = std::fs::read(directory.join("currency-emoji-inventory.md")).unwrap();
        let log = RunLog::open(&root.0, "coin-map-race-test", vec![]);
        let command = if alias {
            "coin-alias-map"
        } else {
            "coin-enhance-map"
        };
        let mut acquisitions = 0;
        let code = coin_map_cli::run_with(
            &config,
            command,
            &[],
            &log,
            |lock| {
                acquisitions += 1;
                assert_eq!(lock, root.0.join(".locks/canonical_map.lock"));
                let lease = Lease::acquire(lock, "original acquisition race")?;
                assert!(Lease::acquire(lock, "must remain exclusive").is_err());
                let mut current: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
                assert_eq!(current, initial);
                // This is deliberately after the real acquisition, not prepopulated setup.
                current["zzz"] = json!("written-by-the-other-tool");
                crate::atomic::write_json(&path, &current)?;
                Ok(lease)
            },
            |path, value| crate::atomic::write_json(path, value),
        )
        .unwrap();
        assert_eq!(code, 0);
        assert_eq!(acquisitions, 1);
        let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(
            saved,
            if alias {
                json!({"ddd":"333","eee":"333","zzz":"written-by-the-other-tool"})
            } else {
                json!({"btc":"111","btcbsc":"111","zzz":"written-by-the-other-tool"})
            }
        );
        let filled = std::fs::read_to_string(directory.join("currency-emoji-inventory.filled.md"))
            .unwrap()
            .replace("\r\n", "\n");
        if alias {
            assert!(filled.contains("ticker: ccc\n   premium-id:\n"));
            assert!(filled.contains("ticker: eee\n   premium-id: 333\n"));
        } else {
            assert_eq!(filled, "   ticker: btcbsc\n   premium-id: 111\n");
        }
        assert_eq!(
            std::fs::read(directory.join("currency-emoji-inventory.md")).unwrap(),
            inventory
        );
        released(&root.0);
        log.finish(0);
        drop(log);
        assert!(Instant::now() < deadline);
        root.clean().unwrap();
    }
}

#[test]
fn original_interrupted_alias_commit_preserves_canonical_bytes_and_stops_before_refill() {
    let root = Root::new("coin map interrupted commit");
    let (config, initial) = seed(&root.0, true);
    let directory = root.0.join("coins");
    let path = directory.join("ticker_to_id.json");
    let before = std::fs::read(&path).unwrap();
    let inventory = std::fs::read(directory.join("currency-emoji-inventory.md")).unwrap();
    let keywords = std::fs::read(directory.join("keywords.csv")).unwrap();
    let log = RunLog::open(&root.0, "coin-map-interrupted-test", vec![]);
    let started = Instant::now();
    let mut commits = 0;
    let mut replacements = 0;
    let error = coin_map_cli::run_with(
        &config,
        "coin-alias-map",
        &[],
        &log,
        |path| Lease::acquire(path, "original interrupted commit"),
        |destination, value| {
            commits += 1;
            assert_eq!(destination, path);
            assert_eq!(value, &json!({"ddd":"333","eee":"333"}));
            assert!(
                Lease::acquire(
                    &root.0.join(".locks/canonical_map.lock"),
                    "commit still owns map"
                )
                .is_err()
            );
            let bytes = serde_json::to_vec(value)?;
            crate::atomic::write_bytes_with(
                destination,
                &bytes,
                std::fs::File::sync_all,
                |staged, target| {
                    replacements += 1;
                    assert_eq!(target, path);
                    assert_eq!(std::fs::read(staged).unwrap(), bytes);
                    assert_eq!(std::fs::read(target).unwrap(), before);
                    Err(std::io::Error::other("interrupted"))
                },
            )
        },
    )
    .unwrap_err();
    assert_eq!(error, "cannot persist canonical coin map");
    assert_eq!((commits, replacements), (1, 1));
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(serde_json::from_slice::<Value>(&before).unwrap(), initial);
    assert_eq!(
        std::fs::read(directory.join("currency-emoji-inventory.md")).unwrap(),
        inventory
    );
    assert_eq!(
        std::fs::read(directory.join("keywords.csv")).unwrap(),
        keywords
    );
    let names = std::fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        names,
        BTreeSet::from([
            "currency-emoji-inventory.md".into(),
            "keywords.csv".into(),
            "ticker_to_id.json".into(),
        ])
    );
    released(&root.0);
    assert!(started.elapsed() < Duration::from_secs(10));
    log.finish(0);
    drop(log);
    root.clean().unwrap();
}
