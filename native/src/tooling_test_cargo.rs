//! Finite same-Cargo metadata admission; observations and caller plans remain unauthenticated.
use crate::tooling_test_observation::manifest::{
    Admission, SMALL_LIMIT, read_file, regular, relative,
};
use serde::{Deserialize, de};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
#[path = "tooling_test_cargo_postflight.rs"]
pub(crate) mod postflight;

fn error(message: &str) -> String {
    format!(
        "Cargo observation refused: {message}; inspect the retained plan/build log and correct the selected target; no fallback executed"
    )
}
fn text(value: &str) -> bool {
    !value.is_empty() && !value.chars().any(char::is_control)
}
fn hex(value: &str, size: usize) -> bool {
    value.len() == size
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Plan {
    schema: u32,
    target_revision: String,
    baseline_revision: String,
    package_id: String,
    files: Vec<Value>,
    targets: Vec<Target>,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Target {
    name: String,
    kind: Vec<String>,
    src_path: String,
    mode: String,
    reason: Option<String>,
}
// Value normally accepts duplicate object keys. Reject them before semantic admission at this boundary.
struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: de::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = Unique;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("duplicate-free JSON")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Unique, E> {
                Ok(Unique(Value::Bool(v)))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Unique, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| Unique(Value::Number(n)))
                    .ok_or_else(|| E::custom("nonfinite JSON number"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Unique, E> {
                Ok(Unique(Value::String(v.into())))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Unique, E> {
                Ok(Unique(Value::String(v)))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_none<E: de::Error>(self) -> Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_seq<A: de::SeqAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut values = Vec::new();
                while let Some(Unique(v)) = a.next_element()? {
                    values.push(v);
                }
                Ok(Unique(Value::Array(values)))
            }
            fn visit_map<A: de::MapAccess<'de>>(self, mut a: A) -> Result<Unique, A::Error> {
                let mut object = serde_json::Map::new();
                while let Some((key, Unique(value))) = a.next_entry::<String, Unique>()? {
                    if object.insert(key, value).is_some() {
                        return Err(de::Error::custom("duplicate JSON field"));
                    }
                }
                Ok(Unique(Value::Object(object)))
            }
        }
        d.deserialize_any(Visitor)
    }
}
struct Record {
    raw: String,
    value: Value,
}
struct Prefix {
    artifacts: Vec<Record>,
    hash: String,
    bytes: u64,
}
fn parse_prefix(reader: impl Read) -> Result<Prefix, String> {
    let mut reader = BufReader::with_capacity(4096, reader);
    let mut hash = Sha256::new();
    let mut bytes = 0_u64;
    let mut artifacts = Vec::new();
    loop {
        let mut line = Vec::new();
        // Take bounds one record before allocation, even if a runtime suffix is enormous.
        let count = Read::by_ref(&mut reader)
            .take(SMALL_LIMIT - bytes + 1)
            .read_until(b'\n', &mut line)
            .map_err(|_| error("cannot read bounded Cargo prefix"))?;
        bytes += count as u64;
        if count == 0 || bytes > SMALL_LIMIT || !line.ends_with(b"\n") {
            return Err(error(
                "missing, truncated or oversized build-finished boundary",
            ));
        }
        hash.update(&line);
        let raw = std::str::from_utf8(&line).map_err(|_| error("Cargo prefix must be UTF-8"))?;
        let Unique(value) = serde_json::from_str::<Unique>(raw)
            .map_err(|_| error("non-JSON/duplicate-key Cargo prefix record"))?;
        let reason = value["reason"]
            .as_str()
            .ok_or_else(|| error("Cargo record has no reason"))?;
        match reason {
            "compiler-artifact" => {
                if artifacts.len() >= 4096 {
                    return Err(error("too many Cargo artifacts"));
                }
                artifacts.push(Record {
                    raw: raw.trim_end_matches(['\r', '\n']).into(),
                    value,
                });
            }
            "build-finished" => {
                if value.as_object().is_none_or(|o| o.len() != 2) || value["success"] != true {
                    return Err(error("Cargo compilation did not finish successfully"));
                }
                return Ok(Prefix {
                    artifacts,
                    hash: format!("{:x}", hash.finalize()),
                    bytes,
                });
            }
            "compiler-message" | "build-script-executed" | "build-started" => {}
            _ => {
                return Err(error(
                    "unsupported Cargo prefix record; runtime text is not metadata",
                ));
            }
        }
    }
}
pub(crate) fn noncoverage_reason(
    name: &str,
    kind: &[String],
    source: &str,
    files: &[Value],
) -> Option<&'static str> {
    if kind == ["bin"]
        && matches!(
            (name, source),
            ("numera-emoji", "native/src/main.rs") | ("numera-build", "native/src/build_main.rs")
        )
    {
        return Some("Application binary harness is not mapped migration case evidence");
    }
    // A changed source must be reviewed again, not silently inherit a platform exemption.
    let hash = match (name, source) {
        ("sandbox_lease_windows", "native/tests/sandbox_lease_windows.rs")
            if cfg!(all(
                target_os = "linux",
                target_arch = "x86_64",
                target_env = "gnu",
                target_pointer_width = "64"
            )) =>
        {
            "8d109d5487a650b109e82165f5b41750b9fcccf0b09ab5d34d04e845ea2ff5db"
        }
        ("tooling_test_cohort", "native/tests/tooling_test_cohort.rs")
            if cfg!(all(windows, target_arch = "x86_64", target_env = "msvc")) =>
        {
            "8d2989b99aadb93f7663fe89825441f7d7633b7ac9c16e339e8b8ae3cc0df4ff"
        }
        _ => return None,
    };
    let mut members = files.iter().filter(|row| row["path"] == source);
    let member = members.next()?;
    if kind != ["test"]
        || members.next().is_some()
        || !matches!(member["role"].as_str(), Some("target" | "support"))
        || member["sha256"] != hash
    {
        return None;
    }
    Some("Source-bound platform-inapplicable harness; no executed or passed cases claimed")
}
fn validate_target(target: &Target, files: &[Value]) -> Result<(), String> {
    if !text(&target.name)
        || !text(&target.src_path)
        || target.name.contains(['/', '\\', ':'])
        || target.kind.is_empty()
        || target.kind.len() > 4
        || target
            .kind
            .iter()
            .any(|k| !matches!(k.as_str(), "test" | "bin" | "lib" | "rlib" | "cdylib"))
        || !(target.src_path.starts_with("native/src/")
            || target.src_path.starts_with("native/tests/"))
        || !target.src_path.ends_with(".rs")
        || target.src_path.contains(['\\', ':'])
        || target
            .src_path
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == ".." || p.starts_with('.'))
    {
        return Err(error("invalid exact PLAN target descriptor"));
    }
    match target.mode.as_str() {
        "observe" if target.reason.is_none() => Ok(()),
        "noncoverage"
            if target.reason.as_deref().is_some_and(text)
                && noncoverage_reason(&target.name, &target.kind, &target.src_path, files)
                    .is_some() =>
        {
            Ok(())
        }
        _ => Err(error(
            "only exact application binaries or source-bound inactive-platform harnesses may be noncoverage",
        )),
    }
}
fn select_target<'a>(
    plan: &'a Plan,
    name: &str,
    kind: &[Value],
    source: &str,
) -> Result<&'a Target, String> {
    let matches = plan
        .targets
        .iter()
        .filter(|target| {
            target.name == name
                && target.src_path == source
                && target
                    .kind
                    .iter()
                    .map(String::as_str)
                    .eq(kind.iter().filter_map(Value::as_str))
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(error(
            "actual compiler target has no unique exact PLAN classification",
        ));
    }
    Ok(matches[0])
}
fn unique_artifact<'a>(
    root: &Path,
    records: &'a [Record],
    executable: &Path,
) -> Result<&'a Record, String> {
    let expected = relative(root, executable)?;
    let mut selected = None;
    for record in records
        .iter()
        .filter(|r| r.value["profile"]["test"] == true)
    {
        let name = record.value["executable"]
            .as_str()
            .ok_or_else(|| error("test artifact has no executable path"))?;
        let path = Path::new(name);
        if !path.is_absolute() {
            return Err(error("test artifact executable must be absolute"));
        }
        // Path equality is lexical; canonicalize each actual test candidate before comparing Windows spelling.
        let candidate = relative(root, path)?;
        if !candidate.starts_with("native/target/") {
            return Err(error("test artifact is not a project-native target"));
        }
        regular(path)?;
        if candidate == expected && selected.replace(record).is_some() {
            return Err(error("duplicate compiler-artifact identity"));
        }
    }
    selected.ok_or_else(|| error("no actual test artifact for this executable"))
}

