//! Original version2 bundle operations over one fixed public clip and caller-owned replay dataset.
use super::modules;
use crate::source_git_inventory::Source;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use std::io::Read;
use std::path::{Path, PathBuf};

const OLD: &str = "v:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const KEY: &str = "v:3cf3deb8098c04f13c4f989370acac89";
const HASH: u64 = 5300177295401782857;
const DIGEST: &str = "9bbe7eb2e975eb1f325a733fba5e5f97918e3423bb19e5e10d25942aff78ef57";
const SOURCE: &str = "001_video_aaaaaaaaaaaa.webm";
const DEST: &str = "001_video_3cf3deb8098c.webm";

fn state(key: &str) -> Value {
    json!({"sets":[{"keys":[key]}],"skipped":[]})
}
fn sibling(value: &Value, data: &Path, name: &str) -> bool {
    let Some(path) = value
        .as_str()
        .map(Path::new)
        .filter(|path| path.is_absolute())
    else {
        return false;
    };
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|actual| {
            // canonical() folds Windows names, including the backup timestamp's T/Z.
            // Match the host's filename identity while retaining the exact sibling binding.
            if cfg!(windows) {
                actual.eq_ignore_ascii_case(name)
            } else {
                actual == name
            }
        })
        && path
            .parent()
            .and_then(|parent| crate::migration_bundle::canonical(parent).ok())
            .as_deref()
            == Some(data)
        && crate::tooling_environment::reject_links(path, "bundle intent cannot follow links")
            .is_ok()
}
fn document(path: &Path) -> Result<Value, String> {
    crate::tooling_environment::reject_links(path, "bundle replay input cannot follow links")?;
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| "cannot inspect bundle replay document")?;
    if !metadata.is_file() || metadata.len() > 65536 {
        return Err("bundle replay document must be regular and under64KiB".into());
    }
    let file = std::fs::File::open(path).map_err(|_| "cannot open bundle replay document")?;
    if !file
        .metadata()
        .map_err(|_| "cannot inspect bundle replay document")?
        .is_file()
    {
        return Err("bundle replay document must be regular".into());
    }
    let mut bytes = Vec::new();
    file.take(65537)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read bundle replay document")?;
    if bytes.len() > 65536 {
        return Err("bundle replay document exceeds64KiB".into());
    }
    serde_json::from_slice(&bytes).map_err(|_| "invalid bundle replay document".into())
}

