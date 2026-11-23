// Ported from native/tests/roster_pack_parity.rs at a39a049c02f8dcc90d18fe8771869f82dd655221.
// Original source SHA256: 39843b2a1707b9b02b060fa599ada181aa00243595846eed05a65bbf662ff763.
// Original test identities and behavioral assertions retained against the production crate.
extern crate numera_runtime as _native;

// Source-generated whole-pack output protects identity/history joins and brand exceptions.
use serde_json::Value;
use std::path::PathBuf;

#[test]
fn live_pack_provenance_and_history_match_pinned_source() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let fixture: Value = serde_json::from_slice(
        &std::fs::read(root.join("tests/fixtures/backend/roster-pack.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(fixture["source"], "d173116:pack_manifest.build_pack");
    let cases = fixture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4);
    let case = &fixture["index_case"];
    let index = _native::roster_index::build(
        case["documents"].as_array().unwrap(),
        "general",
        &case["old"],
        &case["inputs"],
        "2026-10-07 00:00:00 UTC",
    )
    .unwrap();
    assert_eq!(index, case["expected"]);
    assert_eq!(
        _native::roster_markdown::index(&index).unwrap(),
        case["markdown"].as_str().unwrap()
    );
    for case in cases {
        let actual = _native::roster::pack(
            &case["record"],
            &case["pack"],
            case["family"].as_str().unwrap(),
            &case["provenance"],
            &case["prior"],
            "2026-10-07 00:00:00 UTC",
        )
        .unwrap();
        assert_eq!(actual, case["expected"], "{}", case["name"]);
        assert_eq!(
            _native::roster_markdown::pack(&actual).unwrap(),
            case["markdown"].as_str().unwrap()
        );
    }
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
