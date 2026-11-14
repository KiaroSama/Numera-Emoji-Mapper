//! Original report bytes come from the admitted historical functions, not native formatting.
use super::modules::Modules;
use crate::source_git_inventory::Source;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use std::path::Path;

const LOGO: &str = "6f85376de3893dfa1e78045ce264103b0055acec400dd8a35222405d5c4f6951";

pub(super) fn admit(root: &Path) -> Result<std::path::PathBuf, String> {
    let path = root.join("assets/numera-emoji-mapper-logo.png");
    crate::tooling_environment::reject_links(&path, "identity report input cannot follow links")?;
    let path = crate::migration_bundle::resolved(&path)?;
    if !path.starts_with(root) || crate::tooling_manifest::digest(&path)? != LOGO {
        return Err("original identity report public image drift".into());
    }
    Ok(path)
}

fn capture(py: Python<'_>, function: &Bound<'_, PyAny>, data: &Path) -> PyResult<Value> {
    let sys = py.import("sys")?;
    let stdout = sys.getattr("stdout")?;
    let output = py.import("io")?.getattr("StringIO")?.call0()?;
    let migrate = function.getattr("__name__")?.extract::<String>()? == "migrate";
    let redirect = py
        .import("contextlib")?
        .getattr("redirect_stdout")?
        .call1((&output,))?;
    redirect.call_method0("__enter__")?;
    let result = if migrate {
        function.call1((data, false))
    } else {
        function.call1((data,))
    };
    let restored = redirect.call_method1("__exit__", (py.None(), py.None(), py.None()));
    restored?;
    if !sys.getattr("stdout")?.is(&stdout) {
        return Err(PyValueError::new_err(
            "original report stream not restored; retained",
        ));
    }
    Ok(
        json!({"code":result?.extract::<u8>()?,"stdout":output.call_method0("getvalue")?.extract::<String>()?}),
    )
}

pub(super) fn run(
    py: Python<'_>,
    root: &Path,
    sources: [&Source; 2],
    modules: &Modules<'_>,
    ledger: &Bound<'_, PyAny>,
) -> PyResult<Value> {
    let input = admit(root).map_err(PyValueError::new_err)?;
    let bytes = std::fs::read(&input)?;
    let temporary = ledger.call_method0("temporary")?;
    let data = temporary.getattr("name")?.extract::<std::path::PathBuf>()?;
    let image = data.join("source.png");
    std::fs::write(&image, &bytes)?;
    let original = crate::source_image_identity::build(py, sources[0], sources[1])?;
    let (key, hash): (String, u64) = original
        .identity
        .call_method1("fingerprint", (&image, "static"))?
        .extract()?;
    if key != "s:1745338c58098cc08d6406ac9d629f6a" || hash != 1096366658209276687 {
        return Err(PyValueError::new_err(
            "original report static identity drift; retained",
        ));
    }
    let catalog = data.join("catalog.db");
    let db = py.import("sqlite3")?.call_method1("connect", (&catalog,))?;
    let seeded = (|| -> PyResult<()> {
        db.call_method1("executescript", ("CREATE TABLE items(content_key TEXT,file_path TEXT,format TEXT,phash INTEGER);CREATE TABLE publications(content_key TEXT);CREATE TABLE seen_files(content_key TEXT);",))?;
        db.call_method1(
            "execute",
            (
                "INSERT INTO items VALUES(?,?,?,?)",
                (&key, "./source.png", "static", hash as i64),
            ),
        )?;
        db.call_method0("commit")?;
        Ok(())
    })();
    let closed = db.call_method0("close");
    seeded?;
    closed?;
    let before = std::fs::read(&catalog)?;
    let plan = data.join("publish_plan_fixture.json");
    let document = PyDict::new(py);
    document.set_item("static", [key.as_str(), "s:obsolete"])?;
    modules
        .state
        .call_method1("write_json_atomic", (&plan, &document))?;
    let saved = modules.identity.getattr("fingerprint")?;
    modules
        .identity
        .setattr("fingerprint", original.identity.getattr("fingerprint")?)?;
    let observed = (|| -> PyResult<Value> {
        let maintenance = modules.migration.call_method1("maintenance", (&data,))?;
        maintenance.call_method0("__enter__")?;
        let report = capture(py, &modules.repair.getattr("report")?, &data);
        let released = maintenance.call_method1("__exit__", (py.None(), py.None(), py.None()));
        let report = report?;
        released?;
        if report["code"] != 0 || std::fs::read(&catalog)? != before {
            return Err(PyValueError::new_err(
                "original report changed storage or failed; retained",
            ));
        }
        std::fs::remove_file(&plan)?;
        let preview = capture(py, &modules.repair.getattr("migrate")?, &data)?;
        if preview["code"] != 0
            || std::fs::read(&catalog)? != before
            || std::fs::read(&image)? != bytes
            || crate::process_owner::cleanup_pending(&data)
        {
            return Err(PyValueError::new_err(
                "original report preview changed storage; retained",
            ));
        }
        let entries = std::fs::read_dir(&data)?
            .map(|e| e.map(|e| e.file_name()))
            .collect::<std::io::Result<std::collections::BTreeSet<_>>>()?;
        if entries
            != ["catalog.db", "source.png", ".maintenance.lock"]
                .map(std::ffi::OsString::from)
                .into_iter()
                .collect()
        {
            return Err(PyValueError::new_err(
                "original report fixture inventory differs; retained",
            ));
        }
        Ok(
            json!({"catalog":catalog,"content_key":key,"phash":hash,"report":report,"preview":preview}),
        )
    })();
    let restored = modules.identity.setattr("fingerprint", &saved);
    restored?;
    if !modules.identity.getattr("fingerprint")?.is(&saved) {
        return Err(PyValueError::new_err(
            "original report fingerprint not restored; retained",
        ));
    }
    let observed = observed?;
    temporary.call_method0("cleanup")?;
    admit(root).map_err(PyValueError::new_err)?;
    Ok(observed)
}
