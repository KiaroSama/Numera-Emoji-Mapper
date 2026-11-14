//! Fixed historical generator replay and tamper evidence share one externally acquired Git corpus.
use super::source_oracle_evidence::refusal;
use serde_json::Value;
type SourceCheck = fn(&_native::source_git_inventory::Source, &Value) -> Result<Value, String>;
type DependentCheck = fn(
    &_native::source_git_inventory::Source,
    &_native::source_git_inventory::Source,
    &Value,
) -> Result<Value, String>;

async fn replay_dependent(
    name: &str,
    root: &std::path::Path,
    executable: &std::path::Path,
    source: &_native::source_git_inventory::Source,
    dependency: &_native::source_git_inventory::Source,
    fixture: &Value,
) -> Result<Value, String> {
    use _native::source_oracle_worker as worker;
    match name {
        "state-artifacts" => {
            worker::check_state_artifacts(root, executable, source, dependency, fixture).await
        }
        "steps" => worker::check_steps(root, executable, source, dependency, fixture).await,
        "ingest" => worker::check_ingest(root, executable, source, dependency, fixture).await,
        "roster-pack" => {
            worker::check_roster_pack(root, executable, source, dependency, fixture).await
        }
        "archive" => worker::check_archive(root, executable, source, dependency, fixture).await,
        _ => panic!("unknown dependent oracle"),
    }
}

async fn replay(
    name: &str,
    root: &std::path::Path,
    executable: &std::path::Path,
    source: &_native::source_git_inventory::Source,
    fixture: &Value,
) -> Result<Value, String> {
    use _native::source_oracle_worker as worker;
    match name {
        "intent" => worker::check_intent(root, executable, source, fixture).await,
        "resume" => worker::check_resume(root, executable, source, fixture).await,
        "bot" => worker::check_bot(root, executable, source, fixture).await,
        "roster" => worker::check_roster(root, executable, source, fixture).await,
        "reconcile" => worker::check_reconcile(root, executable, source, fixture).await,
        "view" => worker::check_view(root, executable, source, fixture).await,
        "signature" => worker::check_signature(root, executable, source, fixture).await,
        "database" => worker::check_database(root, executable, source, fixture).await,
        _ => panic!("unknown fixture oracle"),
    }
}

