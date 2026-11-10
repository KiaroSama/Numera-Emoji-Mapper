//! Private original migration modules; fixed source admission belongs to the identity replay.
use crate::source_git_inventory::Source;
use crate::source_tooling_oracle::required;
use pyo3::exceptions::{PyImportError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};
use std::path::Path;

#[pyclass]
pub(crate) struct Imports {
    collection: Py<PyAny>,
    bundle: Py<PyAny>,
    reconcile: Py<PyAny>,
    decoder: Py<PyAny>,
}
#[pymethods]
impl Imports {
    #[pyo3(signature = (name, _globals=None, _locals=None, fromlist=None, level=0))]
    fn __call__<'py>(
        &self,
        py: Python<'py>,
        name: &str,
        _globals: Option<&Bound<'_, PyAny>>,
        _locals: Option<&Bound<'_, PyAny>>,
        fromlist: Option<&Bound<'_, PyAny>>,
        level: i32,
    ) -> PyResult<Bound<'py, PyAny>> {
        let from = fromlist
            .filter(|v| !v.is_none())
            .map(|v| v.extract::<Vec<String>>())
            .transpose()?
            .unwrap_or_default();
        if level == 1 && name == "collection_state" && from == ["_lock_path"] {
            return Ok(self.collection.bind(py).clone());
        }
        if level == 0 {
            if ["tempfile", "argparse", "sqlite3"].contains(&name) && from.is_empty() {
                return Ok(crate::source_oracle_runtime::host_import(py, name)?.into_any());
            }
            if name == "tests.reference.collection_reconcile" && from == ["reconcile_set"] {
                return Ok(self.reconcile.bind(py).clone());
            }
            if name == "tests._migration_process" && from == ["_decode"] {
                return Ok(self.decoder.bind(py).clone());
            }
            if name == "tests.reference" && from == ["migration_bundle"] {
                let package = crate::source_oracle_runtime::host_import(py, "types")?
                    .getattr("SimpleNamespace")?
                    .call0()?;
                package.setattr("migration_bundle", self.bundle.bind(py))?;
                return Ok(package);
            }
        }
        Err(PyImportError::new_err(
            "unexpected original identity replay import",
        ))
    }
}

pub(crate) struct Modules<'py> {
    pub migration: Bound<'py, PyModule>,
    pub repair: Bound<'py, PyModule>,
    pub state: Bound<'py, PyModule>,
    pub bundle: Bound<'py, PyModule>,
    pub collection: Bound<'py, PyModule>,
    pub catalog: Bound<'py, PyAny>,
    pub identity: Bound<'py, PyAny>,
    pub imports: Py<Imports>,
}

