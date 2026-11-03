//! Fixed historical inventory functions and their unchanged original assertion classes.
use crate::source_git_inventory::Source;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use serde_json::{Value, json};
use std::path::Path;

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const IDENTITIES: [(&str, &str); 2] = [
    (
        "tests/reference/fetch_emoji_ids.py",
        "47488444ab9122be1806ae8bb321031a7a888924042374ca97bbbdf362702f2b",
    ),
    (
        "tests/test_fetch_emoji_ids.py",
        "d0d35f40f9cd05640ccc126a8d314a295b667823d459adbf85cc016be4419bc0",
    ),
];
const CLASSES: [(&str, &[&str]); 3] = [
    (
        "ExtractRealIds",
        &[
            "test_bare_entry_lines",
            "test_example_prose_line_is_ignored",
            "test_trailing_label_and_emoji_still_captured",
            "test_bullet_and_spacing_variants",
            "test_bare_number_list_fallback",
            "test_short_or_nonnumeric_noise_ignored",
            "test_overlong_number_not_truncated",
        ],
    ),
    (
        "WithinFileDuplicates",
        &[
            "test_detects_true_within_file_duplicate",
            "test_example_line_is_not_a_duplicate",
        ],
    ),
    (
        "CollectIdsDedup",
        &[
            "test_cross_file_dedup_preserves_first_seen_order",
            "test_within_and_cross_file_and_inline_all_collapse",
            "test_example_ids_never_downloaded",
        ],
    ),
];

pub fn source_identity(source: &Source, test: &Source) -> Result<(), String> {
    for (source, (path, hash)) in [source, test].into_iter().zip(IDENTITIES) {
        if source.baseline_revision != BASELINE
            || source.path != path
            || source.sha256 != hash
            || crate::codec_raster::digest_hex(source.text.as_bytes()) != hash
        {
            return Err("inventory oracle source identity drift; refusing execution".into());
        }
    }
    Ok(())
}

fn patterns<'py>(py: Python<'py>, source: &Source, bindings: &Bound<'py, PyDict>) -> PyResult<()> {
    let ast = py.import("ast")?;
    let regex = py.import("re")?;
    let tree = ast.call_method1("parse", (&source.text, &source.path))?;
    let mut found = std::collections::BTreeSet::new();
    for node in crate::source_test_inventory::body(&tree)? {
        if crate::source_test_inventory::kind(&node)? != "Assign" {
            continue;
        }
        let targets = node.getattr("targets")?;
        if targets.len()? != 1 {
            continue;
        }
        let target = targets.get_item(0)?;
        if crate::source_test_inventory::kind(&target)? != "Name" {
            continue;
        }
        let name: String = target.getattr("id")?.extract()?;
        if !["_REAL_ENTRY", "_ANY_MENTION", "_BARE_ID"].contains(&name.as_str()) {
            continue;
        }
        if !found.insert(name.clone()) {
            return Err(PyValueError::new_err(
                "duplicate original inventory pattern",
            ));
        }
        let value = node.getattr("value")?;
        if crate::source_test_inventory::kind(&value)? != "Call" {
            return Err(PyValueError::new_err("inventory pattern is not a call"));
        }
        let function = value.getattr("func")?;
        let args = value.getattr("args")?;
        let ignore_case = name != "_BARE_ID";
        if crate::source_test_inventory::kind(&function)? != "Attribute"
            || function.getattr("attr")?.extract::<String>()? != "compile"
            || function
                .getattr("value")?
                .getattr("id")?
                .extract::<String>()?
                != "re"
            || args.len()? != if ignore_case { 2 } else { 1 }
            || value.getattr("keywords")?.len()? != 0
        {
            return Err(PyValueError::new_err(
                "original inventory pattern assignment drift",
            ));
        }
        let pattern = ast.call_method1("literal_eval", (args.get_item(0)?,))?;
        let flags = if ignore_case {
            let flag = args.get_item(1)?;
            if crate::source_test_inventory::kind(&flag)? != "Attribute"
                || flag.getattr("attr")?.extract::<String>()? != "IGNORECASE"
                || flag.getattr("value")?.getattr("id")?.extract::<String>()? != "re"
            {
                return Err(PyValueError::new_err(
                    "original inventory pattern flags drift",
                ));
            }
            regex.getattr("IGNORECASE")?.extract::<u32>()?
        } else {
            0
        };
        bindings.set_item(name, regex.call_method1("compile", (pattern, flags))?)?;
    }
    if found.len() != 3 {
        return Err(PyValueError::new_err("missing original inventory patterns"));
    }
    Ok(())
}

