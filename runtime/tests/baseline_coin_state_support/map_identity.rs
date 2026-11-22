use super::*;
use numera_runtime as rt;
use serde_json::json;
use std::sync::{Arc, Mutex};
// tests.test_rebuild_dedup_map.MapIsResolvedByImageIdentity.test_the_untouched_pack_maps_by_content
#[test]
fn test_the_untouched_pack_maps_by_content() {
    let f = mapping_fixture();
    let server = Server::new(mapping_script(&["aaa", "bbb", "ccc"]));
    f.map(&server).unwrap();
    server.finish();
    assert_eq!(f.mapping(), json!({"aaa":"s1-0","bbb":"s1-1","ccc":"s1-2"}));
    f.released();
}
// tests.test_rebuild_dedup_map.MapIsResolvedByImageIdentity.test_a_same_length_reorder_is_never_mapped_by_position
#[test]
fn test_a_same_length_reorder_is_never_mapped_by_position() {
    let f = mapping_fixture();
    let server = Server::new(mapping_script(&["ccc", "bbb", "aaa"]));
    f.map(&server).unwrap();
    server.finish();
    assert_eq!(f.mapping()["aaa"], "s1-2");
    assert_eq!(f.mapping()["ccc"], "s1-0");
    assert_ne!(f.mapping()["aaa"], "s1-0");
    f.released();
}
// tests.test_rebuild_dedup_map.MapIsResolvedByImageIdentity.test_a_same_length_replacement_does_not_rewrite_the_map
#[test]
fn test_a_same_length_replacement_does_not_rewrite_the_map() {
    let f = mapping_fixture();
    f.write_mapping(json!({"aaa":"keep-me"}));
    let mut script = vec![listing("s1", 3)];
    download(&mut script, "s1", 0, &noise("aaa"));
    download(&mut script, "s1", 1, &noise("someone-elses-logo"));
    let server = Server::new(script);
    let error = f.map(&server).unwrap_err();
    server.finish();
    assert_eq!(f.mapping(), json!({"aaa":"keep-me"}));
    assert!(error.contains("s1-1"));
    assert!(
        f.files
            .directory
            .join("ticker_to_id.candidate.json")
            .is_file()
    );
    f.released();
}
// tests.test_rebuild_dedup_map.MapIsResolvedByImageIdentity.test_a_live_sticker_that_cannot_be_read_refuses_to_map
#[test]
fn test_a_live_sticker_that_cannot_be_read_refuses_to_map() {
    let f = mapping_fixture();
    f.write_mapping(json!({"aaa":"keep-me"}));
    let mut script = vec![listing("s1", 3)];
    download(&mut script, "s1", 0, &noise("aaa"));
    download(&mut script, "s1", 1, b"not an image at all");
    let server = Server::new(script);
    assert!(f.map(&server).is_err());
    server.finish();
    assert_eq!(f.mapping(), json!({"aaa":"keep-me"}));
    f.released();
}
// tests.test_rebuild_dedup_map.MapIsResolvedByImageIdentity.test_a_missing_source_image_refuses_to_map
#[test]
fn test_a_missing_source_image_refuses_to_map() {
    let f = mapping_fixture();
    f.write_mapping(json!({"aaa":"keep-me"}));
    std::fs::remove_file(f.files.images.join("bbb.png")).unwrap();
    let server = Server::new(vec![listing("s1", 3)]);
    assert!(f.map(&server).is_err());
    server.finish();
    assert_eq!(f.mapping(), json!({"aaa":"keep-me"}));
    f.released();
}
// tests.test_rebuild_dedup_map.MapIsResolvedByImageIdentity.test_an_extra_live_sticker_refuses_to_map
#[test]
fn test_an_extra_live_sticker_refuses_to_map() {
    let f = mapping_fixture();
    f.write_mapping(json!({"aaa":"keep-me"}));
    let server = Server::new(mapping_script(&[
        "aaa",
        "bbb",
        "ccc",
        "appended-by-a-concurrent-tool",
    ]));
    assert!(f.map(&server).is_err());
    server.finish();
    assert_eq!(f.mapping(), json!({"aaa":"keep-me"}));
    f.released();
}
// tests.test_rebuild_dedup_map.MapIsResolvedByImageIdentity.test_the_canonical_map_is_written_under_the_shared_lock
#[test]
fn test_the_canonical_map_is_written_under_the_shared_lock() {
    let f = mapping_fixture();
    let hold = rt::locks::Lease::acquire(
        &rt::locks::canonical_map_path(&f.root.0),
        "actual competing canonical map writer",
    )
    .unwrap();
    let server = Server::new(vec![]);
    assert!(f.map(&server).is_err());
    assert!(server.finish().is_empty());
    assert!(!f.files.directory.join("ticker_to_id.json").exists());
    f.released();
    drop(hold);
    drop(
        rt::locks::Lease::acquire(
            &rt::locks::canonical_map_path(&f.root.0),
            "released canonical positive control",
        )
        .unwrap(),
    );
}
// tests.test_rebuild_dedup_map.MapIsResolvedByImageIdentity.test_a_provider_cannot_change_the_map_during_the_snapshot
#[test]
fn test_a_provider_cannot_change_the_map_during_the_snapshot() {
    let f = mapping_fixture();
    let outcomes = Arc::new(Mutex::new(vec![]));
    let seen = outcomes.clone();
    let root = f.root.0.clone();
    let map = f.files.directory.join("ticker_to_id.json");
    let mut script = mapping_script(&["aaa", "bbb", "ccc"]);
    script[0].action = Some(Box::new(move |r| {
        assert_eq!(r.form["name"], "s1");
        let attempt = match rt::locks::Lease::acquire(
            &rt::locks::canonical_map_path(&root),
            "actual provider during snapshot",
        ) {
            Ok(lease) => {
                write(&map, &json!({"prov":"provider-cid"}));
                drop(lease);
                Ok(())
            }
            Err(e) => Err(e.to_string()),
        };
        seen.lock().unwrap().push(attempt);
    }));
    let server = Server::new(script);
    f.map(&server).unwrap();
    server.finish();
    let results = outcomes.lock().unwrap();
    assert_eq!(results.len(), 1);
    assert!(
        results[0]
            .as_ref()
            .unwrap_err()
            .contains("OS lock acquisition failed")
    );
    assert_eq!(f.mapping(), json!({"aaa":"s1-0","bbb":"s1-1","ccc":"s1-2"}));
    f.released();
    drop(
        rt::locks::Lease::acquire(
            &rt::locks::canonical_map_path(&f.root.0),
            "post-snapshot map availability",
        )
        .unwrap(),
    );
}
// tests.test_rebuild_dedup_map.MapIsResolvedByImageIdentity.test_the_pack_family_lock_is_held_across_the_mapping
#[test]
fn test_the_pack_family_lock_is_held_across_the_mapping() {
    let f = mapping_fixture();
    let hold = rt::locks::Lease::acquire(
        &rt::locks::pack_family_path(&f.root.0, &f.files.base),
        "actual concurrent rebuild",
    )
    .unwrap();
    let server = Server::new(vec![]);
    assert!(f.map(&server).is_err());
    assert!(server.finish().is_empty());
    assert!(!f.files.directory.join("ticker_to_id.json").exists());
    drop(hold);
    f.released();
}
// tests.test_rebuild_dedup_map.MapIsResolvedByImageIdentity.test_a_provider_top_up_is_mapped_from_its_own_image
#[test]
fn test_a_provider_top_up_is_mapped_from_its_own_image() {
    let f = mapping_fixture();
    std::fs::write(f.files.images.join("ddd.png"), noise("ddd")).unwrap();
    let mut state = f.saved();
    state["provider_added"] = json!(["ddd"]);
    write(&f.files.directory.join("rebuild_dedup_state.json"), &state);
    let server = Server::new(mapping_script(&["aaa", "bbb", "ccc", "ddd"]));
    f.map(&server).unwrap();
    server.finish();
    assert_eq!(f.mapping()["ddd"], "s1-3");
    assert_eq!(f.mapping()["aaa"], "s1-0");
    f.released();
}
// tests.test_rebuild_dedup_map.MapIsResolvedByImageIdentity.test_a_provider_top_up_without_its_image_still_refuses
#[test]
fn test_a_provider_top_up_without_its_image_still_refuses() {
    let f = mapping_fixture();
    let mut state = f.saved();
    state["provider_added"] = json!(["ddd"]);
    write(&f.files.directory.join("rebuild_dedup_state.json"), &state);
    let server = Server::new(vec![listing("s1", 4)]);
    assert!(f.map(&server).is_err());
    server.finish();
    assert!(!f.files.directory.join("ticker_to_id.json").exists());
    f.released();
}
// tests.test_rebuild_dedup_map.MapIsResolvedByImageIdentity.test_aliases_in_the_current_map_survive_the_rebuild
#[test]
fn test_aliases_in_the_current_map_survive_the_rebuild() {
    let f = mapping_fixture();
    f.write_mapping(json!({"aaa":"old-a","aaabsc":"old-a","bbb":"old-b","ccc":"old-c"}));
    let server = Server::new(mapping_script(&["aaa", "bbb", "ccc"]));
    f.map(&server).unwrap();
    server.finish();
    assert_eq!(
        f.mapping(),
        json!({"aaa":"s1-0","aaabsc":"s1-0","bbb":"s1-1","ccc":"s1-2"})
    );
    f.released();
}
// tests.test_rebuild_dedup_map.MapIsResolvedByImageIdentity.test_an_alias_that_cannot_be_reapplied_stops_the_write
#[test]
fn test_an_alias_that_cannot_be_reapplied_stops_the_write() {
    let f = mapping_fixture();
    let before = json!({"aaa":"old-a","zzz":"old-z","zzzbsc":"old-z"});
    f.write_mapping(before.clone());
    let server = Server::new(mapping_script(&["aaa", "bbb", "ccc"]));
    let error = f.map(&server).unwrap_err();
    server.finish();
    assert!(error.contains("zzzbsc"));
    assert_eq!(f.mapping(), before);
    f.released();
}
