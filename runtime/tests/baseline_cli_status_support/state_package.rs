use crate::support::*;
use numera_runtime as native;
use rusqlite::{Connection, OpenFlags};
use serde_json::{Value, json};
use std::{io::Read, path::Path};
// tests.test_native_state_compat.NativeStateCompatibility.test_exported_state_lease_callback_and_atomic_bytes
#[test]
fn state_native_lease_raii_atomic_utf8_and_typed_validation() {
    let root = Scratch::new();
    let path = root.0.join("family.lock");
    let raw = {
        let lease = native::locks::Lease::acquire(&path, UTC).unwrap();
        let descriptor = lease.clone_file().unwrap();
        #[cfg(unix)]
        let raw = {
            use std::os::fd::AsRawFd;
            descriptor.as_raw_fd()
        };
        #[cfg(windows)]
        let raw = {
            use std::os::windows::io::AsRawHandle;
            descriptor.as_raw_handle() as usize
        };
        assert!(descriptor.metadata().is_ok());
        let record = read_json(&path);
        assert_eq!(record["pid"], std::process::id());
        assert!(record.get("started").is_some());
        lease.heartbeat().unwrap();
        assert!(native::locks::Lease::acquire(&path, UTC).is_err());
        raw
    };
    closed_descriptor(raw);
    assert!(path.exists());
    {
        let _other = native::locks::Lease::acquire(&path, UTC).unwrap();
        assert!(native::locks::Lease::acquire(&path, UTC).is_err());
    }
    let failure = std::panic::catch_unwind(|| {
        let _lease = native::locks::Lease::acquire(&path, UTC).unwrap();
        panic!("callback failed");
    });
    assert!(failure.is_err());
    drop(native::locks::Lease::acquire(&path, UTC).unwrap());
    let native_path = root.0.join("native.json");
    #[derive(serde::Serialize)]
    struct Input<'a> {
        label: &'a str,
        real: f64,
        keys: [&'a str; 2],
    }
    let value = Input {
        label: "é 😀",
        real: f64::INFINITY,
        keys: ["a", "b"],
    };
    native::atomic::write_json(&native_path, &value).unwrap();
    assert_eq!(
        read_json(&native_path),
        json!({"label":"é 😀","real":null,"keys":["a","b"]})
    );
    assert_eq!(
        std::fs::read(&native_path).unwrap(),
        "{\n \"label\": \"é 😀\",\n \"real\": null,\n \"keys\": [\n  \"a\",\n  \"b\"\n ]\n}"
            .as_bytes()
    );
    let before = std::fs::read(&native_path).unwrap();
    struct Bad;
    impl serde::Serialize for Bad {
        fn serialize<S: serde::Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
            Err(serde::ser::Error::custom("unsupported object"))
        }
    }
    assert!(
        native::atomic::write_json(
            &native_path,
            &std::collections::BTreeMap::from([("bad", Bad)])
        )
        .is_err()
    );
    assert_eq!(std::fs::read(&native_path).unwrap(), before);
    let blocked = root.0.join("blocked.json");
    write(&blocked.join("keep"), b"retained");
    assert!(native::atomic::write_json(&blocked, &json!({"new":true})).is_err());
    assert_eq!(std::fs::read(blocked.join("keep")).unwrap(), b"retained");
    assert!(
        !std::fs::read_dir(&root.0).unwrap().any(|e| e
            .unwrap()
            .path()
            .extension()
            .is_some_and(|s| s == "tmp"))
    );
    let valid =
        json!({"base":"fixture","done":[],"sets":[{"name":"fixture1","index":1,"count":1}]});
    let mut bool_state = valid.clone();
    bool_state["sets"][0]["index"] = json!(true);
    bool_state["sets"][0]["count"] = json!(false);
    assert!(native::ticker_state::validate(&valid, "fixture", 200).is_ok());
    assert!(native::ticker_state::validate(&bool_state, "fixture", 200).is_err());
    for mutation in ["done", "zero-index", "replace"] {
        let mut state = valid.clone();
        match mutation {
            "done" => state["done"] = Value::Null,
            "zero-index" => state["sets"][0]["index"] = json!(0),
            _ => state["in_flight"] = json!({"key":"a","operation":"replace","set_name":"fixture"}),
        };
        assert!(
            native::ticker_state::validate(&state, "fixture", 200).is_err(),
            "{mutation}"
        );
    }
    let mut intent = native::ticker_state::intent("a", "fixture", 1, 1);
    assert_eq!(intent["operation"], "add");
    let started = intent["started_utc"].as_str().unwrap();
    assert!(started.ends_with('Z'));
    assert_eq!(
        chrono::DateTime::parse_from_rfc3339(started)
            .unwrap()
            .offset()
            .local_minus_utc(),
        0
    );
    intent["operation"] = json!("replace");
    let mut replaced_state = valid;
    replaced_state["in_flight"] = intent;
    assert!(native::ticker_state::validate(&replaced_state, "fixture", 200).is_err());
}
#[cfg(unix)]
fn closed_descriptor(raw: i32) {
    // SAFETY: query only the observed descriptor after its owned File was dropped.
    assert_eq!(unsafe { libc::fcntl(raw, libc::F_GETFD) }, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::EBADF)
    );
}
#[cfg(windows)]
fn closed_descriptor(raw: usize) {
    use windows_sys::Win32::Foundation::{
        ERROR_INVALID_HANDLE, GetHandleInformation, GetLastError,
    };
    let mut flags = 0;
    // SAFETY: this only queries the observed handle, with a valid writable flags pointer.
    assert_eq!(unsafe { GetHandleInformation(raw as _, &mut flags) }, 0);
    assert_eq!(unsafe { GetLastError() }, ERROR_INVALID_HANDLE);
}
// tests.test_native_package.NativePackage.test_actual_built_wheel_excludes_reference_and_local_content
#[test]
fn actual_standalone_archive_excludes_test_and_operator_content() {
    let root = Scratch::new();
    let artifact = native::package::package(
        &project(),
        Path::new(env!("CARGO_BIN_EXE_numera-emoji")),
        None,
        &root.0.join("out"),
        "119-fixture",
        &native::package::platform().unwrap(),
    )
    .unwrap();
    let forbidden = [
        "tests",
        "private",
        ".ai",
        ".claude",
        ".kiro",
        ".codex",
        ".agents",
        ".specify",
        "specs",
        "plans",
        "logs",
        "collection",
        "packs",
        "graphify-out",
        ".codebase-memory",
    ];
    let mut archive = zip::ZipArchive::new(std::fs::File::open(artifact).unwrap()).unwrap();
    let mut has_native = false;
    for i in 0..archive.len() {
        let mut member = archive.by_index(i).unwrap();
        let name = member.name().unwrap().into_owned();
        let path = Path::new(&name);
        let basename = path.file_name().unwrap().to_str().unwrap();
        if basename == format!("numera-emoji{}", std::env::consts::EXE_SUFFIX) {
            has_native = true;
        }
        for part in path.components() {
            assert!(
                !forbidden.contains(&part.as_os_str().to_str().unwrap()),
                "{name}"
            );
        }
        assert!(
            !["secrets.md", "explain-ai.md", ".ignoreme"]
                .contains(&basename.to_lowercase().as_str()),
            "{name}"
        );
        assert!(
            !basename.starts_with(".env") || basename == ".env.example",
            "{name}"
        );
        let mut bytes = Vec::new();
        member.read_to_end(&mut bytes).unwrap();
        assert_eq!(bytes.len() as u64, member.size());
    }
    assert!(has_native);
}
// tests.test_native_real_catalog.RealCatalogReplay.test_copied_operator_rows_replay_and_python_reads_native_changes
#[test]
#[ignore = "private catalog is local-by-design; public native catalog parity runs in CI"]
fn optional_real_catalog_snapshot_replays_and_sqlite_reads_native_changes() {
    // Compile with the opt-in fixture path, then select this case with --ignored;
    // the isolated runner does not inherit operator environment variables.
    let raw = std::env::var_os("NUMERA_REAL_CATALOG_FIXTURE")
        .or_else(|| option_env!("NUMERA_REAL_CATALOG_FIXTURE").map(std::ffi::OsString::from))
        .expect("ignored operator replay requires NUMERA_REAL_CATALOG_FIXTURE at compile time");
    let source = Path::new(&raw).canonicalize().unwrap();
    assert!(source.is_file());
    let before = native::package::digest(&source).unwrap();
    let root = Scratch::new();
    let copied = root.0.join("catalog.db");
    let input = Connection::open_with_flags(&source, OpenFlags::SQLITE_OPEN_READ_ONLY).unwrap();
    native::sqlite_snapshot::backup(&input, &copied).unwrap();
    drop(input);
    let db = Connection::open(&copied).unwrap();
    let bindings = db
        .prepare("SELECT content_key,file_path FROM items")
        .unwrap()
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    for (key, path) in bindings {
        let resolved = native::paths::resolve(source.parent().unwrap(), &path, &project());
        db.execute(
            "UPDATE items SET file_path=? WHERE content_key=?",
            rusqlite::params![resolved.to_str().unwrap(), key],
        )
        .unwrap();
    }
    drop(db);
    drop(native::catalog::Catalog::open(&copied, "2026-10-09 00:00:00 UTC", &project()).unwrap());
    let db = Connection::open(&copied).unwrap();
    let expected_rows=db.prepare("SELECT content_key,format,file_path,emojis,keywords,sources,phash,custom_emoji_id,uploaded,included FROM items ORDER BY position,content_key").unwrap().query_map([],|r|{
        let parse=|i|->rusqlite::Result<Value>{let raw=r.get::<_,String>(i)?;Ok(serde_json::from_str(&raw).unwrap())};
        Ok(json!({"content_key":r.get::<_,String>(0)?,"fmt":r.get::<_,String>(1)?,"file_path":r.get::<_,String>(2)?,"emojis":parse(3)?,"keywords":parse(4)?,"sources":parse(5)?,"phash":r.get::<_,Option<i64>>(6)?.map(|v|v as u64),"custom_emoji_id":r.get::<_,Option<String>>(7)?,"uploaded":r.get::<_,i64>(8)?!=0,"included":r.get::<_,i64>(9)?!=0}))
    }).unwrap().map(Result::unwrap).collect::<Vec<_>>();
    let expected_stats=db.prepare("SELECT format,COUNT(*),SUM(uploaded),SUM(CASE WHEN included=0 THEN 1 ELSE 0 END),SUM(CASE WHEN uploaded=0 AND included=1 THEN 1 ELSE 0 END) FROM items GROUP BY format").unwrap().query_map([],|r|Ok((r.get::<_,String>(0)?,json!({"total":r.get::<_,i64>(1)?,"uploaded":r.get::<_,i64>(2)?,"excluded":r.get::<_,i64>(3)?,"pending":r.get::<_,i64>(4)?})))).unwrap().map(Result::unwrap).collect::<serde_json::Map<_,_>>();
    let mut order = db
        .prepare("SELECT content_key FROM items ORDER BY position,content_key")
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    assert!(!order.is_empty(), "real fixture cannot be empty");
    let maps = db
        .prepare("SELECT base,content_key,custom_emoji_id FROM publications")
        .unwrap()
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    drop(db);
    let mut cat =
        native::catalog::Catalog::open(&copied, "2026-10-09 00:00:00 UTC", &project()).unwrap();
    let all = cat.all(None).unwrap();
    assert_eq!(all, expected_rows);
    assert_eq!(
        all.iter()
            .map(|row| row["content_key"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        order
    );
    let stats = cat.stats().unwrap();
    assert_eq!(stats, Value::Object(expected_stats));
    assert_eq!(
        stats
            .as_object()
            .unwrap()
            .values()
            .map(|s| s["total"].as_u64().unwrap())
            .sum::<u64>(),
        order.len() as u64
    );
    for (base, key, id) in &maps {
        assert_eq!(
            cat.dispatch(json!({"operation":"custom_id","base":base,"content_key":key}))
                .unwrap(),
            json!(id)
        );
    }
    order.reverse();
    assert_eq!(cat.order(&order).unwrap(), order.len());
    assert_eq!(
        cat.inclusion(&[order[0].clone()]).unwrap(),
        json!([order.len() - 1, 1])
    );
    drop(cat);
    let db = Connection::open(&copied).unwrap();
    let actual = db
        .prepare("SELECT content_key,included FROM items ORDER BY position,content_key")
        .unwrap()
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect::<Vec<_>>();
    assert_eq!(
        actual.iter().map(|(k, _)| k.clone()).collect::<Vec<_>>(),
        order
    );
    assert_eq!(actual[0].1, 0);
    for (base, key, id) in maps {
        assert_eq!(
            db.query_row(
                "SELECT custom_emoji_id FROM publications WHERE base=? AND content_key=?",
                [base, key],
                |r| r.get::<_, Option<String>>(0)
            )
            .unwrap(),
            id
        );
    }
    assert_eq!(
        native::package::digest(&source).unwrap(),
        before,
        "operator source database changed"
    );
}
