use super::*;

mod test_video_collision_ingest {
    use super::*;
    mod native_video_boundaries {
        use super::*;
        // tests.test_video_collision_ingest.NativeVideoBoundaries.test_collision_preserves_both_files_and_distinct_identifiers_after_reopen
        #[test]
        fn test_collision_preserves_both_files_and_distinct_identifiers_after_reopen() {
            let f = Collision::new();
            let t = Scratch::new();
            assert_eq!(key(&t.0, f.get("red")), key(&t.0, f.get("middle")));
            let mut cat = catalog(&t.0);
            let (red, _) = add(&mut cat, &t.0, &f.copy(&t.0, "red"), "fuid-red", -1).unwrap();
            let (middle, new) =
                add(&mut cat, &t.0, &f.copy(&t.0, "middle"), "fuid-middle", -1).unwrap();
            assert!(new);
            assert_ne!(red, middle);
            assert!(t.join("middle.webm").is_file());
            let (alias, new) =
                add(&mut cat, &t.0, &f.copy(&t.0, "remux"), "fuid-remux", -1).unwrap();
            assert!(!new);
            assert_eq!(alias, middle);
            drop(cat);
            let mut cat = catalog(&t.0);
            assert_eq!(cat.all(None).unwrap().len(), 2);
            for (id, key) in [
                ("fuid-red", &red),
                ("fuid-middle", &middle),
                ("fuid-remux", &middle),
            ] {
                assert_eq!(seen(&mut cat, id), json!(key));
            }
            let a = cat.get(&red).unwrap().unwrap();
            let b = cat.get(&middle).unwrap().unwrap();
            assert_eq!(
                same(
                    &t.0,
                    std::path::Path::new(a["file_path"].as_str().unwrap()),
                    std::path::Path::new(b["file_path"].as_str().unwrap())
                ),
                Some(false)
            );
        }
        // tests.test_video_collision_ingest.NativeVideoBoundaries.test_different_unmatched_tail_is_checked_in_both_directions
        #[test]
        fn test_different_unmatched_tail_is_checked_in_both_directions() {
            let f = Collision::new();
            for (a, b) in [("short", "last"), ("last", "short")] {
                assert_eq!(same(&f.scratch.0, f.get(a), f.get(b)), Some(false));
            }
            for (a, b) in [("short", "red"), ("red", "short")] {
                assert_eq!(same(&f.scratch.0, f.get(a), f.get(b)), Some(true));
            }
        }
        // tests.test_video_collision_ingest.NativeVideoBoundaries.test_sample_cache_does_not_alias_relative_paths_in_different_directories
        #[test]
        fn test_sample_cache_does_not_alias_relative_paths_in_different_directories() {
            if !is_child() {
                let t = Scratch::new();
                child(
                    "test_video_collision_ingest::native_video_boundaries::test_sample_cache_does_not_alias_relative_paths_in_different_directories",
                    &t.0,
                    &[],
                );
                return;
            }
            let f = Collision::new();
            let t = Scratch::new();
            let size = std::fs::metadata(f.get("red"))
                .unwrap()
                .len()
                .max(std::fs::metadata(f.get("first")).unwrap().len())
                as usize;
            for (folder, name) in [("one", "red"), ("two", "first")] {
                let mut bytes = std::fs::read(f.get(name)).unwrap();
                bytes.resize(size, 0);
                let path = file(&t.join(&format!("{folder}/clip.webm")), &bytes);
                age(&path, 1700000000);
            }
            let d = decoder(&t.0);
            let runtime = runtime();
            std::env::set_current_dir(t.join("one")).unwrap();
            assert!(
                runtime
                    .block_on(d.frames_rgba_typed(std::path::Path::new("clip.webm"), 30))
                    .is_err()
            );
            let first = runtime
                .block_on(d.frames_rgba_typed(&std::fs::canonicalize("clip.webm").unwrap(), 30))
                .unwrap();
            std::env::set_current_dir(t.join("two")).unwrap();
            assert!(
                runtime
                    .block_on(d.frames_rgba_typed(std::path::Path::new("clip.webm"), 30))
                    .is_err()
            );
            let second = runtime
                .block_on(d.frames_rgba_typed(&std::fs::canonicalize("clip.webm").unwrap(), 30))
                .unwrap();
            let fresh = runtime
                .block_on(
                    decoder(&t.0)
                        .frames_rgba_typed(&std::fs::canonicalize("clip.webm").unwrap(), 30),
                )
                .unwrap();
            assert_eq!(second, fresh);
            assert_ne!(first, second);
            std::env::set_current_dir(project()).unwrap();
        }
        // tests.test_video_collision_ingest.NativeVideoBoundaries.test_sample_cache_revalidates_after_toolchain_change
        #[test]
        fn test_sample_cache_revalidates_after_toolchain_change() {
            let Some(fault) = fault_case(
                "test_video_collision_ingest::native_video_boundaries::test_sample_cache_revalidates_after_toolchain_change",
            ) else {
                return;
            };
            let clip = alpha(&fault.root, "opaque", 255, "libvpx-vp9");
            let d = decoder(&fault.root);
            let runtime = runtime();
            runtime.block_on(d.frames_rgba_typed(&clip, 10)).unwrap();
            fault.set("missing-decoder");
            age(&fault.ffmpeg, 1700000300);
            let error = runtime
                .block_on(d.frames_rgba_typed(&clip, 10))
                .unwrap_err();
            assert_eq!(error.kind, "UndecodableVideo");
            assert!(error.message.contains("no libvpx-vp9 decoder"));
        }
        // tests.test_video_collision_ingest.NativeVideoBoundaries.test_vfr_fixture_has_complete_native_timing_evidence
        #[test]
        fn test_vfr_fixture_has_complete_native_timing_evidence() {
            let f = Collision::new();
            let timeline = runtime()
                .block_on(decoder(&f.scratch.0).timeline_typed(f.get("vfr")))
                .unwrap();
            assert_eq!(timeline.times.len(), 4);
            assert!(((timeline.times[2] - timeline.times[1]) - 0.010).abs() < 0.0005);
            assert!(timeline.duration > *timeline.times.last().unwrap());
            assert!((timeline.duration - 1.0).abs() < 0.0005);
        }
        // tests.test_video_collision_ingest.NativeVideoBoundaries.test_native_first_last_and_variable_rate_frames_cannot_disappear
        #[test]
        fn test_native_first_last_and_variable_rate_frames_cannot_disappear() {
            let f = Collision::new();
            for name in ["first", "middle", "last", "vfr"] {
                assert_eq!(same(&f.scratch.0, f.get("red"), f.get(name)), Some(false));
            }
        }
        // tests.test_video_collision_ingest.NativeVideoBoundaries.test_exact_lookup_recovery_never_binds_the_foreign_clip
        #[test]
        fn test_exact_lookup_recovery_never_binds_the_foreign_clip() {
            let f = Collision::new();
            let t = Scratch::new();
            let mut cat = catalog(&t.0);
            add(&mut cat, &t.0, &f.copy(&t.0, "red"), "fuid-red", -1).unwrap();
            let server = Transport::new(std::collections::BTreeMap::from([(
                "foreign".into(),
                f.get("middle").into(),
            )]));
            assert_eq!(resolve(&mut cat, &t.0, &server, "foreign").unwrap(), None);
            assert_eq!(seen(&mut cat, "fuid-foreign"), Value::Null);
            assert_eq!(bases(&mut cat), json!([]));
        }
        // tests.test_video_collision_ingest.NativeVideoBoundaries.test_recovery_and_resume_bind_the_colliding_item_only
        #[test]
        fn test_recovery_and_resume_bind_the_colliding_item_only() {
            let f = Collision::new();
            let t = Scratch::new();
            let mut cat = catalog(&t.0);
            let (red, _) = add(&mut cat, &t.0, &f.copy(&t.0, "red"), "fuid-red", -1).unwrap();
            let (middle, _) =
                add(&mut cat, &t.0, &f.copy(&t.0, "middle"), "fuid-middle", -1).unwrap();
            let db = rusqlite::Connection::open(t.join("catalog.db")).unwrap();
            db.execute("UPDATE items SET phash=-1 WHERE content_key=?", [&middle])
                .unwrap();
            drop(db);
            let server = Transport::new(std::collections::BTreeMap::from([(
                "live".into(),
                f.get("remux").into(),
            )]));
            let mut state = json!({"name":"fixture","fmt":"video","keys":[]});
            reconcile(&mut cat, &t.0, &server, &mut state).unwrap();
            assert_eq!(state["keys"], json!([middle]));
            drop(cat);
            let mut cat = catalog(&t.0);
            assert_eq!(seen(&mut cat, "fuid-live"), middle);
            assert_eq!(
                cat.dispatch(json!({"operation":"custom_id","base":"audit","content_key":middle}))
                    .unwrap(),
                "live"
            );
            assert_eq!(
                cat.dispatch(json!({"operation":"published","base":"audit","content_key":red}))
                    .unwrap(),
                false
            );
            reconcile(&mut cat, &t.0, &server, &mut state).unwrap();
            assert_eq!(state["keys"], json!([middle]));
        }
        // tests.test_video_collision_ingest.NativeVideoBoundaries.test_upload_confirmation_checks_the_unmatched_terminal_frame
        #[test]
        fn test_upload_confirmation_checks_the_unmatched_terminal_frame() {
            let f = Collision::new();
            let t = Scratch::new();
            let mut cat = catalog(&t.0);
            let (short, _) = add(&mut cat, &t.0, &f.copy(&t.0, "short"), "fuid-short", -1).unwrap();
            let mut item = cat.get(&short).unwrap().unwrap();
            item["emojis"] = json!(["x"]);
            item["keywords"] = json!([]);
            let server = Transport::new(std::collections::BTreeMap::from([(
                "live".into(),
                f.get("last").into(),
            )]));
            let tg = server.telegram();
            let runtime = runtime();
            let codec = Codec::new(&t.0);
            let p = rt::publication::Publisher {
                tg: &tg,
                cat: &mut cat,
                media: &codec,
                data: t.0.clone(),
                runtime: &runtime,
                config: None,
            };
            let error = p
                .mutate(&item, "fixture", Some("Fixture"), 1, false)
                .unwrap_err();
            assert!(matches!(error, rt::telegram::ApiError::Unproven(_)));
            assert!(error.to_string().contains("not proven ours"));
            assert_eq!(seen(&mut cat, "fuid-live"), Value::Null);
            assert_eq!(bases(&mut cat), json!([]));
            assert!(
                server
                    .calls
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|path| path.ends_with("/createNewStickerSet"))
            );
        }
        // tests.test_video_collision_ingest.NativeVideoBoundaries.test_near_merge_verifies_content_and_accepts_a_real_reencode
        #[test]
        fn test_near_merge_verifies_content_and_accepts_a_real_reencode() {
            let f = Collision::new();
            let t = Scratch::new();
            let mut cat = catalog(&t.0);
            let (red, _) = add(&mut cat, &t.0, &f.copy(&t.0, "red"), "fuid-red", 4).unwrap();
            let (last, new) = add(&mut cat, &t.0, &f.copy(&t.0, "last"), "fuid-last", 4).unwrap();
            assert!(new);
            assert_ne!(red, last);
            let (pattern, _) =
                add(&mut cat, &t.0, &f.copy(&t.0, "pattern"), "fuid-pattern", 4).unwrap();
            assert_ne!(key(&t.0, f.get("pattern")), key(&t.0, f.get("encoded")));
            let (encoded, new) =
                add(&mut cat, &t.0, &f.copy(&t.0, "encoded"), "fuid-encoded", 4).unwrap();
            assert!(!new);
            assert_eq!(encoded, pattern);
        }
        // tests.test_video_collision_ingest.NativeVideoBoundaries.test_unreadable_exact_candidate_preserves_input_and_binds_nothing
        #[test]
        fn test_unreadable_exact_candidate_preserves_input_and_binds_nothing() {
            let f = Collision::new();
            let t = Scratch::new();
            let mut cat = catalog(&t.0);
            let (red, _) = add(&mut cat, &t.0, &f.copy(&t.0, "red"), "fuid-red", -1).unwrap();
            let row = cat.get(&red).unwrap().unwrap();
            std::fs::remove_file(row["file_path"].as_str().unwrap()).unwrap();
            let path = f.copy(&t.0, "middle");
            assert!(add(&mut cat, &t.0, &path, "foreign", -1).is_err());
            assert!(path.is_file());
            assert_eq!(seen(&mut cat, "foreign"), Value::Null);
        }
        // tests.test_video_collision_ingest.NativeVideoBoundaries.test_untracked_storage_collision_never_overwrites_the_existing_bytes
        #[test]
        fn test_untracked_storage_collision_never_overwrites_the_existing_bytes() {
            let f = Collision::new();
            let t = Scratch::new();
            let source = f.copy(&t.0, "middle");
            let destination = file(&t.join("occupied.webm"), b"unrelated sentinel");
            let codec = Codec::new(&t.0);
            let stored = runtime()
                .block_on(rt::media_store::store(
                    &codec,
                    &source,
                    &destination,
                    "video",
                    &key(&t.0, &source),
                    &json!({}),
                ))
                .unwrap();
            assert_eq!(std::fs::read(&destination).unwrap(), b"unrelated sentinel");
            assert_ne!(stored, destination);
            assert_eq!(
                std::fs::read(stored).unwrap(),
                std::fs::read(f.get("middle")).unwrap()
            );
        }
        // tests.test_video_collision_ingest.NativeVideoBoundaries.test_all_ingest_paths_preserve_existing_destination_and_incoming_difference
        #[test]
        fn test_all_ingest_paths_preserve_existing_destination_and_incoming_difference() {
            let f = Collision::new();
            let t = Scratch::new();
            for route in ["pack", "ids", "local"] {
                let data = t.join(route);
                std::fs::create_dir(&data).unwrap();
                let sentinel = file(
                    &data.join("tmp/another-writer.download"),
                    b"not this run's scratch file",
                );
                let clips = std::collections::BTreeMap::from([
                    ("red".into(), f.get("red").into()),
                    ("middle".into(), f.get("middle").into()),
                ]);
                let codec = Codec::new(&data);
                if route == "local" {
                    let cfg = config(&data, &[]);
                    let log = rt::logging::RunLog::open(&data, "local-collision", vec![]);
                    let args = vec![
                        f.get("red").to_string_lossy().into_owned(),
                        f.get("middle").to_string_lossy().into_owned(),
                        "--data-dir".into(),
                        data.to_string_lossy().into(),
                    ];
                    assert_eq!(
                        rt::local_cli::run_with_codec(&cfg, &args, &log, |_| Ok(codec.clone()))
                            .unwrap(),
                        0
                    );
                } else {
                    let server = Transport::new(clips);
                    let tg = server.telegram();
                    let mut cat = catalog(&data);
                    let mut collector = rt::collector::Collector {
                        cat: &mut cat,
                        media: &codec,
                        data: data.clone(),
                        threshold: -1,
                    };
                    let runtime = runtime();
                    let log = rt::logging::RunLog::open(&data, "remote-collision", vec![]);
                    let counts = if route == "pack" {
                        collector
                            .fetch_pack(&tg, &runtime, "fixture", 0, "keep")
                            .unwrap()
                    } else {
                        rt::fetch_ids_cli::fetch_ids(
                            &mut collector,
                            &tg,
                            &runtime,
                            &["red".into(), "middle".into()],
                            "keep",
                            None,
                            &log,
                        )
                        .unwrap()
                    };
                    assert_eq!(counts["failed"], 0);
                }
                assert_eq!(
                    std::fs::read(sentinel).unwrap(),
                    b"not this run's scratch file"
                );
                let cat = catalog(&data);
                let rows = cat.all(None).unwrap();
                assert_eq!(rows.len(), 2);
                assert_ne!(rows[0]["file_path"], rows[1]["file_path"]);
                for row in &rows {
                    assert!(std::path::Path::new(row["file_path"].as_str().unwrap()).is_file());
                }
                assert_eq!(
                    same(
                        &data,
                        std::path::Path::new(rows[0]["file_path"].as_str().unwrap()),
                        std::path::Path::new(rows[1]["file_path"].as_str().unwrap())
                    ),
                    Some(false)
                );
            }
        }
    }
    mod timeline_alignment {
        use super::*;
        // tests.test_video_collision_ingest.TimelineAlignment.test_equal_frames_at_different_times_do_not_prove_equivalent_timelines
        #[test]
        fn test_equal_frames_at_different_times_do_not_prove_equivalent_timelines() {
            let raw = [RED.repeat(64 * 64), BLUE.repeat(64 * 64)].concat();
            let a = rt::codec_video_timeline::Timeline {
                rgba: std::sync::Arc::new(raw.clone()),
                times: vec![0.0, 0.3],
                duration: 1.0,
            };
            let b = rt::codec_video_timeline::Timeline {
                rgba: std::sync::Arc::new(raw),
                times: vec![0.0, 0.4],
                duration: 1.0,
            };
            assert_eq!(rt::codec_video_timeline::same_image(&a, &b), Some(false));
        }
    }
}