pub(crate) enum Selection {
    Observe(PathBuf),
    NonCoverage { reason: String },
}
pub(crate) fn prepare(
    root: &Path,
    plan: &Path,
    build_log: &Path,
    executable: &Path,
) -> Result<Selection, String> {
    let root = crate::tooling_environment::physical_root(root)?;
    if ![plan, build_log, executable]
        .iter()
        .all(|p| p.is_absolute())
    {
        return Err(error("PLAN/build log/executable paths must be absolute"));
    }
    for (path, extension) in [(plan, ".json"), (build_log, ".log")] {
        let name = relative(&root, path)?;
        if !name.starts_with("logs/tmp/") || !name.ends_with(extension) {
            return Err(error(
                "PLAN/build log must be a confined logs/tmp JSON/log input",
            ));
        }
    }
    relative(&root, executable)?;
    let executable = crate::migration_bundle::resolved(executable)?;
    let (plan_hash, plan_bytes, _) = read_file(plan, SMALL_LIMIT, true)?;
    let Unique(plan_value) = serde_json::from_slice(&plan_bytes)
        .map_err(|_| error("PLAN contains invalid/duplicate JSON"))?;
    let decoded: Plan = serde_json::from_slice(&plan_bytes)
        .map_err(|_| error("PLAN schema has missing/unknown or invalid fields"))?;
    if plan_value["targets"].as_array().is_none_or(|a| {
        a.iter()
            .any(|t| t["mode"] == "observe" && t.get("reason").is_some())
    }) {
        return Err(error(
            "observe target must omit reason; noncoverage requires an explicit reason",
        ));
    }
    if decoded.schema != 1
        || !hex(&decoded.target_revision, 40)
        || !hex(&decoded.baseline_revision, 40)
        || !text(&decoded.package_id)
        || decoded.targets.is_empty()
        || decoded.targets.len() > 4096
    {
        return Err(error(
            "PLAN requires schema1/revisions/package and1..4096 explicit targets",
        ));
    }
    let mut targets = BTreeSet::new();
    for target in &decoded.targets {
        validate_target(target, &decoded.files)?;
        if !targets.insert((&target.name, &target.kind, &target.src_path)) {
            return Err(error("duplicate PLAN target"));
        }
    }
    let input = regular(build_log)?;
    let prefix = parse_prefix(input)?;
    let record = unique_artifact(&root, &prefix.artifacts, &executable)?;
    let value = &record.value;
    let manifest_path = value["manifest_path"]
        .as_str()
        .ok_or_else(|| error("artifact has no manifest path"))?;
    if !Path::new(manifest_path).is_absolute() {
        return Err(error("compiler manifest path must be absolute"));
    }
    if value["package_id"] != decoded.package_id
        || relative(&root, Path::new(manifest_path))? != "native/Cargo.toml"
    {
        return Err(error("artifact belongs to another package/manifest"));
    }
    let target_name = value["target"]["name"]
        .as_str()
        .ok_or_else(|| error("artifact has no target name"))?;
    let kind = value["target"]["kind"]
        .as_array()
        .ok_or_else(|| error("artifact has no target kinds"))?;
    let source = value["target"]["src_path"]
        .as_str()
        .ok_or_else(|| error("artifact has no source path"))?;
    if !Path::new(source).is_absolute() {
        return Err(error("compiler source path must be absolute"));
    }
    let source = relative(&root, Path::new(source))?;
    let target = select_target(&decoded, target_name, kind, &source)?;
    let mut random = [0_u8; 16];
    getrandom::fill(&mut random)
        .map_err(|_| error("cannot allocate per-binary manifest identity"))?;
    let name = random
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let base = crate::tooling_environment::confined_directory(&root, "logs/tmp", true)?;
    let directory = base.join(format!("cargo-observation-{name}"));
    std::fs::create_dir(&directory)
        .map_err(|_| error("cannot create exclusive manifest directory"))?;
    eprintln!(
        "cargo observation inputs retained at {}",
        directory.display()
    );
    let candidate = directory.join("manifest.json");
    let header = json!({"schema":1,"target_revision":decoded.target_revision,"baseline_revision":decoded.baseline_revision,"files":decoded.files});
    let mut output = serde_json::to_string(&header)
        .map_err(|_| error("cannot serialize admitted PLAN fields"))?;
    output.pop();
    output.push_str(",\"compiler_artifact\":");
    output.push_str(&record.raw);
    output.push_str("}\n");
    if output.len() as u64 > SMALL_LIMIT {
        return Err(error("generated manifest exceeds8MiB"));
    }
    crate::atomic::write_bytes(&candidate, output.as_bytes())
        .map_err(|_| error("cannot preserve per-binary manifest"))?;
    Admission::new(&root, &candidate, &executable).map(|_| ())?;
    if read_file(plan, SMALL_LIMIT, false)?.0 != plan_hash {
        return Err(error("PLAN changed during artifact admission"));
    }
    let checked = parse_prefix(regular(build_log)?)?;
    if checked.hash != prefix.hash || checked.bytes != prefix.bytes {
        return Err(error("Cargo metadata prefix changed during admission"));
    }
    let binding = json!({"schema":1,"trust":"untrusted","plan":relative(&root, plan)?,"build_log":relative(&root, build_log)?,"plan_sha256":plan_hash,"cargo_prefix_sha256":prefix.hash,"cargo_prefix_bytes":prefix.bytes,
        "compiler_artifact_sha256":crate::codec_raster::digest_hex(record.raw.as_bytes()),"target":target_name,"mode":target.mode,
        "limits":"PLAN completeness, compiler origin and CI provenance require independent authentication"});
    crate::atomic::write_json(&directory.join("binding.json"), &binding)
        .map_err(|_| error("cannot preserve Cargo prefix binding"))?;
    match target.mode.as_str() {
        "observe" => Ok(Selection::Observe(candidate)),
        "noncoverage" => Ok(Selection::NonCoverage {
            reason: target.reason.clone().unwrap_or_default(),
        }),
        _ => Err(error("unsupported target mode")),
    }
}
pub(crate) fn complete(root: &Path, manifest: &Path, report: &Path) -> Result<(), String> {
    let name = relative(root, manifest)?;
    let directory = manifest
        .parent()
        .ok_or_else(|| error("missing generated manifest directory"))?;
    let id = directory
        .file_name()
        .and_then(|s| s.to_str())
        .and_then(|s| s.strip_prefix("cargo-observation-"));
    if directory.parent() != Some(root.join("logs/tmp").as_path())
        || !id.is_some_and(|s| hex(s, 32))
        || !name.ends_with("/manifest.json")
    {
        return Err(error(
            "completion requires the generated Cargo manifest directory",
        ));
    }
    regular(manifest)?;
    let report_name = relative(root, report)?;
    let parts = report_name.split('/').collect::<Vec<_>>();
    if parts.len() != 4
        || parts[..2] != ["logs", "test-observations"]
        || parts[3] != "observation.json"
    {
        return Err(error("completion report must be a direct test observation"));
    }
    regular(report)?;
    let output = directory.join("completion.json");
    if std::fs::symlink_metadata(&output).is_ok() {
        return Err(error("completion receipt already exists"));
    }
    crate::atomic::write_json(
        &output,
        &json!({"schema":1,"trust":"untrusted","observation":report_name}),
    )
    .map_err(|_| error("cannot preserve invocation completion locator"))
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    const ARTIFACT: &str = r#"{"reason":"compiler-artifact","package_id":"path+file:///public/native#numera-emoji-core@2.0.0","manifest_path":"/public/native/Cargo.toml","target":{"kind":["test"],"crate_types":["bin"],"name":"cases","src_path":"/public/native/tests/cases.rs","edition":"2024","doc":false,"doctest":false,"test":true},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":true},"features":[],"filenames":["/public/native/target/debug/deps/cases"],"executable":"/public/native/target/debug/deps/cases","fresh":false}"#;
    #[test]
    fn actual_cargo_literals_stop_at_build_boundary_not_later_runtime_output() {
        let input = format!(
            "{ARTIFACT}\n{{\"reason\":\"build-finished\",\"success\":true}}\nrunning test\nnot json\n"
        );
        let prefix = parse_prefix(Cursor::new(input.as_bytes())).unwrap();
        assert_eq!(prefix.artifacts.len(), 1);
        assert_eq!(prefix.artifacts[0].value["target"]["name"], "cases");
        assert_eq!(prefix.bytes, input.find("running test").unwrap() as u64);
        let large_suffix =
            Cursor::new(input.as_bytes()).chain(std::io::repeat(b'x').take(SMALL_LIMIT + 1));
        assert_eq!(parse_prefix(large_suffix).unwrap().hash, prefix.hash);
        let duplicate_nested = ARTIFACT.replacen("\"profile\":{", "\"profile\":{\"test\":true,", 1);
        assert!(
            parse_prefix(Cursor::new(format!(
                "{duplicate_nested}\n{{\"reason\":\"build-finished\",\"success\":true}}\n"
            )))
            .is_err()
        );
        for input in [
            "not JSON\n",
            "{\"reason\":\"build-finished\",\"success\":false}\n",
            "{\"reason\":\"build-finished\",\"success\":true}",
            "{\"reason\":\"unknown\"}\n",
            "{\"reason\":\"build-finished\",\"reason\":\"build-finished\",\"success\":true}\n",
        ] {
            assert!(parse_prefix(Cursor::new(input)).is_err(), "{input}");
        }
        let missing = format!("{ARTIFACT}\n");
        assert!(parse_prefix(Cursor::new(missing)).is_err());
        let too_large = vec![b' '; SMALL_LIMIT as usize + 1];
        assert!(parse_prefix(Cursor::new(too_large)).is_err());
    }
    #[test]
    fn explicit_known_binary_noncoverage_never_becomes_an_unknown_target_fallback() {
        let target: Target = serde_json::from_str(r#"{"name":"numera-emoji","kind":["bin"],"src_path":"native/src/main.rs","mode":"noncoverage","reason":"No mapped cases in the application harness"}"#).unwrap();
        assert!(validate_target(&target, &[]).is_ok());
        for (name, path) in [
            ("other", "native/src/main.rs"),
            ("numera-emoji", "native/src/other.rs"),
        ] {
            let mut bad = target.clone();
            bad.name = name.into();
            bad.src_path = path.into();
            assert!(validate_target(&bad, &[]).is_err());
        }
        let mut bad = target.clone();
        bad.reason = None;
        assert!(validate_target(&bad, &[]).is_err());
        let mut observed = target;
        observed.mode = "observe".into();
        observed.reason = None;
        assert!(validate_target(&observed, &[]).is_ok());
        for (name, source, hash, inactive) in [
            (
                "sandbox_lease_windows",
                "native/tests/sandbox_lease_windows.rs",
                "8d109d5487a650b109e82165f5b41750b9fcccf0b09ab5d34d04e845ea2ff5db",
                cfg!(all(
                    target_os = "linux",
                    target_arch = "x86_64",
                    target_env = "gnu",
                    target_pointer_width = "64"
                )),
            ),
            (
                "tooling_test_cohort",
                "native/tests/tooling_test_cohort.rs",
                "8d2989b99aadb93f7663fe89825441f7d7633b7ac9c16e339e8b8ae3cc0df4ff",
                cfg!(all(windows, target_arch = "x86_64", target_env = "msvc")),
            ),
        ] {
            let files = [json!({"role":"target","path":source,"sha256":hash})];
            let platform = Target {
                name: name.into(),
                kind: vec!["test".into()],
                src_path: source.into(),
                mode: "noncoverage".into(),
                reason: Some("Platform-inapplicable source-bound harness, not passed cases".into()),
            };
            assert_eq!(validate_target(&platform, &files).is_ok(), inactive);
            assert!(validate_target(&platform, &[]).is_err());
            let mut drift = files.clone();
            drift[0]["sha256"] = json!("0".repeat(64));
            assert!(validate_target(&platform, &drift).is_err());
            drift[0] = files[0].clone();
            drift[0]["role"] = json!("fixture");
            assert!(validate_target(&platform, &drift).is_err());
            assert!(validate_target(&platform, &[files[0].clone(), files[0].clone()]).is_err());
            for changed in ["another", "sandbox_lease_windows/other"] {
                let mut wrong = platform.clone();
                wrong.name = changed.into();
                assert!(validate_target(&wrong, &files).is_err());
            }
        }
    }
    mod fixture {
        use crate as _native;
        include!("../tests/support/tooling_fixture.rs");
    }
    #[test]
    fn prepare_produces_real_admitted_manifest_or_explicit_known_noncoverage() {
        const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        for known_bin in [false, true] {
            let root = fixture::Root::new("same Cargo prefix café");
            let source = if known_bin {
                "native/src/main.rs"
            } else {
                "native/tests/cases.rs"
            };
            let files = [
                ("source", "tests/test_original.py"),
                ("target", source),
                ("fixture", "tests/fixtures/sample.json"),
                ("config", "native/Cargo.toml"),
                ("config", "native/Cargo.lock"),
                ("config", ".cargo/tests.toml"),
            ];
            for (_, name) in files {
                let path = root.0.join(name);
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, b"abc").unwrap();
            }
            let binary = root.0.join("native/target/debug/deps/cases");
            std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
            std::fs::write(&binary, b"abc").unwrap();
            let mut artifact: Value = serde_json::from_str(ARTIFACT).unwrap();
            artifact["manifest_path"] = json!(root.0.join("native/Cargo.toml"));
            artifact["executable"] = json!(binary);
            artifact["filenames"] = json!([binary]);
            artifact["target"]["src_path"] = json!(root.0.join(source));
            if known_bin {
                artifact["target"]["kind"] = json!(["bin"]);
                artifact["target"]["name"] = json!("numera-emoji");
            }
            let target = if known_bin {
                json!({"name":"numera-emoji","kind":["bin"],"src_path":source,"mode":"noncoverage","reason":"No mapped source cases"})
            } else {
                json!({"name":"cases","kind":["test"],"src_path":source,"mode":"observe"})
            };
            let plan = root.0.join("logs/tmp/plan.json");
            crate::atomic::write_json(&plan,&json!({"schema":1,"target_revision":"a".repeat(40),"baseline_revision":"b".repeat(40),"package_id":artifact["package_id"],"files":files.map(|(role,path)|json!({"role":role,"path":path,"sha256":ABC})),"targets":[target]})).unwrap();
            let log = root.0.join("logs/tmp/cargo.log");
            std::fs::write(&log,format!("{artifact}\n{{\"reason\":\"build-finished\",\"success\":true}}\nruntime stdout is not JSON\n")).unwrap();
            match prepare(&root.0, &plan, &log, &binary).unwrap() {
                Selection::Observe(path) => {
                    assert!(!known_bin);
                    Admission::new(&root.0, &path, &binary).unwrap();
                    let receipt: Value = serde_json::from_slice(
                        &std::fs::read(path.parent().unwrap().join("binding.json")).unwrap(),
                    )
                    .unwrap();
                    assert_eq!(receipt["trust"], "untrusted");
                    assert_eq!(receipt["target"], "cases");
                }
                Selection::NonCoverage { reason } => {
                    assert!(known_bin);
                    assert_eq!(reason, "No mapped source cases");
                }
            }
            std::fs::write(root.0.join("tests/fixtures/sample.json"), b"drift").unwrap();
            assert!(prepare(&root.0, &plan, &log, &binary).is_err());
            root.clean().unwrap();
        }
    }
    #[test]
    fn wrong_executable_duplicate_artifact_and_unselected_target_refuse() {
        let root = fixture::Root::new("Cargo physical path identity");
        let exe = root.0.join("native/target/debug/deps/cases");
        let other = root.0.join("native/target/debug/other/cases");
        for path in [&exe, &other] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"synthetic executable").unwrap();
        }
        let record = |path: &Path| {
            let mut value: Value = serde_json::from_str(ARTIFACT).unwrap();
            value["executable"] = json!(path);
            Record {
                raw: value.to_string(),
                value,
            }
        };
        assert!(unique_artifact(&root.0, &[record(&other)], &exe).is_err());
        assert!(unique_artifact(&root.0, &[record(&exe)], &exe).is_ok());
        assert!(unique_artifact(&root.0, &[record(&exe), record(&exe)], &exe).is_err());
        #[cfg(windows)]
        {
            let alternate = PathBuf::from(exe.to_str().unwrap().to_ascii_uppercase());
            assert!(unique_artifact(&root.0, &[record(&alternate)], &exe).is_ok());
            let verbatim = exe.canonicalize().unwrap();
            assert!(unique_artifact(&root.0, &[record(&verbatim)], &exe).is_ok());
            assert!(
                unique_artifact(&root.0, &[record(&alternate), record(&verbatim)], &exe).is_err()
            );
        }
        #[cfg(unix)]
        {
            let linked = root.0.join("native/target/debug/linked");
            std::os::unix::fs::symlink(&exe, &linked).unwrap();
            assert!(unique_artifact(&root.0, &[record(&linked)], &exe).is_err());
            std::fs::remove_file(linked).unwrap();
        }
        root.clean().unwrap();
        let plan: Plan=serde_json::from_str(r#"{"schema":1,"target_revision":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","baseline_revision":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","package_id":"p","files":[],"targets":[{"name":"cases","kind":["test"],"src_path":"native/tests/cases.rs","mode":"observe"}]}"#).unwrap();
        assert_eq!(
            select_target(&plan, "cases", &[json!("test")], "native/tests/cases.rs")
                .unwrap()
                .mode,
            "observe"
        );
        assert!(select_target(&plan, "other", &[json!("test")], "native/tests/cases.rs").is_err());
        let duplicate = r#"{"schema":1,"schema":1,"target_revision":"a","baseline_revision":"b","package_id":"p","files":[],"targets":[]}"#;
        assert!(serde_json::from_str::<Plan>(duplicate).is_err());
    }
}
