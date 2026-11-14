//! Source-observed cutoff and runner-up policy never maps by sticker position.
use _native::coin_remap_policy::{Match, Options, decide};
use serde_json::{Value, json};

#[test]
fn source_remap_admission_keeps_dry_candidates_margin_and_complete_only_apply() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/coin-remap-policy.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_revision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(
        fixture["source_sha256"],
        "b66136d42adc3448d44e67c36c9f8c827a4e63d6ce259af5d6b437299b043ed7"
    );
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let matches = (0..2)
            .map(|index| Match {
                ticker: if index == 0 { "aaa" } else { "bbb" }.into(),
                cid: if index == 0 { "c1" } else { "c2" }.into(),
                distance: input["d2"][index].as_f64().unwrap().sqrt(),
                margin: input["second"][index].as_f64().unwrap().sqrt()
                    - input["d2"][index].as_f64().unwrap().sqrt(),
            })
            .collect::<Vec<_>>();
        let result = decide(
            &matches,
            Options {
                apply: input["apply"].as_bool().unwrap(),
                max_distance: input["max"].as_f64(),
                min_margin: input["margin"].as_f64(),
                incomplete_sets: input["incomplete"].as_array().map_or(0, Vec::len),
                failed_images: input["errors"].as_object().map_or(0, serde_json::Map::len),
            },
        )
        .unwrap();
        assert_eq!(
            u64::from(result.code),
            case["expected"]["code"].as_u64().unwrap()
        );
        let writes = if let Some(destination) = result.destination {
            json!([[destination, result.mapping]])
        } else {
            json!([])
        };
        assert_eq!(writes, case["expected"]["writes"]);
        assert_eq!(
            result.backup,
            !case["expected"]["backups"].as_array().unwrap().is_empty()
        );
    }
    let options = Options {
        apply: true,
        max_distance: Some(1.0),
        min_margin: None,
        incomplete_sets: 0,
        failed_images: 0,
    };
    assert!(
        decide(
            &[Match {
                ticker: "x".into(),
                cid: "c".into(),
                distance: f64::NAN,
                margin: 1.0
            }],
            options
        )
        .is_err()
    );
    assert!(
        decide(
            &[],
            Options {
                max_distance: Some(0.0),
                ..options
            }
        )
        .is_err()
    );
}
