use super::*;
fn perform(f: &Verify, kind: &str) -> (bool, Vec<Request>) {
    let mut script = vec![
        listing(&f.before()),
        f.mutation(if kind == "lost-reply" {
            Body::Disconnect
        } else if kind == "rejected" {
            Body::Json(reject("replaceStickerInSet failed: STICKER_INVALID"))
        } else {
            Body::Json(ok(json!(true)))
        }),
    ];
    if kind == "lost-reply" {
        script.push(listing(&f.after()));
    }
    if kind != "rejected" {
        script.push(if kind == "unchanged" {
            listing(&f.before())
        } else if kind == "shifted" {
            listing(&[sticker(NEW), sticker("c")])
        } else {
            listing(&f.after())
        });
        if !matches!(kind, "unchanged" | "shifted") {
            if kind == "foreign" {
                download(&mut script, &format!("fid-{NEW}"), &noise("stranger"));
            } else {
                uploaded_download(&mut script, &format!("fid-{NEW}"));
            }
        }
    }
    let server = Server::new(script);
    let result = f.run(&server).unwrap();
    (result, server.finish())
}
// tests.test_verify_logos.VerifyLogosFix.test_timeout_after_apply_is_verified_not_resent
#[test]
fn test_timeout_after_apply_is_verified_not_resent() {
    let f = Verify::new();
    let (ok, r) = perform(&f, "lost-reply");
    assert!(ok);
    assert_eq!(
        r.iter()
            .filter(|r| r.method == "replaceStickerInSet")
            .count(),
        1
    );
    assert_eq!(doc(&f.files.map)["btc"], NEW);
}
// tests.test_verify_logos.VerifyLogosFix.test_the_uploaded_body_is_bytes_a_retry_can_resend
#[test]
fn test_the_uploaded_body_is_bytes_a_retry_can_resend() {
    let f = Verify::new();
    let (_, r) = perform(&f, "lost-reply");
    let body = &r
        .iter()
        .find(|r| r.method == "replaceStickerInSet")
        .unwrap()
        .files["file0"];
    assert!(body.starts_with(b"\x89PNG"));
    assert!(image::load_from_memory(body).is_ok());
}
// tests.test_verify_logos.VerifyLogosFix.test_every_entry_sharing_the_old_cid_is_repointed
#[test]
fn test_every_entry_sharing_the_old_cid_is_repointed() {
    let f = Verify::new();
    let (ok, r) = perform(&f, "normal");
    assert!(ok);
    assert_eq!(
        doc(&f.files.map),
        json!({"btc":NEW,"wbtc":NEW,"eth":"cid-eth"})
    );
    assert!(!f.files.intent.exists());
    let body = &r
        .iter()
        .find(|r| r.method == "replaceStickerInSet")
        .unwrap()
        .files["file0"];
    assert_eq!(
        &std::fs::read(f.files.images.join("btc.png")).unwrap(),
        body
    );
    assert_eq!(
        std::fs::read(f.root.0.join("png/btc.png")).unwrap(),
        noise("official-btc")
    );
    assert_eq!(
        std::fs::read_dir(f.root.0.join(".incoming"))
            .unwrap()
            .count(),
        0
    );
}
// tests.test_verify_logos.VerifyLogosFix.test_a_definite_failure_leaves_the_oracle_untouched
#[test]
fn test_a_definite_failure_leaves_the_oracle_untouched() {
    let f = Verify::new();
    assert!(!perform(&f, "rejected").0);
    f.unchanged();
    assert!(!f.files.intent.exists());
}
// tests.test_verify_logos.VerifyLogosFix.test_a_shifted_set_is_not_trusted_as_the_replacement
#[test]
fn test_a_shifted_set_is_not_trusted_as_the_replacement() {
    let f = Verify::new();
    let before = doc(&f.files.map);
    assert!(!perform(&f, "shifted").0);
    assert_eq!(doc(&f.files.map), before);
    f.unchanged();
}
// tests.test_verify_logos.VerifyLogosFix.test_an_unchanged_set_is_not_trusted_either
#[test]
fn test_an_unchanged_set_is_not_trusted_either() {
    let f = Verify::new();
    let before = doc(&f.files.map);
    assert!(!perform(&f, "unchanged").0);
    assert_eq!(doc(&f.files.map), before);
    f.unchanged();
    assert!(!f.files.intent.exists());
}
// tests.test_verify_logos.VerifyLogosFix.test_a_replacement_whose_image_is_not_ours_is_refused
#[test]
fn test_a_replacement_whose_image_is_not_ours_is_refused() {
    let f = Verify::new();
    let before = doc(&f.files.map);
    assert!(!perform(&f, "foreign").0);
    assert_eq!(doc(&f.files.map), before);
    f.unchanged();
}
// tests.test_verify_logos.VerifyLogosFix.test_a_lost_confirmation_read_leaves_a_recoverable_intent
#[test]
fn test_a_lost_confirmation_read_leaves_a_recoverable_intent() {
    let f = Verify::new();
    let before = doc(&f.files.map);
    f.pending();
    assert_eq!(doc(&f.files.map), before);
    let intent = doc(&f.files.intent);
    assert_eq!(intent["operation"], "replace");
    assert_eq!(intent["key"], "btc");
    assert_eq!(intent["old_cid"], OLD);
    assert_eq!(intent["set_name"], SET);
    assert_eq!(intent["set_index"], 1);
    assert_eq!(intent["before"], json!(["a", OLD, "c"]));
}
// tests.test_verify_logos.VerifyLogosFix.test_the_next_run_recovers_the_replacement_from_the_intent
#[test]
fn test_the_next_run_recovers_the_replacement_from_the_intent() {
    let f = Verify::new();
    let uploaded = f.pending();
    f.unchanged();
    let stage = std::path::PathBuf::from(doc(&f.files.intent)["stage"].as_str().unwrap());
    assert!(stage.is_dir());
    let r = f.recovered(&uploaded);
    assert!(r.iter().all(|r| r.method != "replaceStickerInSet"));
    assert!(!stage.exists());
}
// tests.test_verify_logos.VerifyLogosFix.test_recovery_refuses_when_the_live_image_is_not_ours
#[test]
fn test_recovery_refuses_when_the_live_image_is_not_ours() {
    let f = Verify::new();
    f.pending();
    let before = doc(&f.files.map);
    let mut script = vec![listing(&f.after())];
    download(&mut script, &format!("fid-{NEW}"), &noise("stranger"));
    let server = Server::new(script);
    assert!(!rt::coin_replace_disk::run(&f.files, &server.telegram()).unwrap());
    server.finish();
    assert_eq!(doc(&f.files.map), before);
    assert!(f.files.intent.exists());
    f.unchanged();
}
// tests.test_verify_logos.VerifyLogosFix.test_recovery_is_idempotent_after_the_map_was_already_repointed
#[test]
fn test_recovery_is_idempotent_after_the_map_was_already_repointed() {
    let f = Verify::new();
    let bytes = f.pending();
    write(&f.files.map, &json!({"btc":NEW,"wbtc":NEW,"eth":"cid-eth"}));
    f.recovered(&bytes);
}
// tests.test_verify_logos.VerifyLogosFix.test_an_unreadable_set_leaves_the_intent_for_the_run_after
#[test]
fn test_an_unreadable_set_leaves_the_intent_for_the_run_after() {
    let f = Verify::new();
    f.pending();
    let server = Server::new(vec![unavailable()]);
    assert!(!rt::coin_replace_disk::run(&f.files, &server.telegram()).unwrap());
    server.finish();
    assert!(f.files.intent.exists());
    f.unchanged();
}
// tests.test_verify_logos.VerifyLogosFix.test_an_intent_cannot_be_reconciled_into_a_different_map
#[test]
fn test_an_intent_cannot_be_reconciled_into_a_different_map() {
    let mut f = Verify::new();
    let bytes = f.pending();
    let original = f.files.map.clone();
    let other = f.root.0.join("other_map.json");
    write(&other, &json!({"btc":OLD}));
    f.files.map = other.clone();
    let server = Server::new(vec![]);
    assert!(!rt::coin_replace_disk::run(&f.files, &server.telegram()).unwrap());
    server.finish();
    assert_eq!(doc(&other), json!({"btc":OLD}));
    assert!(f.files.intent.exists());
    f.files.map = original;
    f.recovered(&bytes);
}
// tests.test_verify_logos.VerifyLogosFix.test_an_intent_is_bound_to_its_state_file_too
#[test]
fn test_an_intent_is_bound_to_its_state_file_too() {
    let mut f = Verify::new();
    let state_a = f.root.0.join("state_a.json");
    let state_b = f.root.0.join("state_b.json");
    write(&state_a, &doc(&f.files.state));
    write(&state_b, &doc(&f.files.state));
    f.files.state = state_a;
    let bytes = f.pending();
    let original = f.files.state.clone();
    let before = doc(&f.files.map);
    f.files.state = state_b;
    let server = Server::new(vec![]);
    assert!(!rt::coin_replace_disk::run(&f.files, &server.telegram()).unwrap());
    server.finish();
    assert_eq!(doc(&f.files.map), before);
    f.files.state = original;
    f.recovered(&bytes);
}
// tests.test_verify_logos.VerifyLogosFix.test_an_intent_with_no_recorded_target_is_refused
#[test]
fn test_an_intent_with_no_recorded_target_is_refused() {
    let f = Verify::new();
    f.pending();
    let mut legacy = doc(&f.files.intent);
    legacy.as_object_mut().unwrap().remove("map_target");
    legacy.as_object_mut().unwrap().remove("state_target");
    write(&f.files.intent, &legacy);
    let before = doc(&f.files.map);
    let server = Server::new(vec![]);
    assert!(!rt::coin_replace_disk::run(&f.files, &server.telegram()).unwrap());
    server.finish();
    assert_eq!(doc(&f.files.map), before);
    assert!(f.files.intent.exists());
}
// tests.test_verify_logos.VerifyLogosFix.test_a_busy_map_lock_stops_the_fix_before_it_touches_the_pack
#[test]
fn test_a_busy_map_lock_stops_the_fix_before_it_touches_the_pack() {
    let f = Verify::new();
    let before = doc(&f.files.map);
    let _map = rt::locks::Lease::acquire(
        &f.root.0.join(".locks/canonical_map.lock"),
        "actual foreign map writer",
    )
    .unwrap();
    let server = Server::new(vec![]);
    assert!(f.run(&server).is_err());
    assert!(server.finish().is_empty());
    assert_eq!(doc(&f.files.map), before);
    assert!(!f.files.intent.exists());
    f.unchanged();
}
// tests.test_verify_logos.VerifyLogosFix.test_fix_locks_on_the_pack_family_not_on_this_script
#[test]
fn test_fix_locks_on_the_pack_family_not_on_this_script() {
    let f = Verify::new();
    assert_eq!(f.files.base, "cryptoemoji");
    let family = rt::locks::pack_family_path(&f.root.0, "cryptoemoji");
    assert_eq!(
        family,
        rt::locks::pack_family_path(&f.root.0, &f.files.base)
    );
    let _hold = rt::locks::Lease::acquire(&family, "actual provider family holder").unwrap();
    let server = Server::new(vec![]);
    assert!(f.run(&server).is_err());
    assert!(server.finish().is_empty());
    assert!(!f.files.intent.exists());
}
// tests.test_verify_logos.VerifyLogosFix.test_verified_new_cid_rules
#[test]
fn test_verified_new_cid_rules() {
    let before = ids(&["a", OLD]);
    assert_eq!(
        rt::coin_replace_proof::new_id(&before, &ids(&["a", NEW]), 1),
        Some(NEW.into())
    );
    for after in [vec![NEW], vec!["z", NEW], vec!["a", "a"], vec!["a", OLD]] {
        assert_eq!(
            rt::coin_replace_proof::new_id(&before, &ids(&after), 1),
            None,
            "{after:?}"
        );
    }
}