fn required<'py>(globals: &Bound<'py, PyDict>, name: &str) -> PyResult<Bound<'py, PyAny>> {
    globals
        .get_item(name)?
        .ok_or_else(|| PyValueError::new_err("missing original inventory definition"))
}

fn execute(py: Python<'_>, workspace: &Path, source: &Source, test: &Source) -> PyResult<Value> {
    let stream = py.import("io")?.getattr("StringIO")?.call0()?;
    let logging = py.import("logging")?;
    // An unattached Logger captures only these source calls, without changing global handlers.
    let logger = logging
        .getattr("Logger")?
        .call1(("original-inventory", 20))?;
    let handler = logging.getattr("StreamHandler")?.call1((&stream,))?;
    logger.call_method1("addHandler", (&handler,))?;
    let result = (|| -> PyResult<Value> {
        let path = py.import("pathlib")?.getattr("Path")?;
        let bindings = PyDict::new(py);
        patterns(py, source, &bindings)?;
        bindings.set_item("Path", &path)?;
        bindings.set_item("log", &logger)?;
        let globals = crate::source_oracle_runtime::namespace(
            py,
            source,
            &["extract_real_ids", "within_file_duplicates", "collect_ids"],
            &["len", "max", "set", "sorted", "sum"],
            &bindings,
        )?;
        let unittest = py.import("unittest")?;
        let options = PyDict::new(py);
        options.set_item("dir", workspace)?;
        let temporary = py.import("types")?.getattr("SimpleNamespace")?.call0()?;
        temporary.setattr(
            "TemporaryDirectory",
            py.import("functools")?.getattr("partial")?.call(
                (py.import("tempfile")?.getattr("TemporaryDirectory")?,),
                Some(&options),
            )?,
        )?;
        let test_bindings = PyDict::new(py);
        test_bindings.set_item("unittest", &unittest)?;
        test_bindings.set_item("Path", &path)?;
        test_bindings.set_item("tempfile", temporary)?;
        for name in ["extract_real_ids", "within_file_duplicates", "collect_ids"] {
            test_bindings.set_item(name, required(&globals, name)?)?;
        }
        for name in ["REAL_A", "REAL_B", "REAL_C", "EXAMPLE"] {
            test_bindings.set_item(
                name,
                crate::source_logging_oracle::literal(py, test, name, false)?,
            )?;
        }
        let tests = crate::source_oracle_runtime::namespace(
            py,
            test,
            &["ExtractRealIds", "WithinFileDuplicates", "CollectIdsDedup"],
            &["__build_class__", "str"],
            &test_bindings,
        )?;
        let loader = unittest.getattr("TestLoader")?.call0()?;
        let results = unittest.getattr("TestResult")?.call0()?;
        let mut cases = Vec::new();
        for (name, methods) in CLASSES {
            let class = required(&tests, name)?;
            let actual: Vec<String> = loader
                .call_method1("getTestCaseNames", (&class,))?
                .extract()?;
            let mut expected = methods.to_vec();
            expected.sort_unstable();
            if actual != expected {
                return Err(PyValueError::new_err("original inventory collection drift"));
            }
            for method in methods {
                class.call1((*method,))?.call_method1("run", (&results,))?;
                cases.push(format!("{name}.{method}"));
            }
        }
        for field in [
            "errors",
            "failures",
            "skipped",
            "expectedFailures",
            "unexpectedSuccesses",
        ] {
            if results.getattr(field)?.len()? != 0 {
                return Err(PyValueError::new_err(format!(
                    "original inventory assertions reported {field}"
                )));
            }
        }
        if results.getattr("testsRun")?.extract::<usize>()? != 12
            || !results.call_method0("wasSuccessful")?.extract::<bool>()?
        {
            return Err(PyValueError::new_err(
                "original inventory execution incomplete",
            ));
        }
        let mut entries = Vec::new();
        for text in [
            "premium-id:111111111\npremium-id:222222222\npremium-id:111111111\n".to_owned(),
            "example premium-id:111111111\npremium-id:222222222 👋 label\n".to_owned(),
            "  - PREMIUM-ID : 111111111\n* premium-id:222222222\n".to_owned(),
            "111111111\n222222222\n111111111\n".to_owned(),
            format!("premium-id:{}\npremium-id:111111111\n", "9".repeat(26)),
            "PREMİUM-ıD:\u{1f}۱۲۳۴۵\u{2028}premium-id:１２３４５\n".to_owned(),
            "prose premium-id:\n222222222\npremium-id:111111111\n".to_owned(),
            "premium-id:123\nprose 111111111\n".to_owned(),
        ] {
            let ids: Vec<String> = required(&globals, "extract_real_ids")?
                .call1((&text,))?
                .extract()?;
            let duplicates: Vec<(String, usize)> = required(&globals, "within_file_duplicates")?
                .call1((&text,))?
                .call_method0("items")?
                .try_iter()?
                .map(|item| item?.extract())
                .collect::<PyResult<_>>()?;
            entries.push(json!({"text":text,"ids":ids,"duplicates":duplicates}));
        }
        let ties = (10000..10012)
            .rev()
            .map(|id| id.to_string())
            .collect::<Vec<_>>();
        let mut repeated = ties.clone();
        repeated.extend(ties);
        let scenarios = [
            (
                "cross-file",
                vec![
                    ("a.md", "premium-id:111111111\npremium-id:222222222\n"),
                    ("b.md", "premium-id:222222222\npremium-id:333333333\n"),
                ],
                vec![],
            ),
            (
                "inline-collapse",
                vec![("a.md", "premium-id:111111111\npremium-id:111111111\n")],
                vec!["111111111".into(), "222222222".into()],
            ),
            (
                "prose-whitespace",
                vec![(
                    "a.md",
                    "prose premium-id:\n222222222\npremium-id:111111111\n",
                )],
                vec![],
            ),
            (
                "bare-clamp",
                vec![("a.md", "111111111\n222222222\n")],
                vec![" ²①𐩁 ".into(), "¼".into(), "۱۲۳۴۵".into()],
            ),
            ("stable-top-ten", vec![], repeated),
        ];
        let mut collections = Vec::new();
        for (name, files, inline) in scenarios {
            let mut filenames = Vec::new();
            for (name, text) in &files {
                let filename = workspace.join(name);
                std::fs::write(&filename, text.as_bytes())?;
                filenames.push(
                    filename
                        .to_str()
                        .ok_or_else(|| PyValueError::new_err("inventory path is not UTF-8"))?
                        .to_owned(),
                );
            }
            stream.call_method1("seek", (0,))?;
            stream.call_method1("truncate", (0,))?;
            let ids: Vec<String> = required(&globals, "collect_ids")?
                .call1((PyList::new(py, &filenames)?, PyList::new(py, &inline)?))?
                .extract()?;
            let messages: Vec<String> = stream
                .call_method0("getvalue")?
                .call_method0("splitlines")?
                .extract()?;
            for (filename, (_, text)) in filenames.iter().zip(&files) {
                if std::fs::read(filename)? != text.as_bytes() {
                    return Err(PyValueError::new_err(
                        "original inventory modified its input",
                    ));
                }
                std::fs::remove_file(filename)?;
            }
            collections.push(json!({"name":name,"files":files.iter().map(|(name,text)|json!({"name":name,"text":text})).collect::<Vec<_>>(),"inline":inline,"ids":ids,"messages":messages}));
        }
        Ok(
            json!({"source_revision":BASELINE,"source_runtime":"CPython3.11.9",
            "source_hash":source.sha256,"test_source_hash":test.sha256,"cases":cases,
            "tests_run":12,"errors":0,"failures":0,"skipped":0,"expected_failures":0,
            "unexpected_successes":0,"entries":entries,"collections":collections,
            "ports":"original three functions/pattern assignments/test classes/constants unchanged; external project-confined TemporaryDirectory and private stdlib Logger capture; application import/download/main excluded; native strict UTF-8 refusal intentionally differs from original replacement decoding"}),
        )
    })();
    let detached = logger.call_method1("removeHandler", (&handler,));
    let handler_closed = handler.call_method0("close");
    let stream_closed = stream.call_method0("close");
    detached?;
    handler_closed?;
    stream_closed?;
    result
}

pub fn observe(
    root: &Path,
    workspace: &Path,
    source: &Source,
    test: &Source,
) -> Result<Value, String> {
    source_identity(source, test)?;
    let root = crate::tooling_environment::physical_root(root)?;
    let workspace = crate::tooling_environment::physical_root(workspace)?;
    if workspace == root.join("logs/tmp") || !workspace.starts_with(root.join("logs/tmp")) {
        return Err("inventory oracle workspace must stay inside project scratch".into());
    }
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot inspect inventory workspace")?
        .next()
        .is_some()
    {
        return Err("inventory oracle workspace must be empty".into());
    }
    Python::initialize();
    let report = Python::attach(|py| {
        crate::source_oracle_runtime::require_runtime(py)?;
        execute(py, &workspace, source, test).map_err(|error| {
            format!(
                "original inventory replay failed: {}",
                crate::logging::redact(&error.to_string(), &[])
            )
        })
    })?;
    if std::fs::read_dir(&workspace)
        .map_err(|_| "cannot verify inventory cleanup")?
        .next()
        .is_some()
    {
        return Err("original inventory tests left scratch; retained for diagnosis".into());
    }
    Ok(report)
}
