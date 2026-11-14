//! Actual historical unittest collection, distinct from execution and static assertion evidence.
use crate::source_git_inventory::Source;
use pyo3::prelude::*;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const GUARD_HASH: &str = "e7daf7810ca29292f5d2e798566241821f7b9b6f55644c9da1b12034ce3f404c";

fn admit(root: &Path, stage: &Path, guard: &Source) -> Result<(), String> {
    if guard.baseline_revision != BASELINE
        || guard.path != "tests/__init__.py"
        || guard.sha256 != GUARD_HASH
        || crate::codec_raster::digest_hex(guard.text.as_bytes()) != GUARD_HASH
    {
        return Err("runtime collection guard identity drift; refusing execution".into());
    }
    crate::source_import_replay::validate(root, stage)
}

pub async fn observe_owned(
    root: &Path,
    executable: &Path,
    stage: &Path,
    guard: &Source,
) -> Result<Value, String> {
    admit(root, stage, guard)?;
    let report = crate::source_oracle_process::check(
        root,
        executable,
        "runtime-discovery",
        guard,
        &[],
        &json!({"stage":stage}),
    )
    .await?;
    crate::source_import_replay::validate(root, stage)?;
    Ok(report)
}

/// Reconcile collection observations, not passed tests. Receipt provenance remains caller-owned.
pub fn reconcile(inventory: &Value, coverage: &Value, observed: &Value) -> Result<Value, String> {
    let revision = inventory["baseline_revision"]
        .as_str()
        .ok_or("missing source revision")?;
    if revision.len() != 40
        || !revision
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        || coverage["baseline_revision"] != revision
        || observed["source_revision"] != revision
        || observed["source_runtime"] != "CPython3.11.9"
        || observed["collection_only"] != true
        || observed["tests_run"].as_u64() != Some(0)
        || observed["guard_imported_first"] != true
        || observed["loader_errors"].as_u64() != Some(0)
        || observed["case_fields"]
            != json!([
                "id",
                "defined_in_file",
                "defined_in_hash",
                "code_line",
                "class_skipped",
                "code_qualname",
                "method_skipped"
            ])
    {
        return Err("runtime collection identity/schema differs from admitted sources".into());
    }
    let files = inventory["files"]
        .as_array()
        .filter(|files| !files.is_empty())
        .ok_or("runtime reconciliation requires source files")?;
    let mut hashes = BTreeMap::new();
    for file in files {
        let path = file["source"].as_str().ok_or("missing source path")?;
        crate::source_test_inventory::source_path(path)?;
        let hash = file["sha256"].as_str().ok_or("missing source hash")?;
        if hash.len() != 64
            || !hash
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
            || hashes.insert(path, hash).is_some()
        {
            return Err("runtime reconciliation source identity is ambiguous".into());
        }
    }
    let in_scope = |path: &str| {
        path.starts_with("tests/")
            && path
                .rsplit('/')
                .next()
                .is_some_and(|name| name.starts_with("test_") && name.ends_with(".py"))
    };
    let mut expected = BTreeMap::new();
    let mut all = BTreeSet::new();
    let mut out_of_scope = Vec::new();
    for file in files {
        let path = file["source"].as_str().unwrap();
        for case in file["test_cases"]
            .as_array()
            .ok_or("missing static source cases")?
        {
            let case = case
                .as_str()
                .filter(|case| !case.is_empty())
                .ok_or("invalid static source case")?;
            if !all.insert((path, case)) {
                return Err("runtime reconciliation repeats a static source case".into());
            }
            if !in_scope(path) {
                out_of_scope.push(format!("{path}::{case}"));
                continue;
            }
            let id = format!(
                "{}.{}",
                path.strip_suffix(".py").unwrap().replace('/', "."),
                case
            );
            if expected.insert(id, (file, case)).is_some() {
                return Err("runtime reconciliation has ambiguous static identities".into());
            }
        }
    }
    let mut definitions = BTreeMap::new();
    for file in files {
        let Some(assertions) = file.get("assertions") else {
            continue;
        };
        let methods = assertions["methods"]
            .as_array()
            .ok_or("missing original assertion methods")?;
        let origins = assertions["method_origins"]
            .as_array()
            .ok_or("missing original assertion origins")?;
        if methods.len() != origins.len() {
            return Err("original definition/origin lengths differ".into());
        }
        for (method, origin) in methods.iter().zip(origins) {
            let class = method["class"]
                .as_str()
                .filter(|name| !name.is_empty())
                .ok_or("missing original definition class")?;
            let name = method["method"]
                .as_str()
                .filter(|name| !name.is_empty())
                .ok_or("missing original definition method")?;
            let path = file["source"].as_str().unwrap();
            let line = origin["method_line"]
                .as_u64()
                .filter(|line| *line > 0)
                .ok_or("invalid original method line")?;
            let first = origin["code_first_line"]
                .as_u64()
                .filter(|first| *first > 0 && *first <= line)
                .ok_or("invalid original definition code first line")?;
            let identity = origin["class_identity"]
                .as_str()
                .filter(|identity| {
                    identity
                        .strip_prefix(path)
                        .and_then(|tail| tail.strip_prefix(':'))
                        .and_then(|line| line.parse::<u64>().ok())
                        .is_some_and(|line| line > 0)
                })
                .ok_or("invalid original definition class identity")?;
            if origin["method"] != name || !method["assertions"].is_array() {
                return Err("original definition method/evidence differs".into());
            }
            let key = (path.to_owned(), format!("{class}.{name}"), first);
            if definitions.insert(key, (identity, line, method)).is_some() {
                return Err("original runtime definition evidence is ambiguous".into());
            }
        }
    }
    let rows = observed["cases"]
        .as_array()
        .filter(|rows| !rows.is_empty() && rows.len() <= 4096)
        .ok_or("runtime reconciliation requires bounded nonempty observations")?;
    let mut actual = BTreeMap::new();
    for row in rows {
        let values = row
            .as_array()
            .filter(|values| values.len() == 7)
            .ok_or("invalid collected case schema")?;
        let id = values[0]
            .as_str()
            .filter(|id| id.starts_with("tests.") && id.len() <= 1024)
            .ok_or("invalid collected test identity")?;
        let path = values[1]
            .as_str()
            .ok_or("missing collected definition path")?;
        let admitted_hash = hashes
            .get(path)
            .ok_or("collected definition path is not admitted")?;
        if Some(*admitted_hash) != values[2].as_str()
            || values[3].as_u64().is_none_or(|line| line == 0)
            || !values[4].is_boolean()
            || !values[6].is_boolean()
            || values[5]
                .as_str()
                .is_none_or(|name| name.is_empty() || name.len() > 1024)
            || actual.insert(id.to_owned(), row).is_some()
        {
            return Err("collected definition is unadmitted or identity repeats".into());
        }
    }
    let mut mapped = BTreeMap::new();
    for entry in coverage["cases"]
        .as_array()
        .ok_or("missing coverage cases")?
    {
        let path = entry["source_file"]
            .as_str()
            .ok_or("missing coverage path")?;
        let case = entry["source_case"]
            .as_str()
            .ok_or("missing coverage case")?;
        if !all.contains(&(path, case))
            || entry["source_hash"].as_str() != hashes.get(path).copied()
            || !matches!(
                entry["status"].as_str(),
                Some("pending" | "parity" | "native" | "redundant")
            )
        {
            return Err("coverage case differs from admitted source inventory".into());
        }
        let id = format!(
            "{}.{}",
            path.strip_suffix(".py")
                .ok_or("invalid coverage source suffix")?
                .replace('/', "."),
            case
        );
        if mapped.insert(id, entry).is_some() {
            return Err("runtime reconciliation repeats a coverage identity".into());
        }
    }
    let mut matches = Vec::new();
    let mut gaps = Vec::new();
    let mut runtime_only = Vec::new();
    let mut without_coverage = Vec::new();
    let mut pending = Vec::new();
    let mut collection_skipped = Vec::new();
    let mut original_matches = Vec::new();
    let mut original_gaps = Vec::new();
    for (id, row) in &actual {
        if row[4] == true || row[6] == true {
            collection_skipped.push(id.clone());
        }
        let key = (
            row[1].as_str().unwrap().to_owned(),
            row[5].as_str().unwrap().to_owned(),
            row[3].as_u64().unwrap(),
        );
        if let Some((identity, line, method)) = definitions.get(&key) {
            original_matches.push(json!({"id":id,"defined_in_file":row[1],"defined_in_hash":row[2],
                "code_qualname":row[5],"class_identity":identity,"method_line":line,
                "code_first_line":row[3],"assertion_count":method["assertions"].as_array().unwrap().len()}));
        } else {
            original_gaps.push(id.clone());
        }
        if let Some((file, case)) = expected.get(id) {
            let joined = file["case_assertions"]["cases"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|entry| entry["source_case"] == *case)
                .collect::<Vec<_>>();
            if joined.len() != 1 {
                gaps.push(
                    json!({"id":id,"reason":"unique static assertion evidence is unavailable"}),
                );
            } else {
                let definition = joined[0];
                let path = definition["defined_in_file"]
                    .as_str()
                    .ok_or("missing static definition path")?;
                let admitted_hash = hashes
                    .get(path)
                    .ok_or("static definition path is not admitted")?;
                if Some(*admitted_hash) != definition["defined_in_hash"].as_str()
                    || definition["method_line"]
                        .as_u64()
                        .is_none_or(|line| line == 0)
                {
                    return Err("static assertion definition is not admitted".into());
                }
                let first = definition["code_first_line"]
                    .as_u64()
                    .filter(|line| {
                        *line > 0
                            && definition["method_line"]
                                .as_u64()
                                .is_some_and(|method| *line <= method)
                    })
                    .ok_or("static assertion requires independent code-first-line provenance")?;
                if definition["defined_in_file"] == row[1]
                    && definition["defined_in_hash"] == row[2]
                    && row[3].as_u64() == Some(first)
                {
                    matches.push(id.clone());
                } else {
                    // A method decorator starts its code span before the AST def line.
                    gaps.push(json!({"id":id,"reason":"definition span differs or requires decorator provenance",
                        "observed_file":row[1],"observed_code_line":row[3],
                        "static_file":definition["defined_in_file"],"static_method_line":definition["method_line"],
                        "static_code_first_line":first}));
                }
            }
        } else {
            runtime_only.push(id.clone());
        }
        match mapped.get(id) {
            None => without_coverage.push(id.clone()),
            Some(entry) if entry["status"] == "pending" => pending.push(id.clone()),
            _ => {}
        }
    }
    let unobserved = expected
        .keys()
        .filter(|id| !actual.contains_key(*id))
        .cloned()
        .collect::<Vec<_>>();
    let unobserved_coverage = mapped
        .keys()
        .filter(|id| expected.contains_key(*id) && !actual.contains_key(*id))
        .cloned()
        .collect::<Vec<_>>();
    out_of_scope.sort();
    Ok(
        json!({"source_revision":revision,"source_runtime":"CPython3.11.9","collection_only":true,
        "runtime_cases":actual.len(),"static_candidates":expected.len(),"definition_matches":matches,
        "definition_gaps":gaps,"original_definition_matches":original_matches,"original_definition_gaps":original_gaps,
        "runtime_only":runtime_only,"static_unobserved":unobserved,
        "runtime_without_coverage":without_coverage,"unobserved_coverage":unobserved_coverage,
        "observed_pending":pending,"collection_skipped":collection_skipped,"out_of_collection_scope":out_of_scope,
        "static_discovery_complete":inventory["discovery_complete"],
        "coverage_verified_by_collection":0,"release_ready":false,
        "receipt_provenance":"caller must authenticate observed collection/run independently"}),
    )
}