pub(super) fn admit(root: &Path, data: &Path, manifest: Option<&Path>) -> Result<PathBuf, String> {
    crate::tooling_environment::physical_root(data)?;
    let data = crate::migration_bundle::canonical(data)?;
    let root = crate::migration_bundle::canonical(root)?;
    let parent = data.parent().ok_or("bundle replay dataset has no parent")?;
    if !parent.starts_with(root.join("logs/tmp"))
        || parent.parent() != Some(root.join("logs/tmp").as_path())
        || !parent
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.starts_with("original bundle replay "))
        || data.file_name().and_then(|s| s.to_str()) != Some("collection")
    {
        return Err("bundle replay requires its exclusive public-fixture dataset".into());
    }
    for name in [SOURCE, DEST] {
        let path = data.join(name);
        if path.exists() {
            crate::tooling_environment::reject_links(
                &path,
                "bundle replay clip cannot follow links",
            )?;
            let metadata = std::fs::symlink_metadata(&path)
                .map_err(|_| "cannot inspect bundle replay clip")?;
            if !metadata.is_file()
                || metadata.len() != 1570
                || crate::tooling_manifest::digest(&path)? != DIGEST
            {
                return Err("bundle replay clip identity drift".into());
            }
        }
    }
    if !data.join(SOURCE).is_file() && !data.join(DEST).is_file() {
        return Err("bundle replay clip is missing".into());
    }
    crate::tooling_environment::reject_links(
        &data.join("catalog.db"),
        "bundle replay catalog cannot follow links",
    )?;
    let catalog = data.join("catalog.db");
    let metadata =
        std::fs::metadata(&catalog).map_err(|_| "cannot inspect bundle replay catalog")?;
    if !metadata.is_file() || metadata.len() > 65536 {
        return Err("bundle replay catalog must be a regular file under64KiB".into());
    }
    let db =
        rusqlite::Connection::open_with_flags(&catalog, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|_| "cannot inspect bundle replay catalog")?;
    let mut query = db
        .prepare("SELECT name,type FROM sqlite_master WHERE name NOT LIKE 'sqlite_%' ORDER BY name")
        .map_err(|_| "cannot inspect bundle replay schema")?;
    let objects = query
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|_| "cannot inspect bundle replay schema")?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "cannot read bundle replay schema")?;
    if objects
        != ["items", "publications", "seen_files"].map(|name| (name.to_owned(), "table".to_owned()))
    {
        return Err("bundle replay requires exactly its three fixture tables".into());
    }
    drop(query);
    let row: (String, String, String, i64) = db
        .query_row(
            "SELECT content_key,file_path,format,phash FROM items",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .map_err(|_| "bundle replay catalog shape drift")?;
    if ![OLD, KEY].contains(&row.0.as_str())
        || row.2 != "video"
        || row.3 != HASH as i64
        || ![format!("collection/{SOURCE}"), format!("./{DEST}")].contains(&row.1)
    {
        return Err("bundle replay catalog is not the fixed public fixture".into());
    }
    for (table, column, id) in [
        ("publications", "custom_emoji_id", "111111111"),
        ("seen_files", "file_unique_id", "fixture-unique"),
    ] {
        let values: (String, String) = db
            .query_row(
                &format!("SELECT content_key,{column} FROM {table}"),
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .map_err(|_| "bundle replay identifier shape drift")?;
        let count: i64 = db
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .map_err(|_| "cannot count bundle replay identifiers")?;
        if count != 1 || values != (row.0.clone(), id.into()) {
            return Err("bundle replay identifiers are not the fixed fixture".into());
        }
    }
    let count: i64 = db
        .query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0))
        .map_err(|_| "cannot count bundle replay items")?;
    if count != 1 {
        return Err("bundle replay requires exactly one item".into());
    }
    drop(db);
    let actual = document(&data.join("publish_fixture.json"))?;
    if actual != state(OLD) && actual != state(KEY) {
        return Err("bundle replay state is not the fixed fixture".into());
    }
    // Original discovery would otherwise adopt any additional publisher document in this folder.
    let mut members = 0;
    for entry in std::fs::read_dir(&data).map_err(|_| "cannot inspect bundle replay members")? {
        members += 1;
        if members > 64 {
            return Err("bundle replay member limit exceeded".into());
        }
        let entry = entry.map_err(|_| "cannot inspect bundle replay member")?;
        let path = entry.path();
        crate::tooling_environment::reject_links(
            &path,
            "bundle replay members cannot follow links",
        )?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "bundle replay name is not UTF-8")?;
        if name == "tmp" {
            let cache = path.join("identity-codec");
            crate::tooling_environment::reject_links(
                &cache,
                "bundle replay cache cannot follow links",
            )?;
            if !cache.is_dir()
                || crate::process_owner::cleanup_pending(&cache)
                || std::fs::read_dir(&path)
                    .map_err(|_| "cannot inspect bundle replay cache")?
                    .count()
                    != 1
                || std::fs::read_dir(&cache)
                    .map_err(|_| "cannot inspect bundle replay codec scratch")?
                    .next()
                    .is_some()
            {
                return Err("bundle replay codec scratch is not settled".into());
            }
        } else if !entry
            .file_type()
            .map_err(|_| "cannot inspect bundle replay member type")?
            .is_file()
            || !(matches!(
                name.as_str(),
                SOURCE
                    | DEST
                    | "catalog.db"
                    | "publish_fixture.json"
                    | "identity-migration.journal.json"
                    | ".maintenance.lock"
                    | "publish_fixture.lock"
                    | "other.rollback.json"
            ) || name.starts_with("catalog.before-video-identity-")
                && (name.ends_with(".db") || name.ends_with(".rollback.json")))
        {
            return Err("unexpected bundle replay member; no source operation started".into());
        }
    }
    if let Some(manifest) = manifest {
        crate::tooling_environment::reject_links(
            manifest,
            "bundle replay manifest cannot follow links",
        )?;
        let manifest = crate::migration_bundle::canonical(manifest)?;
        if manifest.parent() != Some(data.as_path()) {
            return Err("bundle replay manifest escaped dataset".into());
        }
        let doc = document(&manifest)?;
        let backup = PathBuf::from(
            doc["backup"]
                .as_str()
                .ok_or("bundle replay backup missing")?,
        );
        if backup
            .parent()
            .and_then(|parent| crate::migration_bundle::canonical(parent).ok())
            .as_deref()
            != Some(data.as_path())
            || crate::migration_bundle::canonical(&backup.with_extension("rollback.json"))?
                != manifest
            || !sibling(
                &doc["bundle"],
                &data,
                manifest
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or("bundle replay manifest name missing")?,
            )
            || doc["version"] != 2
            || !doc["data_dir"].as_str().is_some_and(|path| {
                crate::migration_bundle::canonical(Path::new(path))
                    .ok()
                    .as_deref()
                    == Some(data.as_path())
            })
            || !sibling(&doc["catalog"], &data, "catalog.db")
            || doc["key_map"] != json!({OLD:KEY})
            || doc["phash"] != json!({KEY:HASH as i64})
            || doc["states"]
                != json!({"publish_fixture.json":{"before":state(OLD),"after":state(KEY)}})
        {
            return Err("bundle replay manifest binding drift".into());
        }
        crate::tooling_environment::reject_links(
            &backup,
            "bundle replay backup cannot follow links",
        )?;
        let metadata = std::fs::symlink_metadata(&backup)
            .map_err(|_| "cannot inspect bundle replay backup")?;
        if !metadata.is_file() || metadata.len() > 65536 {
            return Err("bundle replay backup must be regular and under64KiB".into());
        }
        if doc["backup_sha256"].as_str() != Some(crate::tooling_manifest::digest(&backup)?.as_str())
        {
            return Err("bundle replay backup hash drift".into());
        }
        let files = doc["files"]
            .as_array()
            .filter(|files| files.len() == 1)
            .ok_or("bundle replay file inventory drift")?;
        if !sibling(&files[0]["source"], &data, SOURCE)
            || !sibling(&files[0]["destination"], &data, DEST)
            || files[0]["sha256"] != DIGEST
            || files[0]["key"] != KEY
            || ![json!(OLD), json!(KEY)].contains(&files[0]["old_key"])
            || files[0]["stored_source"] != format!("collection/{SOURCE}")
            || files[0]["format"] != "video"
            || files[0]["filename_key"] != OLD
            || files[0]["destination_existed"] != false
        {
            return Err("bundle replay media intents drift".into());
        }
        let journal = data.join("identity-migration.journal.json");
        if journal.exists() {
            let mut active = document(&journal)?;
            let mut expected = doc.clone();
            for field in ["stage", "direction"] {
                active
                    .as_object_mut()
                    .ok_or("bundle replay journal must be an object")?
                    .remove(field);
                expected
                    .as_object_mut()
                    .ok_or("bundle replay manifest must be an object")?
                    .remove(field);
            }
            if active != expected {
                return Err("bundle replay journal binding drift".into());
            }
        }
    } else if data.join("identity-migration.journal.json").exists() {
        return Err("bundle preparation requires no active journal".into());
    }
    Ok(data)
}

