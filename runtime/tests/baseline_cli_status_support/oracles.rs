use crate::support::*;
use numera_runtime as native;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::Path;
#[path = "../support/gallery_oracle.rs"]
mod gallery_oracle;
const REFERENCE: &str = "a39a049c02f8dcc90d18fe8771869f82dd655221";
fn historical_sources() -> Result<&'static std::collections::BTreeMap<String, Vec<u8>>, String> {
    static SOURCES: std::sync::OnceLock<
        Result<std::collections::BTreeMap<String, Vec<u8>>, String>,
    > = std::sync::OnceLock::new();
    SOURCES.get_or_init(||{
        let git=native::codec_process::executable("git")?;
        let mut sources=std::collections::BTreeMap::new();
        for line in pinned().lines(){let(_,name)=line.split_once("  ").ok_or("invalid oracle receipt")?;if !name.starts_with("tests/oracles/"){continue;}
            let args=vec!["-C".into(),project().display().to_string(),"cat-file".into(),"blob".into(),format!("{REFERENCE}:{name}")];
            let capture=runtime().block_on(native::codec_process::capture_with_budget(&project(),&git,&args,4*1024*1024,None,&std::env::vars_os().collect(),native::codec_process::Budget{wall:std::time::Duration::from_secs(5),idle:std::time::Duration::from_secs(5),warning:None}))
                .map_err(|e|format!("Frozen oracle provenance requires a full Git checkout with reference {REFERENCE}: {e}"))?;
            sources.insert(name.to_owned(),capture.stdout);
        }Ok(sources)
    }).as_ref().map_err(Clone::clone)
}
pub fn digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn verify_pins(root: &Path, manifest: &str) -> Result<(), String> {
    for line in manifest.lines() {
        let (expected, name) = line
            .split_once("  ")
            .ok_or("invalid oracle digest record")?;
        let bytes = if name.starts_with("tests/oracles/") {
            historical_sources()?
                .get(name)
                .cloned()
                .ok_or_else(|| format!("Uninventoried oracle source: {name}"))?
        } else {
            std::fs::read(root.join(name)).map_err(|e| e.to_string())?
        };
        if digest(&bytes) != expected {
            return Err(format!("Pinned oracle source drift: {name}"));
        }
    }
    Ok(())
}
pub fn pinned() -> &'static str {
    include_str!("oracle.sha256")
}
pub fn corpus(name: &str) -> Value {
    verify_pins(&project(), pinned()).unwrap();
    read_json(&project().join(format!("tests/fixtures/backend/{name}.json")))
}
pub fn validate(value: &Value, source: &str, count: usize) -> Result<(), String> {
    if value["source"] != source {
        return Err("Fixture source does not match its pinned seam".into());
    }
    let cases = value["cases"].as_array().ok_or("cases must be a list")?;
    if cases.is_empty() || cases.len() != count {
        return Err(format!("Expected {count} non-empty oracle cases"));
    }
    let mut names = BTreeSet::new();
    for case in cases {
        let name = case["name"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("missing oracle name")?;
        if !names.insert(name) {
            return Err("duplicate oracle name".into());
        }
        if case.get("expected").is_some() == case.get("error").is_some() {
            return Err("needs exactly one expected result or error".into());
        }
        if case.get("error").is_some() && case["error"].as_str().is_none_or(str::is_empty) {
            return Err("meaningless oracle error".into());
        }
    }
    Ok(())
}
fn assert_result(case: &Value, result: Result<Value, String>) {
    if let Some(error) = case.get("error") {
        assert_eq!(
            result.unwrap_err(),
            error.as_str().unwrap(),
            "{}",
            case["name"]
        );
    } else {
        assert_eq!(result.unwrap(), case["expected"], "{}", case["name"]);
    }
}
fn request(case: &Value) -> Value {
    let mut out = case.clone();
    for key in ["name", "expected", "error"] {
        out.as_object_mut().unwrap().remove(key);
    }
    out
}
fn intent(document: &Value) {
    for case in document["cases"].as_array().unwrap() {
        let mut req = request(case);
        req["written_utc"] = document["utc"].clone();
        assert_result(case, native::intent::execute(req));
    }
}
fn resume(document: &Value) {
    for case in document["cases"].as_array().unwrap() {
        let result = native::resume::collection(case["state"].clone(), "fixture");
        if case.get("error").is_some() {
            assert!(result.is_err(), "{}", case["name"]);
        } else {
            assert_eq!(result.unwrap(), case["expected"], "{}", case["name"]);
        }
    }
}
fn view(document: &Value) {
    validate(document, "d173116:panel_view.build_view", 3).unwrap();
    for case in document["cases"].as_array().unwrap() {
        assert_result(case, native::panel_view::build(case["request"].clone()));
    }
}
fn steps(document: &Value) {
    validate(document, "d173116:plan_apply.compute_steps", 6).unwrap();
    for case in document["cases"].as_array().unwrap() {
        assert_result(case, native::plan_steps::compute(&request(case)));
    }
}
// tests.test_native_state.NativeIntent.test_intent_matches_pinned_source
#[test]
fn intent_matches_pinned_source() {
    let d = corpus("intent");
    assert!(!d["cases"].as_array().unwrap().is_empty());
    intent(&d);
}
// tests.test_native_state.NativeIntent.test_fixture_drift
#[test]
fn intent_and_resume_fixture_drift() {
    let i = corpus("intent");
    let r = corpus("resume");
    intent(&i);
    resume(&r);
}
// tests.test_native_state.NativeIntent.test_collection_resume_matches_source
#[test]
fn collection_resume_matches_source() {
    let d = corpus("resume");
    assert!(!d["cases"].as_array().unwrap().is_empty());
    resume(&d);
}
// tests.test_native_state.NativeIntent.test_invalid_targets_are_refused
#[test]
fn invalid_target_variants_are_refused() {
    for value in [
        json!(true),
        json!(false),
        json!(0),
        json!(-1),
        json!(1.5),
        json!("1"),
        Value::Null,
    ] {
        assert!(native::intent::execute(json!({"operation":"targets","previous":{"targets":[["a",value]],"moves":[],"held":[]}})).is_err(),"{value}");
    }
}
// tests.test_native_panel.NativePanelView.test_visibility_membership_and_labels_match_source
#[test]
fn panel_visibility_membership_labels_match_source() {
    view(&corpus("view"));
}
// tests.test_native_panel.NativePanelView.test_fixture_drift
#[test]
fn panel_fixture_drift() {
    let d = corpus("view");
    view(&d);
}
// tests.test_native_panel.NativePanelView.test_generators_need_no_native_or_live_application_module
#[test]
fn frozen_oracles_need_no_interpreter_or_live_application() {
    let names = BTreeSet::from(["intent.json", "resume.json", "steps.json", "view.json"]);
    assert_eq!(names.len(), 4);
    for name in names {
        let d = corpus(name.trim_end_matches(".json"));
        match name {
            "intent.json" => intent(&d),
            "resume.json" => resume(&d),
            "steps.json" => steps(&d),
            "view.json" => view(&d),
            _ => unreachable!(),
        }
    }
    let cargo = std::fs::read_to_string(project().join("runtime/Cargo.toml")).unwrap();
    assert!(!cargo.contains("pyo3"));
    assert!(!cargo.contains("python"));
}
// tests.test_native_plan_steps.NativePlanSteps.test_moves_holding_reorder_and_capacity_match_source
#[test]
fn moves_holding_reorder_capacity_match_source() {
    steps(&corpus("steps"));
}
// tests.test_native_plan_steps.NativePlanSteps.test_fixture_drift
#[test]
fn steps_fixture_drift() {
    steps(&corpus("steps"));
}
// tests.test_native_plan_steps.NativePlanSteps.test_pinned_source_drift_fails_before_execution
#[test]
fn source_drift_refuses_before_execution() {
    verify_pins(&project(), pinned()).unwrap();
    let line = pinned()
        .lines()
        .find(|l| l.ends_with("tests/oracles/plan_apply.py"))
        .unwrap();
    let altered = pinned().replace(
        line,
        &format!("{}  tests/oracles/plan_apply.py", "0".repeat(64)),
    );
    let mut executions = 0;
    let result = verify_pins(&project(), &altered).map(|()| {
        executions += 1;
        steps(&corpus("steps"));
    });
    assert!(result.unwrap_err().contains("Pinned oracle source drift"));
    assert_eq!(executions, 0);
    let copy = Scratch::new();
    let name = "tests/fixtures/backend/steps.json";
    let pin = pinned().lines().find(|line| line.ends_with(name)).unwrap();
    write(
        &copy.0.join(name),
        std::fs::read(project().join(name)).unwrap(),
    );
    verify_pins(&copy.0, pin).unwrap();
    let mut mutated = read_json(&copy.0.join(name));
    let pending = mutated["cases"][0]["expected"]["pending"]
        .as_bool()
        .unwrap();
    mutated["cases"][0]["expected"]["pending"] = json!(!pending);
    document(&copy.0.join(name), &mutated);
    let result = verify_pins(&copy.0, pin).map(|()| {
        executions += 1;
        steps(&mutated);
    });
    assert!(result.unwrap_err().contains("Pinned oracle source drift"));
    assert_eq!(
        executions, 0,
        "valid JSON with a mutated expected outcome must fail before business execution"
    );
}
// tests.test_native_plan_steps.NativePlanSteps.test_empty_duplicated_or_unannotated_cases_cannot_pass
#[test]
fn malformed_oracle_variants_cannot_pass() {
    let d = corpus("steps");
    for mutation in ["empty", "duplicate", "unannotated", "both", "wrong-source"] {
        let mut broken = d.clone();
        match mutation {
            "empty" => broken["cases"] = json!([]),
            "duplicate" => broken["cases"][1]["name"] = broken["cases"][0]["name"].clone(),
            "unannotated" => {
                broken["cases"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("expected");
            }
            "both" => broken["cases"][0]["error"] = json!("not a source result"),
            _ => broken["source"] = json!("unverified"),
        };
        assert!(
            validate(&broken, "d173116:plan_apply.compute_steps", 6).is_err(),
            "{mutation}"
        );
    }
}
// tests.test_native_bot.NativeBotPayloads.test_render_parse_extract_and_copy_batches_match_source
#[test]
fn bot_render_parse_extract_copy_batches_match_source() {
    let d = corpus("bot");
    validate(&d, "d173116:emoji_bot.pure_payloads", 6).unwrap();
    for case in d["cases"].as_array().unwrap() {
        assert_eq!(
            native::bot_payloads::execute(case).unwrap(),
            case["expected"],
            "{}",
            case["name"]
        );
    }
}
// tests.test_native_reconcile.NativeReconcileParity.test_near_candidates_and_unknown_results_match_source
#[test]
fn reconcile_near_candidates_and_unknown_match_source() {
    let d = corpus("reconcile");
    validate(&d, "d173116:collection_reconcile._near_catalog_match", 8).unwrap();
    for case in d["cases"].as_array().unwrap() {
        let mut calls = Vec::new();
        let result = runtime()
            .block_on(native::reconcile::near_catalog(
                case["items"].as_array().unwrap(),
                case["fmt"].as_str().unwrap(),
                case["probe"].as_u64(),
                case["lookup"].as_str().unwrap(),
                async |item: &Value| {
                    let key = item["content_key"].as_str().unwrap();
                    calls.push(key.to_owned());
                    Ok(case["comparison"][key].as_bool())
                },
            ))
            .unwrap();
        let actual = match result {
            native::reconcile::NearMatch::Missing => Value::Null,
            native::reconcile::NearMatch::Unique(k) => json!(k),
            native::reconcile::NearMatch::Ambiguous => json!("ambiguous"),
            native::reconcile::NearMatch::Undecidable => json!("undecidable"),
        };
        assert_eq!(actual, case["expected"], "{}", case["name"]);
        assert_eq!(json!(calls), case["calls"]);
    }
}
// tests.test_native_ingest.NativeIngestParity.test_nominations_unknown_ambiguity_and_collision_match_source
#[test]
fn ingest_nominations_unknown_ambiguity_collision_match_source() {
    let d = corpus("ingest");
    validate(&d, "d173116:ingest.catalog_identity", 8).unwrap();
    let root = Scratch::new();
    for (index, case) in d["cases"].as_array().unwrap().iter().enumerate() {
        let mut cat = native::catalog::Catalog::open(
            &root.0.join(format!("{index}/catalog.db")),
            "fixture UTC",
            &root.0,
        )
        .unwrap();
        let mut names = std::collections::BTreeMap::new();
        for (n, item) in case["items"].as_array().unwrap().iter().enumerate() {
            let mut item = item.clone();
            let path = root.0.join(format!("rival-{n}"));
            names.insert(
                path.clone(),
                item["content_key"].as_str().unwrap().to_owned(),
            );
            item["file_path"] = json!(path);
            cat.insert(&item, "fixture UTC").unwrap();
        }
        let mut calls = Vec::new();
        let result = runtime().block_on(native::ingest::identify(
            &cat,
            &case["incoming"],
            case["threshold"].as_i64().unwrap() as i32,
            async |a: &Path, _b: &Path, _fmt: &str| {
                let key = &names[a];
                calls.push(key.clone());
                Ok(case["comparison"][key].as_bool())
            },
            async |_path: &Path, _key: &str| Ok(case["collision"].as_str().unwrap().to_owned()),
        ));
        assert_result(case, result.map(|v| json!(v)));
        assert_eq!(json!(calls), case["calls"]);
    }
}
// tests.test_native_roster.NativeRosterRows.test_ids_format_and_history_match_live_source_contract
#[test]
fn roster_ids_format_history_match_source() {
    let d = corpus("roster");
    validate(&d, "d173116:pack_manifest._row", 4).unwrap();
    for case in d["cases"].as_array().unwrap() {
        assert_eq!(
            native::roster::row(case).unwrap(),
            case["expected"],
            "{}",
            case["name"]
        );
    }
}
// tests.test_native_roster.NativeRosterRows.test_gallery_and_whole_roster_fixtures_are_source_generated
#[test]
fn whole_roster_and_gallery_frozen_source_provenance() {
    for (name, source) in [
        (
            "gallery",
            "d173116:pack_gallery.render+script_json.json_for_script",
        ),
        ("roster-pack", "d173116:pack_manifest.build_pack"),
        ("archive", "d173116:pack_archive.names+rows+reports"),
        ("state-artifacts", "d173116:state_artifacts.rewrite"),
        (
            "migration-database",
            "d173116:migration_bundle.rewrite_database",
        ),
        ("signature", "d173116:migration_bundle.database_signature"),
    ] {
        let d = corpus(name);
        assert_eq!(d["source"], source, "{name}");
        assert!(!d["cases"].as_array().unwrap().is_empty(), "{name}");
    }
    let d = corpus("roster-pack");
    for case in d["cases"].as_array().unwrap() {
        let out = native::roster::pack(
            &case["record"],
            &case["pack"],
            case["family"].as_str().unwrap(),
            &case["provenance"],
            &case["prior"],
            "2026-10-07 00:00:00 UTC",
        )
        .unwrap();
        assert_eq!(out, case["expected"]);
        assert_eq!(
            native::roster_markdown::pack(&out).unwrap(),
            case["markdown"].as_str().unwrap()
        );
    }
    let d = corpus("gallery");
    for case in d["cases"].as_array().unwrap() {
        let thumbnails = serde_json::from_value(case["thumbnails"].clone()).unwrap();
        let out = native::pack_gallery::render_page(&case["document"], &thumbnails).unwrap();
        let expected = case["expected"].as_str().unwrap().replace("\r\n", "\n");
        assert_eq!(
            out.replace("\r\n", "\n"),
            gallery_oracle::with_reviewed_motion_gate(&expected)
        );
    }
    for case in corpus("archive")["cases"].as_array().unwrap() {
        let rows = native::archive_rows::rows(
            &case["record"],
            case["live"].as_array().unwrap(),
            &case["items"],
            &case["keys"],
        )
        .unwrap();
        assert_eq!(json!(rows), case["expected"]);
        assert_eq!(
            native::archive_rows::history(&case["record"], &rows).unwrap(),
            case["history_markdown"].as_str().unwrap()
        );
        assert_eq!(
            native::archive_rows::manifest(&case["record"], &rows, &case["items"]).unwrap(),
            case["manifest_markdown"].as_str().unwrap()
        );
    }
    for case in corpus("state-artifacts")["cases"].as_array().unwrap() {
        let map = serde_json::from_value(case["mapping"].clone()).unwrap();
        assert_eq!(
            native::state_artifacts::rewrite(
                case["name"].as_str().unwrap(),
                &case["document"],
                &map
            )
            .unwrap(),
            case["expected"]
        );
    }
    for case in corpus("migration-database")["cases"].as_array().unwrap() {
        let mut db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE items(content_key TEXT PRIMARY KEY,phash INTEGER,label TEXT);CREATE TABLE publications(content_key TEXT,custom_emoji_id TEXT);CREATE TABLE seen_files(content_key TEXT,file_unique_id TEXT);INSERT INTO items VALUES('v:a',123,'alpha'),('v:b',456,'beta');INSERT INTO publications VALUES('v:a','111111111'),('v:b','222222222');INSERT INTO seen_files VALUES('v:a','unique-a'),('v:b','unique-b');").unwrap();
        let moved = native::migration_database::rewrite(
            &mut db,
            &serde_json::from_value(case["map"].clone()).unwrap(),
            &serde_json::from_value(case["hashes"].clone()).unwrap(),
        )
        .unwrap();
        let items = db
            .prepare("SELECT * FROM items ORDER BY content_key")
            .unwrap()
            .query_map([], |r| {
                Ok(json!([
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<i64>>(1)?,
                    r.get::<_, String>(2)?
                ]))
            })
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        let rows = |table: &str| {
            db.prepare(&format!("SELECT * FROM {table} ORDER BY content_key"))
                .unwrap()
                .query_map([], |r| {
                    Ok(json!([r.get::<_, String>(0)?, r.get::<_, String>(1)?]))
                })
                .unwrap()
                .map(Result::unwrap)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            json!({"moved":moved,"items":items,"publications":rows("publications"),"seen_files":rows("seen_files")}),
            case["expected"]
        );
    }
    for case in corpus("signature")["cases"].as_array().unwrap() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE \"z weird\"(text TEXT,n INTEGER,nullable TEXT,b BLOB);CREATE TABLE items(content_key TEXT,file_path TEXT,phash INTEGER);CREATE TABLE aa(note TEXT);INSERT INTO aa VALUES(\"first table\");").unwrap();
        let mut repr = Vec::new();
        for row in case["rows"].as_array().unwrap() {
            let values = row
                .as_array()
                .unwrap()
                .iter()
                .map(|v| match v {
                    Value::Null => rusqlite::types::Value::Null,
                    Value::Number(n) => rusqlite::types::Value::Integer(n.as_i64().unwrap()),
                    Value::String(s) => rusqlite::types::Value::Text(s.clone()),
                    Value::Object(o) => {
                        let hex = o["blob"].as_str().unwrap();
                        rusqlite::types::Value::Blob(
                            (0..hex.len())
                                .step_by(2)
                                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                                .collect(),
                        )
                    }
                    _ => panic!("invalid signature scalar"),
                })
                .collect::<Vec<_>>();
            repr.push(native::python_repr::row(&values).unwrap());
            db.execute(
                "INSERT INTO \"z weird\" VALUES(?,?,?,?)",
                rusqlite::params_from_iter(values),
            )
            .unwrap();
        }
        db.execute("INSERT INTO items VALUES('s:fixture','./after.png',-1)", [])
            .unwrap();
        repr.sort();
        assert_eq!(json!(repr), case["repr_order"]);
        assert_eq!(
            native::migration_signature::database(&db, case["files"].as_array().unwrap()).unwrap(),
            case["expected"]
        );
    }
}