pub(crate) fn observe(root: &Path, stage: &Path, guard: &Source) -> Result<Value, String> {
    admit(root, stage, guard)?;
    let manifest: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/source-baseline.json"
    ))
    .map_err(|_| "invalid runtime collection source manifest")?;
    let hashes = manifest["files"]
        .as_array()
        .ok_or("runtime collection requires original source hashes")?
        .iter()
        .map(|row| {
            Ok((
                row["source"]
                    .as_str()
                    .ok_or("missing original path")?
                    .to_owned(),
                row["sha256"]
                    .as_str()
                    .ok_or("missing original hash")?
                    .to_owned(),
            ))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    Python::initialize();
    let report = Python::attach(|py| -> PyResult<Value> {
        crate::source_oracle_runtime::require_runtime(py)
            .map_err(pyo3::exceptions::PyRuntimeError::new_err)?;
        let sys = py.import("sys")?;
        let modules = sys.getattr("modules")?;
        for prefix in ["tests", "emojikit", "coins", "scripts"] {
            if modules.contains(prefix)? {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "runtime collection requires fresh original package bindings",
                ));
            }
        }
        let search_path = stage.to_str().ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err("runtime collection stage must be UTF-8")
        })?;
        sys.getattr("path")?.call_method1("insert", (0, search_path))?;
        let package = py.import("tests")?;
        let origin: PathBuf = package.getattr("__file__")?.extract()?;
        let expected = crate::migration_bundle::resolved(&stage.join("tests/__init__.py"))
            .map_err(pyo3::exceptions::PyValueError::new_err)?;
        if crate::migration_bundle::resolved(&origin)
            .map_err(pyo3::exceptions::PyValueError::new_err)? != expected
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "runtime collection did not import the admitted original guard",
            ));
        }
        let socket = py.import("socket")?.getattr("socket")?;
        for method in ["connect", "connect_ex"] {
            if socket.getattr(method)?.getattr("__module__")?.extract::<String>()? != "tests" {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "runtime collection socket guard is missing",
                ));
            }
        }
        let environment = py.import("os")?.getattr("environ")?;
        if environment.get_item("NUMERA_EMOJI_MAPPER_NO_DOTENV")?.extract::<String>()? != "1"
            || environment.call_method1("get", ("NUMERA_EMOJI_MAPPER_NO_BROWSER_TESTS", ""))?.extract::<String>()? == "1"
        {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "runtime collection cannot omit the credential guard or browser definitions",
            ));
        }
        let unittest = py.import("unittest")?;
        let loader = unittest.getattr("TestLoader")?.call0()?;
        let suite = loader.call_method1(
            "discover",
            (stage.join("tests"), "test_*.py", stage.to_path_buf()),
        )?;
        if loader.getattr("errors")?.len()? != 0 {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "original runtime collection contains failed imports; no completeness claim",
            ));
        }
        let suite_type = unittest.getattr("TestSuite")?;
        let case_type = unittest.getattr("TestCase")?;
        let inspect = py.import("inspect")?;
        let physical = crate::tooling_environment::physical_root(stage)
            .map_err(pyo3::exceptions::PyValueError::new_err)?;
        let mut pending = vec![(suite, 0usize)];
        let mut cases = Vec::new();
        let mut ids = BTreeSet::new();
        while let Some((item, depth)) = pending.pop() {
            if depth > 32 || cases.len() >= 4096 {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "original runtime collection exceeds depth/case bounds",
                ));
            }
            if item.is_instance(&suite_type)? {
                let children = item.try_iter()?.take(4097).collect::<PyResult<Vec<_>>>()?;
                if children.len() > 4096 {
                    return Err(pyo3::exceptions::PyValueError::new_err(
                        "original runtime collection exceeds suite bound",
                    ));
                }
                pending.extend(children.into_iter().rev().map(|child| (child, depth + 1)));
                continue;
            }
            if !item.is_instance(&case_type)? {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "original runtime collection contains an unsupported callable",
                ));
            }
            let class = item.get_type();
            let module: String = class.getattr("__module__")?.extract()?;
            if !module.starts_with("tests.") {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "original runtime collection contains synthetic skipped/import-failure cases",
                ));
            }
            let id: String = item.call_method0("id")?.extract()?;
            if !ids.insert(id.clone()) {
                return Err(pyo3::exceptions::PyValueError::new_err("original runtime collection repeats a test identity"));
            }
            let name: String = item.getattr("_testMethodName")?.extract()?;
            let bound_method = item.getattr(name.as_str())?;
            let skipped = match class.getattr_opt("__unittest_skip__")? {
                Some(value) => value.is_truthy()?,
                None => false,
            };
            // unittest checks the decorated callable, while provenance belongs to its original body.
            let method_skipped = match bound_method.getattr_opt("__unittest_skip__")? {
                Some(value) => value.is_truthy()?,
                None => false,
            };
            let method = inspect.call_method1("unwrap", (&bound_method,))?;
            let code = method.getattr("__code__")?;
            let file: PathBuf = code.getattr("co_filename")?.extract()?;
            let file = crate::migration_bundle::resolved(&file)
                .map_err(pyo3::exceptions::PyValueError::new_err)?;
            let relative = file.strip_prefix(&physical)
                .map_err(|_| pyo3::exceptions::PyValueError::new_err("collected assertion escaped admitted sources"))?
                .to_str().ok_or_else(|| pyo3::exceptions::PyValueError::new_err("collected assertion path is not UTF-8"))?
                .replace('\\', "/");
            let hash = hashes.get(&relative).ok_or_else(||
                pyo3::exceptions::PyValueError::new_err("collected assertion has no original source identity"))?;
            let line: u64 = code.getattr("co_firstlineno")?.extract()?;
            let qualified: String = code.getattr("co_qualname")?.extract()?;
            if line == 0 || id.len() > 1024 || qualified.is_empty() || qualified.len() > 1024 {
                return Err(pyo3::exceptions::PyValueError::new_err("invalid collected assertion identity"));
            }
            cases.push(json!([id,relative,hash,line,skipped,qualified,method_skipped]));
        }
        if cases.is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err("original runtime collection is empty"));
        }
        Ok(json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9",
            "collection_only":true,"tests_run":0,"guard_imported_first":true,
            "case_fields":["id","defined_in_file","defined_in_hash","code_line","class_skipped","code_qualname","method_skipped"],
            "cases":cases,"loader_errors":0,
            "acceptance":"collection is not original/native test execution or source-retirement approval"}))
    })
    .map_err(|error| format!("original runtime collection failed: {}", crate::logging::redact(&error.to_string(), &[])))?;
    crate::source_import_replay::validate(root, stage)?;
    Ok(report)
}