mod test_video_identity_fidelity {
    use super::*;
    mod alpha_fidelity_is_established_or_refused {
        use super::*;
        // tests.test_video_identity_fidelity.AlphaFidelityIsEstablishedOrRefused.test_a_healthy_decoder_still_separates_them
        #[test]
        fn test_a_healthy_decoder_still_separates_them() {
            let t = Scratch::new();
            let a = alpha(&t.0, "opaque", 255, "libvpx-vp9");
            let b = alpha(&t.0, "translucent", 128, "libvpx-vp9");
            assert_ne!(key(&t.0, &a), key(&t.0, &b));
        }
        // tests.test_video_identity_fidelity.AlphaFidelityIsEstablishedOrRefused.test_a_missing_decoder_refuses_instead_of_dropping_alpha
        #[test]
        fn test_a_missing_decoder_refuses_instead_of_dropping_alpha() {
            let Some(fault) = fault_case(
                "test_video_identity_fidelity::alpha_fidelity_is_established_or_refused::test_a_missing_decoder_refuses_instead_of_dropping_alpha",
            ) else {
                return;
            };
            let clip = alpha(&fault.root, "opaque", 255, "libvpx-vp9");
            let d = decoder(&fault.root);
            fault.set("missing-decoder");
            assert_eq!(
                runtime()
                    .block_on(d.fingerprint_typed(&clip))
                    .unwrap_err()
                    .kind,
                "UndecodableVideo"
            );
        }
        // tests.test_video_identity_fidelity.AlphaFidelityIsEstablishedOrRefused.test_a_missing_decoder_cannot_collapse_two_clips_into_one_key
        #[test]
        fn test_a_missing_decoder_cannot_collapse_two_clips_into_one_key() {
            let Some(fault) = fault_case(
                "test_video_identity_fidelity::alpha_fidelity_is_established_or_refused::test_a_missing_decoder_cannot_collapse_two_clips_into_one_key",
            ) else {
                return;
            };
            let a = alpha(&fault.root, "opaque", 255, "libvpx-vp9");
            let b = alpha(&fault.root, "translucent", 128, "libvpx-vp9");
            let d = decoder(&fault.root);
            fault.set("missing-decoder");
            let runtime = runtime();
            let keys = [a, b]
                .iter()
                .map(|p| match runtime.block_on(d.fingerprint_typed(p)) {
                    Ok(v) => Some(v["content_key"].clone()),
                    Err(e) => {
                        assert_eq!(e.kind, "UndecodableVideo");
                        None
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(keys, vec![None, None]);
        }
        // tests.test_video_identity_fidelity.AlphaFidelityIsEstablishedOrRefused.test_a_probe_failure_is_not_cached_as_a_successful_fallback
        #[test]
        fn test_a_probe_failure_is_not_cached_as_a_successful_fallback() {
            let Some(fault) = fault_case(
                "test_video_identity_fidelity::alpha_fidelity_is_established_or_refused::test_a_probe_failure_is_not_cached_as_a_successful_fallback",
            ) else {
                return;
            };
            let clip = alpha(&fault.root, "opaque", 255, "libvpx-vp9");
            let d = decoder(&fault.root);
            fault.set("flaky-probe");
            let runtime = runtime();
            assert_eq!(
                runtime
                    .block_on(d.decoder_args_typed(&clip))
                    .unwrap_err()
                    .kind,
                "UndecodableVideo"
            );
            assert_eq!(
                runtime.block_on(d.decoder_args_typed(&clip)).unwrap(),
                vec!["-c:v", "libvpx-vp9"]
            );
            assert_eq!(
                fault.count(),
                3,
                "two real probes plus one decoder-capability query must run"
            );
        }
        // tests.test_video_identity_fidelity.AlphaFidelityIsEstablishedOrRefused.test_a_container_naming_no_codec_is_not_read_as_needing_nothing
        #[test]
        fn test_a_container_naming_no_codec_is_not_read_as_needing_nothing() {
            let Some(fault) = fault_case(
                "test_video_identity_fidelity::alpha_fidelity_is_established_or_refused::test_a_container_naming_no_codec_is_not_read_as_needing_nothing",
            ) else {
                return;
            };
            let clip = alpha(&fault.root, "opaque", 255, "libvpx-vp9");
            let d = decoder(&fault.root);
            fault.set("no-codec");
            let error = runtime().block_on(d.decoder_args_typed(&clip)).unwrap_err();
            assert_eq!(error.kind, "UndecodableVideo");
            assert!(error.message.contains("no video codec"));
        }
        // tests.test_video_identity_fidelity.AlphaFidelityIsEstablishedOrRefused.test_an_unreadable_capability_check_is_not_an_answer_of_no
        #[test]
        fn test_an_unreadable_capability_check_is_not_an_answer_of_no() {
            let Some(fault) = fault_case(
                "test_video_identity_fidelity::alpha_fidelity_is_established_or_refused::test_an_unreadable_capability_check_is_not_an_answer_of_no",
            ) else {
                return;
            };
            let clip = alpha(&fault.root, "opaque", 255, "libvpx-vp9");
            let d = decoder(&fault.root);
            fault.set("capability-error");
            let runtime = runtime();
            let error = runtime.block_on(d.decoder_args_typed(&clip)).unwrap_err();
            assert_eq!(error.kind, "UndecodableVideo");
            assert!(error.message.contains("could not ask ffmpeg"));
            fault.set("healthy");
            assert_eq!(
                runtime.block_on(d.decoder_args_typed(&clip)).unwrap(),
                vec!["-c:v", "libvpx-vp9"]
            );
            assert_eq!(
                fault.count(),
                2,
                "a failed capability query must not cache a no answer"
            );
        }
        // tests.test_video_identity_fidelity.AlphaFidelityIsEstablishedOrRefused.test_an_undecodable_video_never_reaches_the_catalog
        #[test]
        fn test_an_undecodable_video_never_reaches_the_catalog() {
            let Some(fault) = fault_case(
                "test_video_identity_fidelity::alpha_fidelity_is_established_or_refused::test_an_undecodable_video_never_reaches_the_catalog",
            ) else {
                return;
            };
            let a = alpha(&fault.root, "opaque", 255, "libvpx-vp9");
            let b = alpha(&fault.root, "translucent", 128, "libvpx-vp9");
            let d = decoder(&fault.root);
            fault.set("missing-decoder");
            let db = fault.root.join("must-not-open.db");
            let outcome = runtime().block_on(d.fingerprint_typed(&a));
            assert_eq!(outcome.unwrap_err().kind, "UndecodableVideo");
            assert!(!db.exists());
            assert!(a.is_file() && b.is_file());
        }
        // tests.test_video_identity_fidelity.AlphaFidelityIsEstablishedOrRefused.test_the_three_public_apis_agree_after_a_recovery
        #[test]
        fn test_the_three_public_apis_agree_after_a_recovery() {
            let Some(fault) = fault_case(
                "test_video_identity_fidelity::alpha_fidelity_is_established_or_refused::test_the_three_public_apis_agree_after_a_recovery",
            ) else {
                return;
            };
            let clip = alpha(&fault.root, "opaque", 255, "libvpx-vp9");
            let d = decoder(&fault.root);
            fault.set("missing-decoder");
            let runtime = runtime();
            assert_eq!(
                runtime
                    .block_on(d.fingerprint_typed(&clip))
                    .unwrap_err()
                    .kind,
                "UndecodableVideo"
            );
            fault.reset_tool();
            let info = runtime.block_on(d.fingerprint_typed(&clip)).unwrap();
            assert_eq!(
                info["content_key"],
                runtime.block_on(d.content_key(&clip)).unwrap()
            );
            assert_eq!(
                info["phash"],
                json!(runtime.block_on(d.perceptual_hash(&clip)).unwrap())
            );
        }
        // tests.test_video_identity_fidelity.AlphaFidelityIsEstablishedOrRefused.test_concurrent_callers_get_one_consistent_answer
        #[test]
        fn test_concurrent_callers_get_one_consistent_answer() {
            let t = Scratch::new();
            let clip = alpha(&t.0, "opaque", 255, "libvpx-vp9");
            let d = std::sync::Arc::new(decoder(&t.0));
            let (send, receive) = std::sync::mpsc::channel();
            let mut threads = Vec::new();
            for _ in 0..8 {
                let d = d.clone();
                let clip = clip.clone();
                let send = send.clone();
                threads.push(std::thread::spawn(move || {
                    send.send(runtime().block_on(d.decoder_args_typed(&clip)).unwrap())
                        .unwrap();
                }));
            }
            drop(send);
            for _ in 0..8 {
                assert_eq!(
                    receive
                        .recv_timeout(std::time::Duration::from_secs(60))
                        .unwrap(),
                    vec!["-c:v", "libvpx-vp9"]
                );
            }
            for thread in threads {
                thread.join().unwrap();
            }
        }
        // tests.test_video_identity_fidelity.AlphaFidelityIsEstablishedOrRefused.test_a_still_image_is_not_subjected_to_the_video_rule
        #[test]
        fn test_a_still_image_is_not_subjected_to_the_video_rule() {
            let t = Scratch::new();
            let still = png(&t.join("still.png"), [10, 200, 90, 255], (80, 60));
            assert!(
                runtime()
                    .block_on(decoder(&t.0).decoder_args_typed(&still))
                    .unwrap()
                    .is_empty()
            );
        }
    }
    mod vp8_is_held_to_the_same_rule {
        use super::*;
        // tests.test_video_identity_fidelity.Vp8IsHeldToTheSameRule.test_vp8_alpha_separates_two_clips
        #[test]
        fn test_vp8_alpha_separates_two_clips() {
            let t = Scratch::new();
            let a = alpha(&t.0, "vp8-opaque", 255, "libvpx");
            let b = alpha(&t.0, "vp8-translucent", 120, "libvpx");
            assert_ne!(key(&t.0, &a), key(&t.0, &b));
        }
        // tests.test_video_identity_fidelity.Vp8IsHeldToTheSameRule.test_vp8_without_its_decoder_refuses
        #[test]
        fn test_vp8_without_its_decoder_refuses() {
            let Some(fault) = fault_case(
                "test_video_identity_fidelity::vp8_is_held_to_the_same_rule::test_vp8_without_its_decoder_refuses",
            ) else {
                return;
            };
            let clip = alpha(&fault.root, "opaque", 255, "libvpx");
            let d = decoder(&fault.root);
            fault.set("missing-decoder");
            assert_eq!(
                runtime()
                    .block_on(d.fingerprint_typed(&clip))
                    .unwrap_err()
                    .kind,
                "UndecodableVideo"
            );
        }
    }
}