#[pyclass]
struct FrozenClip(PathBuf);
#[pymethods]
impl FrozenClip {
    fn __call__(&self, path: PathBuf, format: &str) -> PyResult<(String, u64)> {
        if format != "video"
            || !crate::migration_bundle::canonical(&path)
                .ok()
                .is_some_and(|path| [self.0.join(SOURCE), self.0.join(DEST)].contains(&path))
            || crate::tooling_manifest::digest(&path).ok().as_deref() != Some(DIGEST)
        {
            return Err(PyValueError::new_err("original bundle decoder input drift"));
        }
        Ok((KEY.into(), HASH))
    }
}
fn python<'py>(py: Python<'py>, value: &Value) -> PyResult<Bound<'py, PyAny>> {
    py.import("json")?
        .call_method1("loads", (value.to_string(),))
}
fn value(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<Value> {
    let text: String = py
        .import("json")?
        .call_method1("dumps", (value,))?
        .extract()?;
    serde_json::from_str(&text)
        .map_err(|_| PyValueError::new_err("original bundle result is not JSON"))
}
fn write(py: Python<'_>, modules: &modules::Modules<'_>, path: &Path, doc: &Value) -> PyResult<()> {
    modules
        .state
        .call_method1("write_json_atomic", (path, python(py, doc)?))?;
    Ok(())
}

pub(super) fn run(
    py: Python<'_>,
    data: &Path,
    manifest: Option<&Path>,
    action: &str,
    sources: &[&Source],
) -> PyResult<Value> {
    let modules = modules::build(
        py,
        data.parent()
            .ok_or_else(|| PyValueError::new_err("dataset parent missing"))?,
        sources,
    )?;
    let saved = modules.identity.getattr("fingerprint")?;
    modules
        .identity
        .setattr("fingerprint", Py::new(py, FrozenClip(data.into()))?)?;
    let result = (|| -> PyResult<Value> {
        let manifest = manifest.map(Path::to_path_buf);
        if action == "prepare" {
            let backup: PathBuf = modules
                .migration
                .call_method1("backup_catalog", (data.join("catalog.db"),))?
                .extract()?;
            let mapping = python(py, &json!({OLD:KEY}))?;
            let files = modules
                .bundle
                .call_method1("plan_files", (data.join("catalog.db"), &mapping))?;
            let states = python(
                py,
                &json!({"publish_fixture.json":{"before":state(OLD),"after":state(KEY)}}),
            )?;
            let doc = modules.bundle.call_method1(
                "make_bundle",
                (
                    data,
                    &backup,
                    mapping,
                    python(py, &json!({KEY:HASH as i64}))?,
                    files,
                    states,
                ),
            )?;
            let manifest = backup.with_extension("rollback.json");
            doc.set_item("bundle", manifest.to_string_lossy().as_ref())?;
            let doc = value(py, &doc)?;
            write(py, &modules, &manifest, &doc)?;
            write(
                py,
                &modules,
                &data.join("identity-migration.journal.json"),
                &doc,
            )?;
            return Ok(json!({"manifest":manifest}));
        }
        let manifest = manifest
            .ok_or_else(|| PyValueError::new_err("original bundle action requires manifest"))?;
        let mut doc = document(&manifest).map_err(PyValueError::new_err)?;
        let restore = || -> PyResult<()> {
            let options = PyDict::new(py);
            options.set_item("apply", true)?;
            modules.migration.call_method(
                "restore_migration",
                (data, &manifest),
                Some(&options),
            )?;
            Ok(())
        };
        match action {
            "verify" => {
                let doc = python(py, &doc)?;
                modules
                    .bundle
                    .call_method1("verify_applied", (data, &doc))?;
                // The original comparison undoes only the migration's path rewrite, not SQL edits.
                let signature = modules
                    .bundle
                    .call_method1("verify_current", (data, &doc))?
                    .extract::<String>()?;
                return Ok(json!({"action":action,"signature":signature}));
            }
            "restore" => restore()?,
            "stage" | "finish-native" => {
                if action == "stage" {
                    modules.migration.call_method1(
                        "_apply_database",
                        (
                            data.join("catalog.db"),
                            python(py, &doc["key_map"])?,
                            python(py, &doc["phash"])?,
                        ),
                    )?;
                    modules
                        .bundle
                        .call_method1("move_file", (data.join(SOURCE), data.join(DEST), DIGEST))?;
                    let expected = modules.migration.call_method1("apply_migration", (data,))?;
                    let expected = value(py, &expected)?;
                    restore()?;
                    write(py, &modules, &manifest, &doc)?;
                    write(
                        py,
                        &modules,
                        &data.join("identity-migration.journal.json"),
                        &doc,
                    )?;
                    modules.migration.call_method1(
                        "_apply_database",
                        (
                            data.join("catalog.db"),
                            python(py, &doc["key_map"])?,
                            python(py, &doc["phash"])?,
                        ),
                    )?;
                    modules
                        .bundle
                        .call_method1("move_file", (data.join(SOURCE), data.join(DEST), DIGEST))?;
                    return Ok(json!({"expected":expected}));
                }
                doc["direction"] = json!("forward");
                doc["stage"] = json!("backup");
                write(py, &modules, &manifest, &doc)?;
                write(
                    py,
                    &modules,
                    &data.join("identity-migration.journal.json"),
                    &doc,
                )?;
                modules.migration.call_method1(
                    "_apply_database",
                    (
                        data.join("catalog.db"),
                        python(py, &doc["key_map"])?,
                        python(py, &doc["phash"])?,
                    ),
                )?;
                modules.migration.call_method1("apply_migration", (data,))?;
                let applied = document(&manifest).map_err(PyValueError::new_err)?;
                modules
                    .bundle
                    .call_method1("verify_applied", (data, python(py, &applied)?))?;
                if data.join("identity-migration.journal.json").exists() {
                    return Err(PyValueError::new_err("original bundle journal not retired"));
                }
                restore()?;
            }
            "stage-restore" => {
                doc["direction"] = json!("restore");
                doc["stage"] = json!("restore");
                write(
                    py,
                    &modules,
                    &data.join("identity-migration.journal.json"),
                    &doc,
                )?;
                modules
                    .bundle
                    .call_method1("move_file", (data.join(DEST), data.join(SOURCE), DIGEST))?;
                write(
                    py,
                    &modules,
                    &data.join("publish_fixture.json"),
                    &state(OLD),
                )?;
            }
            _ => return Err(PyValueError::new_err("unknown original bundle action")),
        }
        let signature = modules
            .bundle
            .call_method1("signature", (data.join("catalog.db"),))?
            .extract::<String>()?;
        Ok(json!({"action":action,"signature":signature}))
    })();
    let restored = modules.identity.setattr("fingerprint", &saved);
    restored?;
    if !modules.identity.getattr("fingerprint")?.is(&saved) {
        return Err(PyValueError::new_err(
            "original bundle fingerprint not restored",
        ));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundle_sibling_keeps_timestamp_case_semantics_parent_and_link_bindings() {
        let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let parent =
            crate::tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
        let workspace = parent.join(format!("bundle sibling boundary {}", std::process::id()));
        std::fs::create_dir(&workspace).unwrap();
        let data = workspace.join("collection");
        std::fs::create_dir(&data).unwrap();
        let data = crate::migration_bundle::canonical(&data).unwrap();
        let name = "catalog.before-video-identity-20261110T212800Z.rollback.json";
        let manifest = data.join(name);
        std::fs::write(&manifest, b"{}").unwrap();
        assert!(sibling(&json!(manifest), &data, name));
        assert_eq!(
            sibling(&json!(manifest), &data, &name.to_ascii_lowercase()),
            cfg!(windows)
        );
        assert!(!sibling(&json!(manifest), &data, "other.rollback.json"));
        assert!(!sibling(&json!(manifest), &workspace, name));
        #[cfg(unix)]
        {
            let link = data.join("other.rollback.json");
            std::os::unix::fs::symlink(&manifest, &link).unwrap();
            assert!(!sibling(&json!(link), &data, "other.rollback.json"));
            std::fs::remove_file(link).unwrap();
        }
        std::fs::remove_file(manifest).unwrap();
        std::fs::remove_dir(data).unwrap();
        std::fs::remove_dir(workspace).unwrap();
    }
}
