//! The actual additive map command sees concurrent data only after taking the real canonical lease.
use crate as rt;
use crate::baseline_coin_state_support::*;
use serde_json::json;
// tests.test_coin_ticker_map.AliasMapAmbiguity.test_an_interrupted_write_leaves_the_canonical_map_intact
#[test]
fn test_an_interrupted_write_leaves_the_canonical_map_intact() {
    let root = alias_fixture();
    let path = root.0.join("ticker_to_id.json");
    let original = doc(&path);
    let mut attempts = vec![];
    let result = super::run_with(
        &crate::baseline_coin_provider_support::config(&root.0, &[]),
        "coin-alias-map",
        &ids(&["--coin-dir", root.0.to_str().unwrap()]),
        &root.log(),
        |p| rt::locks::Lease::acquire(p, "actual alias ownership"),
        |p, _| {
            assert_eq!(p, path);
            assert!(
                rt::locks::Lease::acquire(
                    &rt::locks::canonical_map_path(&root.0),
                    "actual competing writer at persist"
                )
                .is_err()
            );
            attempts.push(p.to_owned());
            Err(std::io::Error::other("interrupted"))
        },
    );
    assert!(result.is_err());
    assert_eq!(attempts, vec![path.clone()]);
    assert_eq!(doc(&path), original);
    drop(
        rt::locks::Lease::acquire(
            &rt::locks::canonical_map_path(&root.0),
            "error release positive control",
        )
        .unwrap(),
    );
}
fn race(enhance: bool) -> Root {
    let root = Root::new();
    let path = root.0.join("ticker_to_id.json");
    write(
        &path,
        &if enhance {
            json!({"btc":"111"})
        } else {
            json!({"ddd":"333"})
        },
    );
    std::fs::write(
        root.0.join("currency-emoji-inventory.md"),
        if enhance {
            "   ticker: btcbsc\n   premium-id:\n"
        } else {
            NAME_INVENTORY
        },
    )
    .unwrap();
    std::fs::write(
        root.0.join("keywords.csv"),
        "ticker,name,format,file,keywords\nddd,Bar Labs,svg,logos/svg/ddd.svg,ddd\n",
    )
    .unwrap();
    let mut acquisitions = 0;
    let code = super::run_with(
        &crate::baseline_coin_provider_support::config(&root.0, &[]),
        if enhance {
            "coin-enhance-map"
        } else {
            "coin-alias-map"
        },
        &ids(&["--coin-dir", root.0.to_str().unwrap()]),
        &root.log(),
        |p| {
            assert_eq!(p, rt::locks::canonical_map_path(&root.0));
            let lease = rt::locks::Lease::acquire(p, "actual racing writer")?;
            let mut current = doc(&path);
            current["zzz"] = json!("written-by-the-other-tool");
            rt::atomic::write_json(&path, &current)?;
            acquisitions += 1;
            Ok(lease)
        },
        rt::atomic::write_json,
    )
    .unwrap();
    assert_eq!(code, 0);
    assert_eq!(acquisitions, 1);
    drop(
        rt::locks::Lease::acquire(
            &rt::locks::canonical_map_path(&root.0),
            "post-command ownership release",
        )
        .unwrap(),
    );
    root
}
// tests.test_coin_ticker_map.CanonicalMapWritersCannotLoseAnUpdate.test_alias_map_keeps_the_other_writers_ids
#[test]
fn test_alias_map_keeps_the_other_writers_ids() {
    let root = race(false);
    assert_eq!(
        doc(&root.0.join("ticker_to_id.json")),
        json!({"ddd":"333","eee":"333","zzz":"written-by-the-other-tool"})
    );
}
// tests.test_coin_ticker_map.CanonicalMapWritersCannotLoseAnUpdate.test_enhance_map_keeps_the_other_writers_ids
#[test]
fn test_enhance_map_keeps_the_other_writers_ids() {
    let root = race(true);
    assert_eq!(
        doc(&root.0.join("ticker_to_id.json")),
        json!({"btc":"111","btcbsc":"111","zzz":"written-by-the-other-tool"})
    );
}
