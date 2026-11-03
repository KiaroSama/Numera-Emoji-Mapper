//! Fixed historical SQLite leaves generate signatures and rewrites from isolated in-memory fixtures.
use crate::source_git_inventory::Source;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const BASELINE: &str = "1a36a968b006236bd0665a1da65c541938c06061";
const PATH: &str = "tests/oracles/migration_bundle.py";
const HASH: &str = "8b2648956047e95fe5fcc1e779e85041366de917f1290faeb7b1d6c0e6e040c8";
const SIGNATURE: &str = "d173116:migration_bundle.database_signature";
const DATABASE: &str = "d173116:migration_bundle.rewrite_database";
const SIGNATURE_RECIPE: &str = "5b1e7c00a29800c1ae517459e236198bdedced14837ac9102014683736b9ea40";
const DATABASE_RECIPE: &str = "6266b64288b117e462a33fdf9fa975ad85999e678a4b387c5a1551aaba944b91";
const SIGNATURE_SCHEMA: &str = concat!(
    "CREATE TABLE \"z weird\"(text TEXT,n INTEGER,nullable TEXT,b BLOB);",
    "CREATE TABLE items(content_key TEXT,file_path TEXT,phash INTEGER);",
    "CREATE TABLE aa(note TEXT);INSERT INTO aa VALUES(\"first table\");",
);
const DATABASE_SCHEMA: &str = concat!(
    "CREATE TABLE items(content_key TEXT PRIMARY KEY,phash INTEGER,label TEXT);",
    "CREATE TABLE publications(content_key TEXT,custom_emoji_id TEXT);",
    "CREATE TABLE seen_files(content_key TEXT,file_unique_id TEXT);",
    "INSERT INTO items VALUES('v:a',123,'alpha'),('v:b',456,'beta');",
    "INSERT INTO publications VALUES('v:a','111111111'),('v:b','222222222');",
    "INSERT INTO seen_files VALUES('v:a','unique-a'),('v:b','unique-b');",
);

pub fn source_identity(source: &Source) -> Result<(), String> {
    let actual = Sha256::digest(source.text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    if source.baseline_revision != BASELINE
        || source.path != PATH
        || source.sha256 != HASH
        || actual != HASH
    {
        return Err("SQLite oracle source identity drift; refusing execution".into());
    }
    Ok(())
}

pub fn fixture_signature_identity(fixture: &Value) -> Result<(), String> {
    crate::source_fixture::recipe_with_observations(
        fixture,
        SIGNATURE,
        2,
        SIGNATURE_RECIPE,
        &["repr_order"],
    )
}

pub fn fixture_database_identity(fixture: &Value) -> Result<(), String> {
    crate::source_fixture::recipe(fixture, DATABASE, 2, DATABASE_RECIPE)
}

struct Connection<'py> {
    value: Bound<'py, PyAny>,
    closed: bool,
}
impl<'py> Connection<'py> {
    fn open(py: Python<'py>) -> PyResult<Self> {
        Ok(Self {
            value: py
                .import("sqlite3")?
                .call_method1("connect", (":memory:",))?,
            closed: false,
        })
    }
    fn close(&mut self) -> PyResult<()> {
        if !self.closed {
            self.value.call_method0("close")?;
            self.closed = true;
        }
        Ok(())
    }
}
impl Drop for Connection<'_> {
    fn drop(&mut self) {
        if self.close().is_err() {
            eprintln!("ERROR: historical SQLite oracle connection could not be closed");
        }
    }
}

fn in_memory<'py, T>(
    py: Python<'py>,
    observe: impl FnOnce(&Bound<'py, PyAny>) -> PyResult<T>,
) -> PyResult<T> {
    let mut connection = Connection::open(py)?;
    let result = observe(&connection.value);
    connection.close()?;
    result
}

fn decode<'py>(py: Python<'py>, value: &Value) -> PyResult<Bound<'py, PyAny>> {
    // Preserve serde_json's exact integer token; never pass signed hashes through f64.
    let text = serde_json::to_string(value)
        .map_err(|_| pyo3::exceptions::PyValueError::new_err("cannot encode oracle input"))?;
    py.import("json")?.call_method1("loads", (text,))
}

fn encode(py: Python<'_>, value: &Bound<'_, PyAny>) -> PyResult<Value> {
    let text: String = py
        .import("json")?
        .call_method1("dumps", (value,))?
        .extract()?;
    serde_json::from_str(&text)
        .map_err(|_| pyo3::exceptions::PyValueError::new_err("invalid source SQLite result"))
}

