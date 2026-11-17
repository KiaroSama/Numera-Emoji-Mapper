//! Exact map-acquisition race under the actual replacement flow and real OS leases.
use crate as rt;
use crate::baseline_coin_provider_support::*;
use serde_json::json;
// tests.test_verify_logos.VerifyLogosFix.test_a_map_writer_cannot_repoint_the_ticker_mid_replacement
#[test]
fn test_a_map_writer_cannot_repoint_the_ticker_mid_replacement() {
    let f = Verify::new();
    write(&f.files.map, &json!({"btc":OLD}));
    let _family = rt::locks::Lease::acquire(
        &rt::locks::pack_family_path(&f.root.0, &f.files.base),
        "replacement caller family",
    )
    .unwrap();
    let server = Server::new(vec![listing(&f.before())]);
    let mut observed = false;
    let result = super::replace_locked_with_map_lease(
        &f.files,
        &server.telegram(),
        "btc",
        "Bitcoin",
        &noise("official-btc"),
        42,
        |path| {
            let lease = rt::locks::Lease::acquire(path, "real racing map writer")?;
            write(
                &f.files.map,
                &json!({"btc":"cid-written-by-the-other-tool"}),
            );
            observed = true;
            Ok(lease)
        },
    )
    .unwrap();
    assert!(observed);
    assert!(!result);
    let requests = server.finish();
    assert!(requests.iter().all(|r| r.method != "replaceStickerInSet"));
    assert_eq!(
        doc(&f.files.map),
        json!({"btc":"cid-written-by-the-other-tool"})
    );
    assert!(!f.files.intent.exists());
    f.unchanged();
}
