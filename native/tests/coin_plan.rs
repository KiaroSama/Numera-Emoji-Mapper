//! Pinned source plan representatives/keyword order/alias replay without file or live mutations.
use serde_json::Value;

#[test]
fn frozen_coin_groups_and_alias_replay_match_source() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/native/coin-plan.json")).unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(
        fixture["source_sha256"]["coins/_dedup_plan.py"],
        "d5fe1edbd9ecf37cc59eada51b2108a756d49edece5b1fd47db4a1b325ec5bd3"
    );
    for case in fixture["plans"].as_array().unwrap() {
        let inventory: Vec<String> = serde_json::from_value(case["inventory"].clone()).unwrap();
        let result = _native::coin_plan::group(
            case["files"].as_array().unwrap(),
            &inventory,
            case["keywords"].as_object().unwrap(),
        )
        .unwrap();
        assert_eq!(result, case["expected"]);
    }
    for case in fixture["replays"].as_array().unwrap() {
        let approved: Vec<String> = serde_json::from_value(case["approved"].clone()).unwrap();
        assert_eq!(
            _native::coin_plan::replay(
                case["new"].as_object().unwrap(),
                case["current"].as_object().unwrap(),
                &approved
            )
            .unwrap(),
            case["expected"]
        );
    }
    assert!(
        _native::coin_plan::group(&[serde_json::json!({})], &[], &serde_json::Map::new()).is_err()
    );
}

#[test]
fn real_operator_map_keeps_unreviewed_shared_groups_below_the_original_limit() {
    use std::io::Read;
    use std::path::{Path, PathBuf};

    let Some(selected) = std::env::var_os("NUMERA_REAL_COIN_DIRECTORY") else {
        eprintln!("local-by-design: real coin directory not selected; no real-data parity claim");
        return;
    };
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let project = _native::tooling_environment::physical_root(project).unwrap();
    let selected = PathBuf::from(selected);
    assert!(
        selected.is_absolute(),
        "real coin directory must be absolute"
    );
    _native::tooling_environment::reject_links(
        &selected,
        "real coin directory cannot follow links",
    )
    .ok()
    .expect("real coin directory admission failed");
    let directory = _native::tooling_environment::physical_root(&selected)
        .ok()
        .expect("real coin directory is unavailable");
    assert!(
        directory.starts_with(&project),
        "real coin directory must stay project-contained"
    );
    let read = |name: &str| {
        let path = directory.join(name);
        _native::tooling_environment::reject_links(&path, "real coin input cannot follow links")
            .ok()
            .expect("real coin input admission failed");
        let file = std::fs::File::open(&path).expect("real coin input is unavailable");
        let metadata = file.metadata().expect("cannot inspect real coin input");
        assert!(
            metadata.is_file() && metadata.len() <= 2 * 1024 * 1024,
            "real coin input must be a regular file under2MiB"
        );
        let mut bytes = Vec::new();
        file.take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .expect("cannot read real coin input");
        assert!(
            bytes.len() <= 2 * 1024 * 1024,
            "real coin input grew beyond2MiB"
        );
        bytes
    };
    let mapping_bytes = read("ticker_to_id.json");
    let groups_bytes = read("shared_logo_groups.json");
    // Never use Debug assertions or raw parse errors on private tickers and IDs.
    let mapping: Value = serde_json::from_slice(&mapping_bytes)
        .ok()
        .expect("real coin map is not valid JSON");
    let mapping = mapping
        .as_object()
        .expect("real coin map must be an object");
    assert!(!mapping.is_empty(), "real coin fixture cannot be empty");
    assert!(
        mapping
            .values()
            .all(|value| value.as_str().is_some_and(|id| !id.is_empty())),
        "real coin map IDs must be nonempty strings"
    );
    let groups: Value = serde_json::from_slice(&groups_bytes)
        .ok()
        .expect("real shared groups are not valid JSON");
    let groups = groups
        .as_object()
        .expect("real shared groups must be an object");
    let mut approved = Vec::new();
    for group in groups.values() {
        for ticker in group.as_array().expect("real shared group must be a list") {
            approved.push(
                ticker
                    .as_str()
                    .expect("real shared ticker must be a string")
                    .to_owned(),
            );
        }
    }
    let observed = _native::coin_plan::replay(mapping, &serde_json::Map::new(), &approved)
        .ok()
        .expect("real coin shared-group analysis failed");
    let bad = observed["unapproved"]
        .as_object()
        .expect("shared-group result is missing");
    let biggest = bad
        .values()
        .map(|group| group.as_array().unwrap().len())
        .max()
        .unwrap_or(0);
    assert!(
        biggest < 20,
        "real coin map has an unreviewed group of20or more"
    );
    assert!(
        read("ticker_to_id.json") == mapping_bytes,
        "real coin source map changed"
    );
    assert!(
        read("shared_logo_groups.json") == groups_bytes,
        "real shared-group source changed"
    );
}

#[test]
fn original_shared_logo_groups_require_explicit_review_without_flagging_unique_ids() {
    use serde_json::json;
    for (mapping, approved, expected) in [
        (
            json!({"apt":"SAME","arkm":"SAME","hbar":"SAME","near":"SAME","tao":"SAME","btc":"OWN"}),
            vec![],
            json!({"SAME":["apt","arkm","hbar","near","tao"]}),
        ),
        (
            json!({"usdt":"T","usdtbsc":"T","usdterc20":"T"}),
            vec!["usdt".into(), "usdtbsc".into(), "usdterc20".into()],
            json!({}),
        ),
        (json!({"btc":"1","eth":"2","sol":"3"}), vec![], json!({})),
    ] {
        let actual = _native::coin_plan::replay(
            mapping.as_object().unwrap(),
            &serde_json::Map::new(),
            &approved,
        )
        .unwrap();
        assert_eq!(actual["mapping"], mapping);
        assert_eq!(actual["unapproved"], expected);
        assert_eq!(actual["missing"], json!([]));
    }
}