fn signature_observation(py: Python<'_>, source: &Source, case: &Value) -> PyResult<Value> {
    let bindings = PyDict::new(py);
    bindings.set_item("hashlib", py.import("hashlib")?)?;
    bindings.set_item("json", py.import("json")?)?;
    let globals = crate::source_oracle_runtime::namespace(
        py,
        source,
        &["_quote", "database_signature"],
        &["list", "sorted", "repr"],
        &bindings,
    )?;
    let function = globals.get_item("database_signature")?.ok_or_else(|| {
        pyo3::exceptions::PyValueError::new_err("missing source signature function")
    })?;
    let builtins = py.import("builtins")?;
    let rows = decode(py, &case["rows"])?.cast_into::<PyList>()?;
    let values = PyList::empty(py);
    let representations = PyList::empty(py);
    for row in rows.iter() {
        let converted = PyList::empty(py);
        for value in row.cast_into::<PyList>()?.iter() {
            if let Ok(blob) = value.cast::<PyDict>() {
                let hex = blob.get_item("blob")?.ok_or_else(|| {
                    pyo3::exceptions::PyValueError::new_err("missing oracle blob")
                })?;
                converted.append(builtins.getattr("bytes")?.call_method1("fromhex", (hex,))?)?;
            } else {
                converted.append(value)?;
            }
        }
        representations.append(builtins.getattr("repr")?.call1((&converted,))?)?;
        values.append(converted)?;
    }
    let repr_order = builtins.getattr("sorted")?.call1((representations,))?;
    in_memory(py, |database| {
        database.call_method1("executescript", (SIGNATURE_SCHEMA,))?;
        database.call_method1(
            "executemany",
            ("INSERT INTO \"z weird\" VALUES(?,?,?,?)", values),
        )?;
        database.call_method1(
            "execute",
            (
                "INSERT INTO items VALUES(?,?,?)",
                ("s:fixture", "./after.png", -1),
            ),
        )?;
        let digest = function.call1((database, decode(py, &case["files"])?))?;
        let observation = PyDict::new(py);
        observation.set_item("expected", digest)?;
        observation.set_item("repr_order", repr_order)?;
        encode(py, observation.as_any())
    })
}

fn database_observation(py: Python<'_>, source: &Source, case: &Value) -> PyResult<Value> {
    let bindings = PyDict::new(py);
    bindings.set_item(
        "REFERENCES",
        PyTuple::new(
            py,
            [
                ("items", "content_key"),
                ("publications", "content_key"),
                ("seen_files", "content_key"),
            ],
        )?,
    )?;
    let globals = crate::source_oracle_runtime::namespace(
        py,
        source,
        &["rewrite_database"],
        &["enumerate", "RuntimeError"],
        &bindings,
    )?;
    let function = globals.get_item("rewrite_database")?.ok_or_else(|| {
        pyo3::exceptions::PyValueError::new_err("missing source database function")
    })?;
    let hashes = case["hashes"]
        .as_object()
        .ok_or_else(|| pyo3::exceptions::PyValueError::new_err("invalid oracle hashes"))?;
    let signed = PyDict::new(py);
    for (key, value) in hashes {
        if value.is_null() {
            signed.set_item(key, py.None())?;
        } else {
            let signed_value = if let Some(value) = value.as_u64() {
                if value <= i64::MAX as u64 {
                    value as i64
                } else {
                    (i128::from(value) - (1_i128 << 64)) as i64
                }
            } else {
                value.as_i64().ok_or_else(|| {
                    pyo3::exceptions::PyValueError::new_err("oracle hash is not a 64-bit integer")
                })?
            };
            signed.set_item(key, signed_value)?;
        }
    }
    in_memory(py, |database| {
        database.call_method1("executescript", (DATABASE_SCHEMA,))?;
        let moved = function.call1((database, decode(py, &case["map"])?, signed))?;
        let observed = PyDict::new(py);
        observed.set_item("moved", moved)?;
        for (name, query) in [
            ("items", "SELECT * FROM items ORDER BY content_key"),
            (
                "publications",
                "SELECT * FROM publications ORDER BY content_key",
            ),
            (
                "seen_files",
                "SELECT * FROM seen_files ORDER BY content_key",
            ),
        ] {
            observed.set_item(
                name,
                database
                    .call_method1("execute", (query,))?
                    .call_method0("fetchall")?,
            )?;
        }
        encode(py, observed.as_any())
    })
}

pub fn check_signature(source: &Source, fixture: &Value) -> Result<Value, String> {
    source_identity(source)?;
    fixture_signature_identity(fixture)?;
    let cases = crate::source_fixture::cases(fixture, SIGNATURE, 2)?;
    Python::initialize();
    Python::attach(|py| -> Result<Value, String> {
        crate::source_oracle_runtime::require_runtime(py)?;
        for case in cases {
            let observed = signature_observation(py, source, case).map_err(|error| {
                format!("signature oracle source failed: {}: {error}", case["name"])
            })?;
            if case.get("error").is_some()
                || observed["expected"] != case["expected"]
                || observed["repr_order"] != case["repr_order"]
            {
                return Err(format!("signature fixture drift: {}", case["name"]));
            }
        }
        Ok(
            json!({"cases_checked":cases.len(),"source_runtime":"CPython3.11.9",
            "source_hash":HASH,"fixture_drift":false}),
        )
    })
}

pub fn check_database(source: &Source, fixture: &Value) -> Result<Value, String> {
    source_identity(source)?;
    fixture_database_identity(fixture)?;
    let cases = crate::source_fixture::cases(fixture, DATABASE, 2)?;
    Python::initialize();
    Python::attach(|py| -> Result<Value, String> {
        crate::source_oracle_runtime::require_runtime(py)?;
        for case in cases {
            let observed = database_observation(py, source, case).map_err(|error| {
                format!("database oracle source failed: {}: {error}", case["name"])
            })?;
            if case.get("error").is_some() || observed != case["expected"] {
                return Err(format!("database fixture drift: {}", case["name"]));
            }
        }
        Ok(
            json!({"cases_checked":cases.len(),"source_runtime":"CPython3.11.9",
            "source_hash":HASH,"fixture_drift":false}),
        )
    })
}