pub(crate) fn build<'py>(
    py: Python<'py>,
    workspace: &Path,
    sources: &[&Source],
) -> PyResult<Modules<'py>> {
    let path = py.import("pathlib")?.getattr("Path")?;
    let ns = py.import("types")?.getattr("SimpleNamespace")?;
    let state_globals =
        crate::source_reference_state::namespace(py, sources[6], Some(&workspace.join("locks")))?;
    let state =
        crate::source_roster_modules::rebind(py, &state_globals, "_original_identity_state")?;
    let ownership = crate::source_reference_state::ownership(
        py,
        sources[5],
        &state.dict(),
        "_original_identity_ownership",
    )?;
    let paths = PyDict::new(py);
    paths.set_item("Path", &path)?;
    paths.set_item("PROJECT_ROOT", path.call1((workspace,))?)?;
    paths.set_item("DATA_PREFIX", "./")?;
    let paths = crate::source_oracle_runtime::module(
        py,
        sources[9],
        &["store", "resolve"],
        &["str", "len"],
        &paths,
        "_original_identity_paths",
    )?;
    let snapshot = PyDict::new(py);
    for name in ["math", "sqlite3", "time"] {
        snapshot.set_item(name, py.import(name)?)?;
    }
    for name in ["BACKUP_TIMEOUT", "RESTORE_TIMEOUT"] {
        snapshot.set_item(
            name,
            crate::source_logging_oracle::literal(py, sources[10], name, false)?,
        )?;
    }
    let snapshot = crate::source_oracle_runtime::module(
        py,
        sources[10],
        &["BackupTimeout", "backup"],
        &["__build_class__", "RuntimeError", "ValueError", "min"],
        &snapshot,
        "_original_identity_snapshot",
    )?;
    let artifacts = PyDict::new(py);
    artifacts.set_item("Path", &path)?;
    artifacts.set_item("copy", py.import("copy")?)?;
    artifacts.set_item("PACK_PLAN_NAME", "pack_plan.json")?;
    let artifacts = crate::source_oracle_runtime::module(
        py,
        sources[11],
        &[
            "is_state_name",
            "state_files",
            "_plan_refs",
            "plan_keys",
            "remap_plan",
        ],
        &[
            "isinstance",
            "str",
            "list",
            "dict",
            "all",
            "range",
            "len",
            "sorted",
            "ValueError",
        ],
        &artifacts,
        "_original_identity_artifacts",
    )?;
    let errors = crate::source_oracle_runtime::namespace(
        py,
        sources[13],
        &["MediaError"],
        &["__build_class__", "RuntimeError"],
        &PyDict::new(py),
    )?;
    let media_error = required(&errors, "MediaError")?;
    let identity = ns.call0()?;
    let hamm = crate::source_oracle_runtime::namespace(
        py,
        sources[12],
        &["hamming"],
        &[],
        &PyDict::new(py),
    )?;
    identity.setattr("hamming", required(&hamm, "hamming")?)?;
    let options = PyDict::new(py);
    options.set_item(
        "side_effect",
        media_error.call1(("original test did not supply its decoder",))?,
    )?;
    for name in ["fingerprint", "preserve_collision_key"] {
        identity.setattr(
            name,
            py.import("unittest.mock")?
                .getattr("Mock")?
                .call((), Some(&options))?,
        )?;
    }
    let hashes = PyDict::new(py);
    hashes.set_item("_U64", u64::MAX)?;
    let hashes = crate::source_oracle_runtime::namespace(
        py,
        sources[7],
        &["_phash_to_db", "_phash_from_db"],
        &["int"],
        &hashes,
    )?;
    let common = PyDict::new(py);
    for name in ["json", "sqlite3", "time", "hashlib", "os"] {
        common.set_item(name, py.import(name)?)?;
    }
    common.set_item("Path", &path)?;
    common.set_item("media_paths", paths)?;
    common.set_item("sqlite_snapshot", snapshot)?;
    common.set_item("state_artifacts", artifacts)?;
    common.set_item(
        "canonical_directory",
        ownership.getattr("canonical_directory")?,
    )?;
    common.set_item("write_json_atomic", state.getattr("write_json_atomic")?)?;
    common.set_item("VERSION", 2)?;
    common.set_item(
        "REFERENCES",
        crate::source_logging_oracle::literal(py, sources[3], "REFERENCES", false)?,
    )?;
    let bundle = crate::source_oracle_runtime::module(
        py,
        sources[3],
        &[
            "file_digest",
            "_quote",
            "database_signature",
            "signature",
            "rewrite_database",
            "plan_files",
            "make_bundle",
            "validate_bundle",
            "verify_files",
            "verify_states",
            "verify_current",
            "verify_applied",
            "move_file",
            "apply_files",
            "restore",
        ],
        &[
            "list",
            "sorted",
            "repr",
            "enumerate",
            "dict",
            "set",
            "len",
            "str",
            "reversed",
            "isinstance",
            "RuntimeError",
        ],
        &common,
        "_original_identity_bundle",
    )?;
    let collection = PyDict::new(py);
    collection.set_item("Path", &path)?;
    let collection = crate::source_oracle_runtime::module(
        py,
        sources[8],
        &["SetDrift", "_state_path", "_lock_path"],
        &["__build_class__", "Exception"],
        &collection,
        "_original_identity_collection",
    )?;
    let migration = common.copy()?;
    migration.set_item("bundle", &bundle)?;
    migration.set_item("packstate", &state)?;
    migration.set_item("identity", &identity)?;
    migration.set_item("MediaError", &media_error)?;
    migration.set_item("maintenance", ownership.getattr("maintenance")?)?;
    migration.set_item("JOURNAL_NAME", ownership.getattr("JOURNAL_NAME")?)?;
    migration.set_item("JOURNAL_VERSION", 2)?;
    migration.set_item("ExitStack", py.import("contextlib")?.getattr("ExitStack")?)?;
    migration.set_item("dataclass", py.import("dataclasses")?.getattr("dataclass")?)?;
    migration.set_item("field", py.import("dataclasses")?.getattr("field")?)?;
    migration.set_item(
        "log",
        py.import("logging")?
            .call_method1("getLogger", ("original_identity_migration",))?,
    )?;
    for name in ["_phash_to_db", "_phash_from_db"] {
        migration.set_item(name, required(&hashes, name)?)?;
    }
    for name in ["KEY_REFERENCES", "STAGES"] {
        migration.set_item(
            name,
            crate::source_logging_oracle::literal(py, sources[2], name, false)?,
        )?;
    }
    let mut registration = crate::source_oracle_runtime::ModuleRegistration::new(py, &migration)?;
    let migration_globals = crate::source_oracle_runtime::namespace(
        py,
        sources[2],
        &[
            "Survey",
            "catalog_path",
            "survey",
            "_collisions",
            "backup_catalog",
            "_verify_backup",
            "journal_path",
            "read_journal",
            "_write_journal",
            "_apply_database",
            "state_files",
            "_remap",
            "_apply_state",
            "_apply_files",
            "_bases",
            "apply_migration",
            "restore_migration",
            "recover_key_map",
            "stale_state_keys",
            "required_state_keys",
            "invariant_issues",
            "_collect_keys",
        ],
        &[
            "__build_class__",
            "property",
            "frozenset",
            "bool",
            "len",
            "set",
            "dict",
            "list",
            "str",
            "int",
            "isinstance",
            "sorted",
            "range",
            "next",
            "iter",
            "type",
            "any",
            "RuntimeError",
            "FileExistsError",
            "FileNotFoundError",
            "BaseException",
            "Exception",
            "OSError",
            "ValueError",
        ],
        &migration,
    )?;
    registration.restore()?;
    let migration = crate::source_roster_modules::rebind(
        py,
        &migration_globals,
        "_original_identity_migration",
    )?;
    let repair = PyDict::new(py);
    repair.set_item("Path", &path)?;
    repair.set_item("argparse", py.import("argparse")?)?;
    repair.set_item("identity", &identity)?;
    repair.set_item("cm", &migration)?;
    repair.set_item("ROOT", path.call1((workspace,))?)?;
    for (name, value) in [
        ("EXIT_OK", 0),
        ("EXIT_USAGE", 2),
        ("EXIT_STALE", 3),
        ("EXIT_FAILED", 4),
    ] {
        repair.set_item(name, value)?;
    }
    let ast = py.import("ast")?;
    let tree = ast.call_method1("parse", (&sources[4].text, &sources[4].path))?;
    repair.set_item("__doc__", ast.call_method1("get_docstring", (tree,))?)?;
    let options = PyDict::new(py);
    options.set_item("return_value", py.None())?;
    repair.set_item(
        "setup_logging",
        py.import("unittest.mock")?
            .getattr("Mock")?
            .call((), Some(&options))?,
    )?;
    let repair = crate::source_oracle_runtime::module(
        py,
        sources[4],
        &["_print_survey", "_suspects", "report", "migrate", "main"],
        &[
            "print",
            "len",
            "sum",
            "sorted",
            "set",
            "str",
            "FileNotFoundError",
            "RuntimeError",
            "OSError",
            "ValueError",
        ],
        &repair,
        "_original_identity_repair",
    )?;
    let rejecting = PyDict::new(py);
    rejecting.set_item(
        "side_effect",
        media_error.call1(("unexpected original resume content lookup",))?,
    )?;
    let reject = py.import("unittest.mock")?.getattr("Mock")?;
    for name in ["same_image", "collision_key", "content_key"] {
        identity.setattr(name, reject.call((), Some(&rejecting))?)?;
    }
    let near = crate::source_oracle_runtime::namespace(
        py,
        sources[18],
        &["near_indices"],
        &["enumerate"],
        &PyDict::new(py),
    )?;
    let ingest = PyDict::new(py);
    ingest.set_item("identity", &identity)?;
    ingest.set_item("Path", &path)?;
    ingest.set_item("MediaError", &media_error)?;
    ingest.set_item("near_indices", required(&near, "near_indices")?)?;
    let ingest = crate::source_oracle_runtime::namespace(
        py,
        sources[17],
        &["catalog_identity", "equivalent_files"],
        &["set", "enumerate", "len", "OSError"],
        &ingest,
    )?;
    let ports = PyDict::new(py);
    ports.set_item("writer", ownership.getattr("writer")?)?;
    ports.set_item("require_native", reject.call0()?)?;
    ports.set_item("catalog_identity", required(&ingest, "catalog_identity")?)?;
    let catalog = crate::source_catalog_oracle::namespace_with_ports(
        py, sources[7], sources[9], workspace, &ports,
    )?;
    let decoder = PyDict::new(py);
    decoder.set_item("Path", &path)?;
    let decoder = crate::source_oracle_runtime::module(
        py,
        sources[15],
        &["_decode"],
        &[],
        &decoder,
        "_original_identity_decoder",
    )?;
    let protocol = PyDict::new(py);
    protocol.set_item("Enum", py.import("enum")?.getattr("Enum")?)?;
    let protocol = crate::source_oracle_runtime::namespace(
        py,
        sources[19],
        &["SetState", "LiveStateUnknown"],
        &["__build_class__", "RuntimeError"],
        &protocol,
    )?;
    let reconcile = PyDict::new(py);
    reconcile.set_item("Path", &path)?;
    reconcile.set_item("identity", &identity)?;
    reconcile.set_item("Catalog", required(&catalog, "Catalog")?)?;
    reconcile.set_item("SetDrift", collection.getattr("SetDrift")?)?;
    reconcile.set_item("_state_path", collection.getattr("_state_path")?)?;
    for name in ["SetState", "LiveStateUnknown"] {
        reconcile.set_item(name, required(&protocol, name)?)?;
    }
    for name in ["_private_download", "_near_catalog_match"] {
        reconcile.set_item(name, reject.call((), Some(&rejecting))?)?;
    }
    let media = ns.call0()?;
    media.setattr(
        "telegram_sticker_format",
        reject.call((), Some(&rejecting))?,
    )?;
    reconcile.set_item("media", media)?;
    reconcile.set_item("AMBIGUOUS", "ambiguous")?;
    reconcile.set_item("UNDECIDABLE", "undecidable")?;
    reconcile.set_item("redact", py.import("builtins")?.getattr("str")?)?;
    reconcile.set_item(
        "log",
        py.import("logging")?
            .call_method1("getLogger", ("original_identity_resume",))?,
    )?;
    let reconcile = crate::source_oracle_runtime::module(
        py,
        sources[16],
        &[
            "_probe",
            "_manifest_mismatch",
            "Unresolvable",
            "_resolve_sticker_key",
            "_identity",
            "reconcile_set",
        ],
        &[
            "__build_class__",
            "Exception",
            "getattr",
            "len",
            "str",
            "enumerate",
            "hasattr",
        ],
        &reconcile,
        "_original_identity_resume",
    )?;
    let imports = Py::new(
        py,
        Imports {
            collection: collection.clone().into_any().unbind(),
            bundle: bundle.clone().into_any().unbind(),
            reconcile: reconcile.into_any().unbind(),
            decoder: decoder.into_any().unbind(),
        },
    )?;
    for module in [&migration, &repair] {
        module
            .dict()
            .get_item("__builtins__")?
            .ok_or_else(|| PyValueError::new_err("missing original identity builtins"))?
            .set_item("__import__", imports.bind(py))?;
    }
    Ok(Modules {
        migration,
        repair,
        state,
        bundle,
        collection,
        catalog: required(&catalog, "Catalog")?,
        identity,
        imports,
    })
}