pub fn check(
    root: &std::path::Path,
    runtime: &tokio::runtime::Runtime,
    sources: &[_native::source_git_inventory::Source],
) {
    let corpora: [(&str, &str, &str, usize, SourceCheck); 8] = [
        (
            "intent",
            "tests/oracles/panel_plan.py",
            include_str!("../../../tests/fixtures/backend/intent.json"),
            8,
            _native::source_intent_oracle::check,
        ),
        (
            "resume",
            "tests/oracles/collection_state.py",
            include_str!("../../../tests/fixtures/backend/resume.json"),
            8,
            _native::source_resume_oracle::check,
        ),
        (
            "bot",
            "tests/oracles/emoji_bot.py",
            include_str!("../../../tests/fixtures/backend/bot.json"),
            6,
            _native::source_bot_oracle::check,
        ),
        (
            "roster",
            "tests/oracles/pack_manifest.py",
            include_str!("../../../tests/fixtures/backend/roster.json"),
            4,
            _native::source_roster_oracle::check,
        ),
        (
            "reconcile",
            "tests/oracles/collection_reconcile.py",
            include_str!("../../../tests/fixtures/backend/reconcile.json"),
            8,
            _native::source_reconcile_oracle::check,
        ),
        (
            "view",
            "tests/oracles/panel_view.py",
            include_str!("../../../tests/fixtures/backend/view.json"),
            3,
            _native::source_view_oracle::check,
        ),
        (
            "signature",
            "tests/oracles/migration_bundle.py",
            include_str!("../../../tests/fixtures/backend/signature.json"),
            2,
            _native::source_sqlite_oracle::check_signature,
        ),
        (
            "database",
            "tests/oracles/migration_bundle.py",
            include_str!("../../../tests/fixtures/backend/migration-database.json"),
            2,
            _native::source_sqlite_oracle::check_database,
        ),
    ];
    let executable = std::path::Path::new(env!("CARGO_BIN_EXE_numera-compat"));
    let unavailable = root.join("logs/tmp/not-an-oracle-executable");
    for (name, path, text, count, direct) in corpora {
        let source = sources.iter().find(|source| source.path == path).unwrap();
        let fixture: Value = serde_json::from_str(text).unwrap();
        let observed = runtime
            .block_on(replay(name, root, executable, source, &fixture))
            .unwrap();
        assert_eq!(observed["cases_checked"], count, "{name}");
        assert_eq!(observed["source_runtime"], "CPython3.11.9", "{name}");
        assert_eq!(observed["source_hash"], source.sha256, "{name}");
        assert_eq!(observed["fixture_drift"], false, "{name}");
        if matches!(name, "intent" | "resume" | "view") {
            assert_eq!(observed["blocked_application_modules"], 8, "{name}");
        }
        for mutation in ["consistent-result", "name"] {
            let mut changed = fixture.clone();
            if mutation == "name" {
                changed["cases"][0]["name"] = serde_json::json!("uniquely-renamed");
            } else {
                match name {
                    "intent" => {
                        changed["cases"][0]["per_set"] = serde_json::json!(1);
                        changed["cases"][0]["expected"]["per_set"] = serde_json::json!(1);
                    }
                    "resume" => {
                        changed["cases"][0]["state"]["base"] = serde_json::json!("changed-family");
                        changed["cases"][0]["expected"]["base"] =
                            serde_json::json!("changed-family");
                    }
                    "bot" => {
                        changed["cases"][4]["text"] = serde_json::json!("111111111111111");
                        changed["cases"][4]["expected"] = serde_json::json!(["111111111111111"]);
                    }
                    "roster" => {
                        changed["cases"][0]["st"]["custom_emoji_id"] =
                            serde_json::json!("555555555");
                        changed["cases"][0]["expected"]["custom_emoji_id"] =
                            serde_json::json!("555555555");
                    }
                    "reconcile" => {
                        changed["cases"][4]["comparison"]["s:red"] = serde_json::json!(false);
                        changed["cases"][4]["expected"] = Value::Null;
                    }
                    "view" => {
                        changed["cases"][0]["request"]["items"][0]["keywords"] =
                            serde_json::json!(["changed"]);
                        changed["cases"][0]["expected"]["view"][0]["label"] =
                            serde_json::json!("changed");
                        changed["cases"][0]["expected"]["view"][0]["copyId"] =
                            serde_json::json!("");
                    }
                    "signature" => {
                        changed["cases"][0]["rows"][0][1] = serde_json::json!(i64::MIN + 1);
                        changed["cases"][0]["expected"] = serde_json::json!("unverified-digest");
                    }
                    "database" => {
                        changed["cases"][0]["hashes"]["v:a"] = serde_json::json!((1_u64 << 63) + 1);
                        changed["cases"][0]["expected"]["items"][0][1] =
                            serde_json::json!(i64::MIN + 1);
                    }
                    _ => unreachable!(),
                }
            }
            // No executable exists: a recipe refusal must precede scratch or child startup.
            assert!(
                runtime
                    .block_on(replay(name, root, &unavailable, source, &changed))
                    .unwrap_err()
                    .contains("recipe drift"),
                "parent: {name}/{mutation}"
            );
            assert!(
                direct(source, &changed)
                    .unwrap_err()
                    .contains("recipe drift"),
                "child: {name}/{mutation}"
            );
        }
        let mut changed = fixture.clone();
        match name {
            "intent" => {
                changed["cases"][0]["expected"] =
                    serde_json::json!({"target-derived":"must not pass"})
            }
            "resume" => {
                changed["cases"][0]["expected"]["sets"][0]["title"] =
                    serde_json::json!("target-derived title")
            }
            "bot" => changed["cases"][0]["expected"] = serde_json::json!([]),
            "roster" => {
                changed["cases"][1]["expected"]["previous_custom_emoji_ids"] = serde_json::json!([])
            }
            "reconcile" => changed["cases"][4]["expected"] = serde_json::json!(["source-drift"]),
            "view" => changed["cases"][0]["expected"]["hidden"] = serde_json::json!(0),
            "signature" => changed["cases"][0]["expected"] = serde_json::json!("unverified-digest"),
            "database" => changed["cases"][0]["expected"]["moved"] = serde_json::json!(0),
            _ => unreachable!(),
        }
        refusal(
            root,
            name,
            &changed,
            runtime.block_on(replay(name, root, executable, source, &changed)),
            "drift",
        )
        .clean();
        if matches!(name, "resume" | "reconcile" | "signature") {
            changed = fixture.clone();
            match name {
                "resume" => {
                    changed["cases"][2]["error"] = serde_json::json!("target-derived refusal")
                }
                "reconcile" => {
                    changed["cases"][4]["calls"] = serde_json::json!(["s:blue", "s:red"])
                }
                "signature" => changed["cases"][0]["repr_order"]
                    .as_array_mut()
                    .unwrap()
                    .reverse(),
                _ => unreachable!(),
            }
            refusal(
                root,
                name,
                &changed,
                runtime.block_on(replay(name, root, executable, source, &changed)),
                "drift",
            )
            .clean();
        }
        let bad_source = _native::source_git_inventory::Source {
            baseline_revision: source.baseline_revision.clone(),
            path: source.path.clone(),
            sha256: source.sha256.clone(),
            text: format!(
                "{}
raise RuntimeError('must never execute')",
                source.text
            ),
        };
        assert!(
            runtime
                .block_on(replay(name, root, &unavailable, &bad_source, &fixture))
                .unwrap_err()
                .contains("source identity drift"),
            "source: {name}"
        );
        assert!(
            direct(&bad_source, &fixture)
                .unwrap_err()
                .contains("source identity drift"),
            "child source: {name}"
        );
    }
    let dependent: [(&str, &str, &str, &str, usize, &str, DependentCheck); 5] = [
        (
            "state-artifacts",
            "tests/oracles/state_artifacts.py",
            "tests/oracles/collection_migrate.py",
            include_str!("../../../tests/fixtures/backend/state-artifacts.json"),
            3,
            "dependency_source_hash",
            _native::source_state_artifacts_oracle::check,
        ),
        (
            "steps",
            "tests/oracles/plan_apply.py",
            "tests/oracles/panel_plan.py",
            include_str!("../../../tests/fixtures/backend/steps.json"),
            6,
            "dependency_hash",
            _native::source_steps_oracle::check,
        ),
        (
            "ingest",
            "tests/oracles/ingest.py",
            "tests/_similarity_oracle.py",
            include_str!("../../../tests/fixtures/backend/ingest.json"),
            8,
            "near_source_hash",
            _native::source_ingest_oracle::check,
        ),
        (
            "roster-pack",
            "tests/oracles/pack_manifest.py",
            "tests/oracles/pack_rows.py",
            include_str!("../../../tests/fixtures/backend/roster-pack.json"),
            4,
            "rows_source_hash",
            _native::source_roster_pack_oracle::check,
        ),
        (
            "archive",
            "tests/oracles/pack_archive.py",
            "tests/oracles/pack_rows.py",
            include_str!("../../../tests/fixtures/backend/archive.json"),
            1,
            "rows_source_hash",
            _native::source_archive_oracle::check,
        ),
    ];
    for (name, path, dependency_path, text, count, hash_field, direct) in dependent {
        let source = sources.iter().find(|source| source.path == path).unwrap();
        let dependency = sources
            .iter()
            .find(|source| source.path == dependency_path)
            .unwrap();
        let fixture: Value = serde_json::from_str(text).unwrap();
        let observed = runtime
            .block_on(replay_dependent(
                name, root, executable, source, dependency, &fixture,
            ))
            .unwrap();
        assert_eq!(observed["cases_checked"], count, "{name}");
        assert_eq!(observed["source_runtime"], "CPython3.11.9", "{name}");
        assert_eq!(observed["source_hash"], source.sha256, "{name}");
        assert_eq!(observed[hash_field], dependency.sha256, "{name}");
        assert_eq!(observed["fixture_drift"], false, "{name}");
        if name == "steps" {
            assert_eq!(observed["blocked_application_modules"], 8);
            let accepted = fixture["cases"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|case| case.get("error").is_none())
                .collect::<Vec<_>>();
            let renders = observed["renders"].as_array().unwrap();
            assert_eq!(renders.len(), accepted.len());
            for (case, rendered) in accepted.iter().zip(renders) {
                assert_eq!(rendered["name"], case["name"]);
                let steps = _native::plan_steps::compute(case).unwrap();
                assert_eq!(
                    _native::plan_steps::render(&steps, &case["ids"], 20).unwrap(),
                    rendered["text"]
                );
            }
        }
        for mutation in ["output", "observation"] {
            let mut changed = fixture.clone();
            match (name, mutation) {
                ("state-artifacts", "output") => {
                    changed["cases"][0]["expected"]["moves"][0]["label"] =
                        serde_json::json!("v:new")
                }
                ("state-artifacts", _) => continue,
                ("steps", "output") => {
                    changed["cases"][0]["expected"]["counted"] = serde_json::json!(999)
                }
                ("steps", _) => {
                    changed["cases"][2]["error"] = serde_json::json!("altered source refusal")
                }
                ("ingest", "output") => {
                    changed["cases"][0]["expected"] = serde_json::json!(["unverified", false])
                }
                ("ingest", _) => {
                    changed["cases"][4]["calls"] = serde_json::json!(["s:different", "s:abc"])
                }
                ("roster-pack", "output") => {
                    changed["cases"][0]["expected"]["count"] = serde_json::json!(999)
                }
                ("roster-pack", _) => {
                    changed["cases"][0]["markdown"] = serde_json::json!("changed Markdown")
                }
                ("archive", "output") => changed["cases"][0]["expected"] = serde_json::json!([]),
                ("archive", _) => {
                    changed["cases"][0]["history_markdown"] = serde_json::json!("changed Markdown")
                }
                _ => unreachable!(),
            }
            refusal(
                root,
                name,
                &changed,
                runtime.block_on(replay_dependent(
                    name, root, executable, source, dependency, &changed,
                )),
                "drift",
            )
            .clean();
        }
        if name == "archive" {
            assert_eq!(observed["folders_checked"], 4);
            for field in ["manifest", "folder", "archive-name"] {
                let mut changed = fixture.clone();
                match field {
                    "manifest" => {
                        changed["cases"][0]["manifest_markdown"] =
                            serde_json::json!("changed Markdown")
                    }
                    "folder" => {
                        changed["folders"][0]["folder"] = serde_json::json!("changed folder")
                    }
                    "archive-name" => {
                        changed["archive_name"] = serde_json::json!("changed filename")
                    }
                    _ => unreachable!(),
                }
                refusal(
                    root,
                    name,
                    &changed,
                    runtime.block_on(replay_dependent(
                        name, root, executable, source, dependency, &changed,
                    )),
                    "drift",
                )
                .clean();
            }
        }
        if name == "roster-pack" {
            assert_eq!(observed["index_cases_checked"], 1);
            for field in ["documents", "expected", "markdown"] {
                let mut changed = fixture.clone();
                changed["index_case"][field] = serde_json::json!("changed source observation");
                refusal(
                    root,
                    name,
                    &changed,
                    runtime.block_on(replay_dependent(
                        name, root, executable, source, dependency, &changed,
                    )),
                    "drift",
                )
                .clean();
            }
        }
        let mut changed = fixture.clone();
        changed["cases"][0]["name"] = serde_json::json!("uniquely-renamed");
        assert!(
            runtime
                .block_on(replay_dependent(
                    name,
                    root,
                    &unavailable,
                    source,
                    dependency,
                    &changed,
                ))
                .unwrap_err()
                .contains("recipe drift"),
            "parent recipe: {name}"
        );
        assert!(
            direct(source, dependency, &changed)
                .unwrap_err()
                .contains("recipe drift"),
            "child recipe: {name}"
        );
        for change_dependency in [false, true] {
            let original = if change_dependency {
                dependency
            } else {
                source
            };
            for mutation in ["revision", "path", "claimed-hash", "bytes"] {
                let mut bad = _native::source_git_inventory::Source {
                    baseline_revision: original.baseline_revision.clone(),
                    path: original.path.clone(),
                    sha256: original.sha256.clone(),
                    text: original.text.clone(),
                };
                match mutation {
                    "revision" => bad.baseline_revision = "0".repeat(40),
                    "path" => bad.path = "tests/oracles/unverified.py".into(),
                    "claimed-hash" => bad.sha256 = "0".repeat(64),
                    "bytes" => bad.text.push_str(
                        "
raise RuntimeError('must never execute')",
                    ),
                    _ => unreachable!(),
                }
                let (source, dependency) = if change_dependency {
                    (source, &bad)
                } else {
                    (&bad, dependency)
                };
                assert!(
                    runtime
                        .block_on(replay_dependent(
                            name,
                            root,
                            &unavailable,
                            source,
                            dependency,
                            &fixture,
                        ))
                        .unwrap_err()
                        .contains("source identity drift"),
                    "parent source: {name}/{change_dependency}/{mutation}"
                );
                assert!(
                    direct(source, dependency, &fixture)
                        .unwrap_err()
                        .contains("source identity drift"),
                    "child source: {name}/{change_dependency}/{mutation}"
                );
            }
        }
    }
    let gallery = sources
        .iter()
        .find(|source| source.path == "tests/oracles/pack_gallery.py")
        .unwrap();
    let script = sources
        .iter()
        .find(|source| source.path == "tests/oracles/script_json.py")
        .unwrap();
    let gallery_fixture: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/backend/gallery.json")).unwrap();
    let observed = runtime
        .block_on(_native::source_oracle_worker::check_gallery(
            root,
            executable,
            gallery,
            script,
            &gallery_fixture,
        ))
        .unwrap();
    assert_eq!(observed["cases_checked"], 3);
    assert_eq!(observed["source_runtime"], "CPython3.11.9");
    assert_eq!(observed["source_hash"], gallery.sha256);
    assert_eq!(observed["dependency_hash"], script.sha256);
    assert_eq!(observed["fixture_drift"], false);
    for real in [false, true] {
        let mut changed = gallery_fixture.clone();
        if real {
            changed["real_static"]["expected"] = serde_json::json!("changed HTML");
        } else {
            changed["cases"][0]["expected"] = serde_json::json!("changed HTML");
        }
        refusal(
            root,
            "gallery",
            &changed,
            runtime.block_on(_native::source_oracle_worker::check_gallery(
                root, executable, gallery, script, &changed,
            )),
            "drift",
        )
        .clean_gallery(&gallery_fixture, real);
    }
    let mut changed = gallery_fixture.clone();
    changed["cases"][0]["document"]["title"] = serde_json::json!("changed");
    assert!(
        runtime
            .block_on(_native::source_oracle_worker::check_gallery(
                root,
                &unavailable,
                gallery,
                script,
                &changed,
            ))
            .unwrap_err()
            .contains("recipe drift")
    );
    assert!(
        _native::source_gallery_oracle::check(root, gallery, script, &changed)
            .unwrap_err()
            .contains("recipe drift")
    );
    for dependency in [false, true] {
        let original = if dependency { script } else { gallery };
        let bad = _native::source_git_inventory::Source {
            baseline_revision: original.baseline_revision.clone(),
            path: original.path.clone(),
            sha256: original.sha256.clone(),
            text: format!(
                "{}\nraise RuntimeError('must never execute')",
                original.text
            ),
        };
        let (gallery, script) = if dependency {
            (gallery, &bad)
        } else {
            (&bad, script)
        };
        assert!(
            runtime
                .block_on(_native::source_oracle_worker::check_gallery(
                    root,
                    &unavailable,
                    gallery,
                    script,
                    &gallery_fixture,
                ))
                .unwrap_err()
                .contains("source identity drift")
        );
        assert!(
            _native::source_gallery_oracle::check(root, gallery, script, &gallery_fixture)
                .unwrap_err()
                .contains("source identity drift")
        );
    }
    let similarity_source = sources
        .iter()
        .find(|source| source.path == "tests/_similarity_oracle.py")
        .unwrap();
    let similarity: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/similarity.json")).unwrap();
    let observed = runtime
        .block_on(_native::source_oracle_worker::check_similarity(
            root,
            executable,
            similarity_source,
            &similarity,
        ))
        .unwrap();
    assert_eq!(observed["cases_checked"], 30);
    assert_eq!(observed["source_runtime"], "CPython3.11.9");
    assert_eq!(observed["source_hash"], similarity_source.sha256);
    assert_eq!(observed["fixture_drift"], false);
    for mutation in ["input-result", "output", "order", "extra"] {
        let mut changed = similarity.clone();
        match mutation {
            "input-result" => {
                changed["greedy"][1]["hashes"] = serde_json::json!([1]);
                changed["greedy"][1]["expected"] = serde_json::json!([0]);
            }
            "output" => changed["near"][0]["expected"] = serde_json::json!([0]),
            "order" => changed["greedy"].as_array_mut().unwrap().swap(0, 1),
            "extra" => changed["unused"] = serde_json::json!(true),
            _ => unreachable!(),
        }
        refusal(
            root,
            "similarity",
            &changed,
            runtime.block_on(_native::source_oracle_worker::check_similarity(
                root,
                executable,
                similarity_source,
                &changed,
            )),
            "fixture drift",
        )
        .clean();
    }
    let bad_source = _native::source_git_inventory::Source {
        baseline_revision: similarity_source.baseline_revision.clone(),
        path: similarity_source.path.clone(),
        sha256: similarity_source.sha256.clone(),
        text: format!(
            "{}\nraise RuntimeError('must never execute')",
            similarity_source.text
        ),
    };
    assert!(
        runtime
            .block_on(_native::source_oracle_worker::check_similarity(
                root,
                &unavailable,
                &bad_source,
                &similarity,
            ))
            .unwrap_err()
            .contains("source identity drift")
    );
    assert!(
        _native::source_similarity_oracle::check(&bad_source, &similarity)
            .unwrap_err()
            .contains("source identity drift")
    );
}
