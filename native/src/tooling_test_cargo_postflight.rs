//! Reconcile one Cargo invocation's artifacts and local observations; never authenticate CI.
use super::{
    Plan, Record, Target, Unique, hex, parse_prefix, select_target, text, validate_target,
};
use crate::tooling_test_observation::manifest::{
    Admission, SMALL_LIMIT, read_file, regular, relative,
};
use crate::tooling_test_observation::{listing, results};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
fn fail(message: &str) -> String {
    format!(
        "Cargo postflight refused: {message}; retain local inputs and inspect guarded-run evidence; no verified coverage claimed"
    )
}
fn json_file(path: &Path) -> Result<(String, Value), String> {
    let (hash, bytes, _) = read_file(path, SMALL_LIMIT, true)?;
    let Unique(value) = serde_json::from_slice(&bytes)
        .map_err(|_| fail("invalid or duplicate JSON evidence fields"))?;
    Ok((hash, value))
}
fn keys(value: &Value, required: &[&str], optional: &[&str]) -> Result<(), String> {
    let object = value
        .as_object()
        .ok_or_else(|| fail("evidence must be an object"))?;
    if required.iter().any(|k| !object.contains_key(*k))
        || object
            .keys()
            .any(|k| !required.contains(&k.as_str()) && !optional.contains(&k.as_str()))
    {
        return Err(fail("missing/unknown receipt fields"));
    }
    Ok(())
}
fn named(root: &Path, value: &Value, key: &str) -> Result<PathBuf, String> {
    let name = value[key]
        .as_str()
        .filter(|s| text(s))
        .ok_or_else(|| fail("evidence relative path is missing"))?;
    if name.contains(['\\', ':'])
        || name
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return Err(fail("evidence path is not confined relative"));
    }
    let path = root.join(name);
    if relative(root, &path)? != name {
        return Err(fail("evidence path identity differs"));
    }
    Ok(path)
}
fn binary_layout(name: &str) -> bool {
    let parts = name.split('/').collect::<Vec<_>>();
    let host = if cfg!(windows) {
        "x86_64-pc-windows-msvc"
    } else {
        "x86_64-unknown-linux-gnu"
    };
    matches!(parts.as_slice(),["native","target","debug","deps",file] if !file.is_empty())
        || matches!(parts.as_slice(),["native","target",triple,"debug","deps",file] if *triple==host && !file.is_empty())
}
fn locators(root: &Path, stderr: &str) -> Result<Vec<PathBuf>, String> {
    if stderr.len() as u64 > SMALL_LIMIT {
        return Err(fail("stderr locator stream exceeds8MiB"));
    }
    let base = crate::tooling_environment::confined_directory(root, "logs/tmp", false)?;
    let mut paths = BTreeSet::new();
    for line in stderr.lines() {
        let Some(name) = line.strip_prefix("cargo observation inputs retained at ") else {
            continue;
        };
        let path = Path::new(name);
        if !path.is_absolute() || !text(name) {
            return Err(fail(
                "candidate locator is not an absolute control-free path",
            ));
        }
        let physical = crate::tooling_environment::physical_root(path)?;
        let suffix = physical
            .file_name()
            .and_then(|s| s.to_str())
            .and_then(|s| s.strip_prefix("cargo-observation-"));
        if physical.parent() != Some(base.as_path())
            || !suffix.is_some_and(|s| hex(s, 32))
            || !paths.insert(physical)
            || paths.len() > 4096
        {
            return Err(fail("duplicate or unconfined candidate locator"));
        }
    }
    Ok(paths.into_iter().collect())
}
type Identity = (String, Vec<String>, String);
fn identity(target: &Target) -> Identity {
    (
        target.name.clone(),
        target.kind.clone(),
        target.src_path.clone(),
    )
}
fn artifacts<'a>(
    root: &Path,
    plan: &'a Plan,
    records: &'a [Record],
) -> Result<BTreeMap<Identity, (&'a Target, &'a Record, PathBuf)>, String> {
    let mut observed = BTreeMap::new();
    for record in records
        .iter()
        .filter(|r| r.value["profile"]["test"] == true)
    {
        let value = &record.value;
        let path = |field: &Value| -> Result<PathBuf, String> {
            let path = Path::new(
                field
                    .as_str()
                    .ok_or_else(|| fail("artifact path missing"))?,
            );
            if !path.is_absolute() {
                return Err(fail("artifact paths must be absolute"));
            }
            relative(root, path)?;
            crate::migration_bundle::resolved(path)
        };
        let manifest = path(&value["manifest_path"])?;
        if value["package_id"] != plan.package_id
            || relative(root, &manifest)? != "native/Cargo.toml"
        {
            return Err(fail("test artifact package/manifest differs from PLAN"));
        }
        let name = value["target"]["name"]
            .as_str()
            .ok_or_else(|| fail("artifact target name missing"))?;
        let kind = value["target"]["kind"]
            .as_array()
            .ok_or_else(|| fail("artifact target kind missing"))?;
        let source = relative(root, &path(&value["target"]["src_path"])?)?;
        let target = select_target(plan, name, kind, &source)?;
        let binary = path(&value["executable"])?;
        if !binary_layout(&relative(root, &binary)?) {
            return Err(fail(
                "test executable is outside native debug/deps or the admitted host debug/deps",
            ));
        }
        regular(&binary)?;
        if observed
            .insert(identity(target), (target, record, binary))
            .is_some()
        {
            return Err(fail("duplicate actual test artifact"));
        }
    }
    if observed.len() != plan.targets.len()
        || plan
            .targets
            .iter()
            .any(|t| !observed.contains_key(&identity(t)))
    {
        return Err(fail(
            "actual test artifact set is missing or extra relative to PLAN",
        ));
    }
    Ok(observed)
}
fn report(
    root: &Path,
    completion: &Value,
    manifest: &Value,
    manifest_hash: &str,
    admission: &Admission,
    successful: bool,
) -> Result<(), String> {
    keys(completion, &["schema", "trust", "observation"], &[])?;
    if completion["schema"] != 1 || completion["trust"] != "untrusted" {
        return Err(fail("invalid completion receipt"));
    }
    let path = named(root, completion, "observation")?;
    let name = relative(root, &path)?;
    let parts = name.split('/').collect::<Vec<_>>();
    if !matches!(parts.as_slice(),["logs","test-observations",id,"observation.json"] if !id.is_empty())
    {
        return Err(fail(
            "completion observation is not in an exclusive observation child",
        ));
    }
    let parent = path
        .parent()
        .ok_or_else(|| fail("observation has no parent"))?;
    let (_, value) = json_file(&path)?;
    if value["schema"] != 1
        || value["result"] != "observed"
        || value["trust"] != "untrusted"
        || value["release_ready"] != false
        || !value["completed"].is_boolean()
        || !value["exit_code"].is_i64()
    {
        return Err(fail(
            "report is not explicitly untrusted nonrelease observation",
        ));
    }
    if successful && (value["completed"] != true || value["exit_code"] != 0) {
        return Err(fail(
            "successful Cargo requires completed zero-exit observations",
        ));
    }
    for key in ["target_revision", "baseline_revision", "files"] {
        if value[key] != manifest[key] {
            return Err(fail("report input closure/revision differs from manifest"));
        }
    }
    let expected = admission.report();
    for key in ["manifest_sha256", "executable_sha256", "executable"] {
        if value[key] != expected[key] {
            return Err(fail("report manifest/executable byte identity drift"));
        }
    }
    if value["manifest_sha256"] != manifest_hash
        || json_file(&parent.join("manifest.json"))?.0 != manifest_hash
    {
        return Err(fail("raw manifest copy differs from candidate"));
    }
    let read = |file: &str| -> Result<(String, String), String> {
        let (hash, bytes, _) = read_file(&parent.join(file), SMALL_LIMIT, true)?;
        Ok((
            hash,
            String::from_utf8(bytes).map_err(|_| fail("observation sidecar is not UTF-8"))?,
        ))
    };
    let (list_hash, list) = read("listing.log")?;
    let (result_hash, result) = read("results.log")?;
    let (disposition_hash, disposition) = read("dispositions.log")?;
    if value["listing_sha256"] != list_hash
        || value["results_sha256"] != result_hash
        || value["dispositions_sha256"] != disposition_hash
    {
        return Err(fail("observation sidecar hash drift"));
    }
    let names = listing(&list)?;
    let rows = results(&result, &names)?;
    let public = results(&disposition, &names)?;
    crate::tooling_test_observation::validate_args(
        &serde_json::from_value::<Vec<String>>(value["argv"].clone())
            .map_err(|_| fail("report argv must be libtest string arguments"))?,
    )?;
    if rows != public
        || disposition
            != rows
                .iter()
                .map(|(name, status)| format!("{status} {name}\n"))
                .collect::<String>()
    {
        return Err(fail(
            "sanitized dispositions differ from actual result records",
        ));
    }
    if successful && (!rows.values().any(|s| s == "ok") || rows.values().any(|s| s == "failed")) {
        return Err(fail(
            "successful observation has zero passed cases or failed cases",
        ));
    }
    if value["listed"] != json!(names)
        || value["cases"]
            != json!(
                rows.iter()
                    .map(|(name, status)| json!({"name":name,"status":status}))
                    .collect::<Vec<_>>()
            )
    {
        return Err(fail(
            "report cases/listed identities do not match actual sidecars",
        ));
    }
    if value["closure_sha256"]
        != crate::codec_raster::digest_hex(
            &serde_json::to_vec(&manifest["files"])
                .map_err(|_| fail("cannot serialize closure"))?,
        )
    {
        return Err(fail("report closure digest differs"));
    }
    let members = std::fs::read_dir(parent)
        .map_err(|_| fail("cannot enumerate observation store"))?
        .map(|e| {
            e.map_err(|_| fail("cannot enumerate observation member"))?
                .file_name()
                .into_string()
                .map_err(|_| fail("non-UTF8 observation member"))
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    if members
        != [
            "manifest.json",
            "listing.log",
            "results.log",
            "dispositions.log",
            "observation.json",
        ]
        .map(String::from)
        .into_iter()
        .collect()
    {
        return Err(fail(
            "observation store is not the closed five-member layout",
        ));
    }
    Ok(())
}
/// Call only after the existing outer process owner has verified descendant quiescence.
pub(crate) fn check(
    root: &Path,
    plan: &Path,
    build_log: &Path,
    stderr: &str,
    exit_code: i32,
) -> Result<(), String> {
    let root = crate::tooling_environment::physical_root(root)?;
    if !plan.is_absolute() || !build_log.is_absolute() {
        return Err(fail("PLAN and build log paths must be absolute"));
    }
    let plan_path = relative(&root, plan)?;
    let build_path = relative(&root, build_log)?;
    if !plan_path.starts_with("logs/tmp/")
        || !plan_path.ends_with(".json")
        || !build_path.starts_with("logs/tmp/")
        || !build_path.ends_with(".log")
    {
        return Err(fail(
            "PLAN/build log must be confined logs/tmp JSON/log inputs",
        ));
    }
    let (plan_hash, value) = json_file(plan)?;
    let plan: Plan =
        serde_json::from_value(value.clone()).map_err(|_| fail("invalid PLAN schema"))?;
    if plan.schema != 1
        || !hex(&plan.target_revision, 40)
        || !hex(&plan.baseline_revision, 40)
        || !text(&plan.package_id)
        || plan.targets.is_empty()
        || plan.targets.len() > 4096
    {
        return Err(fail("invalid PLAN revisions or target count"));
    }
    if value["targets"].as_array().is_none_or(|a| {
        a.iter()
            .any(|t| t["mode"] == "observe" && t.get("reason").is_some())
    }) {
        return Err(fail("observe PLAN targets must omit noncoverage reasons"));
    }
    let mut expected = BTreeSet::new();
    for target in &plan.targets {
        validate_target(target, &plan.files)?;
        if !expected.insert(identity(target)) {
            return Err(fail("duplicate PLAN target"));
        }
    }
    let prefix = parse_prefix(regular(build_log)?)?;
    let actual = artifacts(&root, &plan, &prefix.artifacts)?;
    let mut seen = BTreeSet::new();
    let mut observations = 0;
    for candidate in locators(&root, stderr)? {
        let binding_path = candidate.join("binding.json");
        let (binding_hash, binding) = json_file(&binding_path)?;
        keys(
            &binding,
            &[
                "schema",
                "trust",
                "plan",
                "build_log",
                "plan_sha256",
                "cargo_prefix_sha256",
                "cargo_prefix_bytes",
                "compiler_artifact_sha256",
                "target",
                "mode",
                "limits",
            ],
            &[],
        )?;
        if binding["schema"] != 1
            || binding["trust"] != "untrusted"
            || binding["plan"] != plan_path
            || binding["build_log"] != build_path
            || binding["plan_sha256"] != plan_hash
            || binding["cargo_prefix_sha256"] != prefix.hash
            || binding["cargo_prefix_bytes"] != prefix.bytes
        {
            return Err(fail("candidate binding does not match this invocation"));
        }
        let (manifest_hash, manifest) = json_file(&candidate.join("manifest.json"))?;
        let artifact = &manifest["compiler_artifact"];
        let source = Path::new(
            artifact["target"]["src_path"]
                .as_str()
                .ok_or_else(|| fail("candidate source missing"))?,
        );
        if !source.is_absolute() {
            return Err(fail("candidate source must be absolute"));
        }
        let target = select_target(
            &plan,
            artifact["target"]["name"]
                .as_str()
                .ok_or_else(|| fail("candidate target missing"))?,
            artifact["target"]["kind"]
                .as_array()
                .ok_or_else(|| fail("candidate kinds missing"))?,
            &relative(&root, source)?,
        )?;
        let id = identity(target);
        let (_, record, binary) = actual
            .get(&id)
            .ok_or_else(|| fail("candidate has no actual compiled target"))?;
        if !seen.insert(id)
            || artifact != &record.value
            || binding["target"] != target.name
            || binding["mode"] != target.mode
            || binding["compiler_artifact_sha256"]
                != crate::codec_raster::digest_hex(record.raw.as_bytes())
        {
            return Err(fail("duplicate or mismatched candidate artifact identity"));
        }
        for key in ["target_revision", "baseline_revision", "files"] {
            if manifest[key] != value[key] {
                return Err(fail("candidate manifest differs from PLAN closure"));
            }
        }
        let admission = Admission::new(&root, &candidate.join("manifest.json"), binary)?;
        let completion = candidate.join("completion.json");
        if target.mode == "observe" {
            if exit_code == 0 || completion.exists() {
                report(
                    &root,
                    &json_file(&completion)?.1,
                    &manifest,
                    &manifest_hash,
                    &admission,
                    exit_code == 0,
                )?;
                observations += 1;
            }
        } else if completion.exists() {
            return Err(fail(
                "noncoverage target must not claim an observation completion",
            ));
        }
        if json_file(&binding_path)?.0 != binding_hash
            || json_file(&candidate.join("manifest.json"))?.0 != manifest_hash
        {
            return Err(fail("candidate evidence changed during postflight"));
        }
    }
    if exit_code == 0
        && (observations == 0
            || seen != expected
            || observations != plan.targets.iter().filter(|t| t.mode == "observe").count())
    {
        return Err(fail(
            "successful Cargo lacks all actual guarded target completions; possible runner bypass",
        ));
    }
    if json_file(&root.join(plan_path))?.0 != plan_hash
        || parse_prefix(regular(build_log)?)?.hash != prefix.hash
    {
        return Err(fail("PLAN or build prefix changed during postflight"));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    mod fixture {
        use crate as _native;
        include!("../tests/support/tooling_fixture.rs");
    }
    const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    fn seed() -> (fixture::Root, PathBuf, PathBuf, String, PathBuf) {
        let root = fixture::Root::new("Cargo postflight");
        let files = [
            ("source", "tests/original.py"),
            ("target", "native/tests/cases.rs"),
            ("fixture", "tests/fixtures/golden.json"),
            ("config", "native/Cargo.toml"),
            ("config", "native/Cargo.lock"),
            ("config", ".cargo/tests.toml"),
        ];
        for (_, name) in files {
            let p = root.0.join(name);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, b"abc").unwrap();
        }
        let binary = root.0.join("native/target/debug/deps/cases");
        std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
        std::fs::write(&binary, b"abc").unwrap();
        let artifact = json!({"reason":"compiler-artifact","package_id":"opaque","manifest_path":root.0.join("native/Cargo.toml"),"target":{"name":"cases","kind":["test"],"crate_types":["bin"],"src_path":root.0.join("native/tests/cases.rs"),"edition":"2024","doc":false,"doctest":false,"test":true},"profile":{"test":true,"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true},"executable":binary,"filenames":[binary],"features":[],"fresh":false});
        let plan = root.0.join("logs/tmp/plan.json");
        let members = files.map(|(role, path)| json!({"role":role,"path":path,"sha256":ABC}));
        crate::atomic::write_json(&plan,&json!({"schema":1,"target_revision":"a".repeat(40),"baseline_revision":"b".repeat(40),"package_id":"opaque","files":members,"targets":[{"name":"cases","kind":["test"],"src_path":"native/tests/cases.rs","mode":"observe"}]})).unwrap();
        let log = root.0.join("logs/tmp/cargo.log");
        std::fs::write(
            &log,
            format!(
                "{artifact}\n{{\"reason\":\"build-finished\",\"success\":true}}\nruntime suffix\n"
            ),
        )
        .unwrap();
        let candidate = root
            .0
            .join(format!("logs/tmp/cargo-observation-{}", "1".repeat(32)));
        std::fs::create_dir(&candidate).unwrap();
        let manifest = candidate.join("manifest.json");
        crate::atomic::write_json(&manifest,&json!({"schema":1,"target_revision":"a".repeat(40),"baseline_revision":"b".repeat(40),"files":members,"compiler_artifact":artifact})).unwrap();
        let prefix = parse_prefix(regular(&log).unwrap()).unwrap();
        crate::atomic::write_json(&candidate.join("binding.json"),&json!({"schema":1,"trust":"untrusted","plan":relative(&root.0,&plan).unwrap(),"build_log":relative(&root.0,&log).unwrap(),"plan_sha256":read_file(&plan,SMALL_LIMIT,false).unwrap().0,"cargo_prefix_sha256":prefix.hash,"cargo_prefix_bytes":prefix.bytes,"compiler_artifact_sha256":crate::codec_raster::digest_hex(artifact.to_string().as_bytes()),"target":"cases","mode":"observe","limits":"not authenticated"})).unwrap();
        let store = root.0.join("logs/test-observations/observation");
        std::fs::create_dir_all(&store).unwrap();
        let mut observation = crate::tooling_test_observation::Observation::begin(
            &root.0,
            &store,
            &manifest,
            &binary,
            &[],
        )
        .unwrap();
        std::fs::write(store.join("listing.log"), b"test actual_case\n").unwrap();
        observation.run_args(0).unwrap();
        std::fs::write(store.join("results.log"), b"ok actual_case\n").unwrap();
        let report = observation.finish(0).unwrap();
        crate::atomic::write_json(&candidate.join("completion.json"),&json!({"schema":1,"trust":"untrusted","observation":relative(&root.0,&report).unwrap()})).unwrap();
        let stderr = format!(
            "cargo observation inputs retained at {}\n",
            candidate.display()
        );
        (root, plan, log, stderr, report)
    }
    #[test]
    fn actual_small_file_closure_refuses_runner_bypass_missing_and_duplicate_receipts() {
        let (root, plan, log, stderr, report) = seed();
        check(&root.0, &plan, &log, &stderr, 0).unwrap();
        assert!(check(&root.0, &plan, &log, "", 0).is_err());
        assert!(check(&root.0, &plan, &log, &format!("{stderr}{stderr}"), 0).is_err());
        std::fs::write(
            report.parent().unwrap().join("results.log"),
            b"ignored actual_case\n",
        )
        .unwrap();
        assert!(check(&root.0, &plan, &log, &stderr, 0).is_err());
        root.clean().unwrap();
    }
    #[test]
    fn known_noncoverage_bins_alone_or_unreported_extra_artifacts_cannot_satisfy_success() {
        let (root, plan, log, stderr, _) = seed();
        let raw = std::fs::read_to_string(&log).unwrap();
        let mut artifact: Value = serde_json::from_str(raw.lines().next().unwrap()).unwrap();
        artifact["target"]["name"] = json!("extra");
        std::fs::write(
            &log,
            format!(
                "{}\n{artifact}\n{{\"reason\":\"build-finished\",\"success\":true}}\n",
                raw.lines().next().unwrap()
            ),
        )
        .unwrap();
        assert!(check(&root.0, &plan, &log, &stderr, 0).is_err());
        let source = root.0.join("native/src/main.rs");
        std::fs::create_dir_all(source.parent().unwrap()).unwrap();
        std::fs::write(&source, b"abc").unwrap();
        artifact["target"]["name"] = json!("numera-emoji");
        artifact["target"]["kind"] = json!(["bin"]);
        artifact["target"]["src_path"] = json!(source);
        let mut p: Value = serde_json::from_slice(&std::fs::read(&plan).unwrap()).unwrap();
        p["targets"] = json!([{"name":"numera-emoji","kind":["bin"],"src_path":"native/src/main.rs","mode":"noncoverage","reason":"Not mapped case evidence"}]);
        crate::atomic::write_json(&plan, &p).unwrap();
        std::fs::write(
            &log,
            format!("{artifact}\n{{\"reason\":\"build-finished\",\"success\":true}}\n"),
        )
        .unwrap();
        assert!(check(&root.0, &plan, &log, "", 0).is_err());
        root.clean().unwrap();
    }
    #[test]
    fn marker_and_host_paths_are_locators_not_authority() {
        let root = fixture::Root::new("postflight locator");
        let host = if cfg!(windows) {
            "x86_64-pc-windows-msvc"
        } else {
            "x86_64-unknown-linux-gnu"
        };
        for path in [
            "native/target/debug/deps/test".to_owned(),
            format!("native/target/{host}/debug/deps/test"),
        ] {
            assert!(binary_layout(&path));
        }
        for path in [
            "native/target/release/deps/test",
            "native/target/aarch64-unknown-linux-gnu/debug/deps/test",
            "private/test",
            "native/target/debug/deps/x/other",
        ] {
            assert!(!binary_layout(path));
        }
        assert!(locators(&root.0, "cargo observation inputs retained at /outside\n").is_err());
        assert!(locators(&root.0, "cargo observation inputs retained at relative\n").is_err());
        assert!(
            locators(&root.0, "unrelated diagnostic\n")
                .unwrap()
                .is_empty()
        );
        root.clean().unwrap();
    }
    #[test]
    fn prefix_target_closure_and_revision_or_sidecar_drift_refuse() {
        let (root, plan, log, stderr, report) = seed();
        let original = std::fs::read(&log).unwrap();
        let text = String::from_utf8(original.clone()).unwrap();
        std::fs::write(
            &log,
            text.replacen("\"name\":\"cases\"", "\"name\":\"other\"", 1),
        )
        .unwrap();
        assert!(check(&root.0, &plan, &log, &stderr, 0).is_err());
        std::fs::write(&log, original).unwrap();
        let mut value: Value = serde_json::from_slice(&std::fs::read(&report).unwrap()).unwrap();
        value["target_revision"] = json!("c".repeat(40));
        crate::atomic::write_json(&report, &value).unwrap();
        assert!(check(&root.0, &plan, &log, &stderr, 0).is_err());
        assert!(check(&root.0, &plan, &log, "", 101).is_ok());
        root.clean().unwrap();
    }
}
