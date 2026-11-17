// Ported from native/tests/coin_replace_proof.rs at a32dbab6e3e694f1fac9656997fa2edb6e3b2ede.
// Original source SHA256: f1e82aa0732b165c16859d00aeb8c86b73074cc89f8add6e8dda7d0a6bf50841.
// Original test identities and behavioral assertions retained against the production crate.
extern crate numera_runtime as _native;

// Pinned full-manifest evidence, not positional guessing, proves a replacement CID.
use serde_json::Value;
#[test]
fn source_replacement_id_requires_new_identity_and_unchanged_other_ids() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/coin-replace-proof.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_sha256"],
        "86f17862ebe5019e05965fa7aa30380a20226b3632f192f051525290ae7f7031"
    );
    for case in fixture["cases"].as_array().unwrap() {
        let rows = |key: &str| {
            case[key]
                .as_array()
                .unwrap()
                .iter()
                .map(|value| value.as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            serde_json::json!(_native::coin_replace_proof::new_id(
                &rows("before"),
                &rows("after"),
                case["position"].as_i64().unwrap()
            )),
            case["expected"]
        );
    }
    // Original VerifyLogosFix literals retain the exact neighbors and old/new identities.
    let before = vec!["a".into(), "cid-old".into()];
    for (after, expected) in [
        (["a", "cid-new"], Some("cid-new")),
        (["z", "cid-new"], None),
        (["a", "a"], None),
        (["a", "cid-old"], None),
    ] {
        let after = after.into_iter().map(str::to_owned).collect::<Vec<_>>();
        assert_eq!(
            _native::coin_replace_proof::new_id(&before, &after, 1).as_deref(),
            expected
        );
    }
    assert!(_native::coin_replace_proof::new_id(&before, &["cid-new".into()], 1).is_none());
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
