//! Benchmark policy and read-only input seams; actual source/native timing remains a hosted gate.
use _native::tooling_similarity_benchmark::{
    Arguments, Failure, arguments, finish, load_hashes, run, summarize,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("logs/tmp");
        std::fs::create_dir_all(&parent).unwrap();
        let path = parent.join(format!(
            "similarity benchmark {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn five_sample_medians_report_before_the_source_size_sensitive_regression_gate() {
    // Clock samples are test inputs. Odd medians are the third sorted values, not means/minima.
    let report = summarize(100, [9.0, 1.0, 2.0, 8.0, 3.0], [5.0, 4.0, 9.0, 1.0, 6.0]).unwrap();
    assert_eq!(report.python_median, 3.0);
    assert_eq!(report.rust_median, 5.0);
    assert_eq!(
        report.line(),
        "Exact parity: hashes=100 python_median=3.000000s rust_median=5.000000s speedup=0.60x"
    );
    assert_eq!(
        report.check_regression().unwrap_err(),
        "Native median regressed against the same-input source baseline"
    );
    assert!(
        summarize(99, [1.0; 5], [2.0; 5])
            .unwrap()
            .check_regression()
            .is_ok()
    );
    assert!(
        summarize(100, [1.0; 5], [1.0; 5])
            .unwrap()
            .check_regression()
            .is_ok()
    );
    assert_eq!(
        summarize(1, [1e-9; 5], [0.0; 5]).unwrap().line(),
        "Exact parity: hashes=1 python_median=0.000000s rust_median=0.000000s speedup=1.00x"
    );
    for count in [0, 10001] {
        assert_eq!(
            summarize(count, [1.0; 5], [1.0; 5]).unwrap_err(),
            "Benchmark requires 1..10000 hashes"
        );
    }
    for invalid in [f64::NAN, f64::INFINITY, -1.0] {
        assert!(summarize(1, [invalid; 5], [1.0; 5]).is_err());
        assert!(summarize(1, [1.0; 5], [invalid; 5]).is_err());
    }
}

#[test]
fn contributor_options_and_logged_failure_keep_source_output_order() {
    assert_eq!(arguments(&[]).unwrap(), Arguments::Catalog(None));
    let strings = |args: &[&str]| args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    for (args, path) in [
        (
            vec!["--catalog", "first.db", "--catalog=second file.db"],
            "second file.db",
        ),
        (vec!["--catalog="], "."),
        (vec!["--c", "abbreviated.db"], "abbreviated.db"),
        (vec!["--catalog", "-1"], "-1"),
    ] {
        assert_eq!(
            arguments(&strings(&args)).unwrap(),
            Arguments::Catalog(Some(PathBuf::from(path)))
        );
    }
    for args in [
        vec!["--catalog"],
        vec!["--unknown"],
        vec!["--catalog", "--other"],
    ] {
        assert_eq!(arguments(&strings(&args)).unwrap(), Arguments::Exit(2));
    }
    assert_eq!(
        arguments(&strings(&["--help"])).unwrap(),
        Arguments::Exit(0)
    );
    let root = Root::new();
    let log = _native::logging::RunLog::open(&root.0, "benchmark_similarity", vec![]);
    let path = log.path.clone().expect("fixture log must open");
    let regressed = summarize(100, [1.0; 5], [2.0; 5]).unwrap();
    assert_eq!(finish(Ok(regressed), &log), 1);
    assert_eq!(
        finish(
            Err(Failure::Verification("controlled input refusal".into())),
            &log
        ),
        1
    );
    assert_eq!(
        finish(
            summarize(99, [1.0; 5], [2.0; 5]).map_err(Failure::Verification),
            &log
        ),
        0
    );
    drop(log);
    let output = std::fs::read_to_string(path).unwrap();
    let parity = output.find("Exact parity: hashes=100").unwrap();
    let refusal = output
        .find("Verification failed: Native median regressed")
        .unwrap();
    assert!(
        parity < refusal,
        "the parity observation precedes the regression refusal"
    );
    assert!(output.contains("Verification failed: controlled input refusal"));
    assert!(output.contains("[INFO] [benchmark_similarity]"));
    let invalid: _native::tooling_similarity_benchmark::Report =
        serde_json::from_str(r#"{"hashes":1,"python_median":-1.0,"rust_median":1.0}"#).unwrap();
    assert!(
        invalid.validate().is_err(),
        "worker output must not bypass timing admission"
    );
}

#[test]
fn catalog_input_is_read_only_ordered_masked_and_closed_even_on_failure() {
    let root = Root::new();
    // URI escaping must preserve spaces, Unicode, '#' and literal '%' on every supported OS.
    let catalog = root.0.join("catalog café # literal%20.db");
    let db = rusqlite::Connection::open(&catalog).unwrap();
    db.execute_batch(concat!(
        "CREATE TABLE items(content_key TEXT, position INTEGER, phash);",
        "INSERT INTO items VALUES ('z',2,7),('b',1,-1),('a',1,NULL),",
        "('a',1,'18446744073709551616'),('c',1,3.75);"
    ))
    .unwrap();
    db.close().unwrap();
    let before = std::fs::read(&catalog).unwrap();
    assert_eq!(load_hashes(Some(&catalog)).unwrap(), [0, u64::MAX, 3, 7]);
    assert_eq!(std::fs::read(&catalog).unwrap(), before);
    assert_eq!(std::fs::read_dir(&root.0).unwrap().count(), 1);
    // On Windows this move also detects an unclosed SQLite connection.
    let moved = root.0.join("moved catalog.db");
    std::fs::rename(&catalog, &moved).unwrap();
    let db = rusqlite::Connection::open(&moved).unwrap();
    db.execute(
        "UPDATE items SET phash='not an integer' WHERE content_key='z'",
        [],
    )
    .unwrap();
    db.close().unwrap();
    let failed_bytes = std::fs::read(&moved).unwrap();
    assert!(
        load_hashes(Some(&moved))
            .unwrap_err()
            .to_string()
            .contains("invalid literal")
    );
    assert_eq!(std::fs::read(&moved).unwrap(), failed_bytes);
    std::fs::rename(&moved, &catalog).unwrap();
    let missing = root.0.join("missing.db");
    assert!(load_hashes(Some(&missing)).is_err());
    assert!(
        !missing.exists(),
        "read-only input must never create a database"
    );
}

#[test]
fn catalog_cardinality_refuses_before_similarity_computation() {
    let root = Root::new();
    let catalog = root.0.join("bounded.db");
    let db = rusqlite::Connection::open(&catalog).unwrap();
    db.execute_batch("CREATE TABLE items(content_key TEXT, position INTEGER, phash INTEGER);")
        .unwrap();
    assert_eq!(
        load_hashes(Some(&catalog)).unwrap_err().to_string(),
        "Benchmark requires 1..10000 hashes"
    );
    db.execute_batch(concat!(
        "WITH RECURSIVE inputs(n) AS (VALUES(1) UNION ALL SELECT n+1 FROM inputs WHERE n<10001) ",
        "INSERT INTO items SELECT 'fixture',n,n FROM inputs;"
    ))
    .unwrap();
    db.close().unwrap();
    let before = std::fs::read(&catalog).unwrap();
    assert_eq!(
        load_hashes(Some(&catalog)).unwrap_err().to_string(),
        "Benchmark requires 1..10000 hashes"
    );
    assert_eq!(std::fs::read(&catalog).unwrap(), before);
    let db = rusqlite::Connection::open(&catalog).unwrap();
    db.execute("DELETE FROM items WHERE position=10001", [])
        .unwrap();
    db.close().unwrap();
    assert_eq!(load_hashes(Some(&catalog)).unwrap().len(), 10000);
    let db = rusqlite::Connection::open(&catalog).unwrap();
    db.execute("DELETE FROM items", []).unwrap();
    db.execute("INSERT INTO items VALUES('fixture',1,?1)", [f64::INFINITY])
        .unwrap();
    db.close().unwrap();
    assert!(
        matches!(load_hashes(Some(&catalog)), Err(Failure::Uncaught(_))),
        "int(infinity) raises OverflowError outside the source's caught tuple"
    );
    std::fs::remove_file(&catalog).unwrap();
}

#[test]
fn forged_reference_is_rejected_before_reading_catalog_or_executing_source() {
    let source = _native::source_git_inventory::Source {
        baseline_revision: "1a36a968b006236bd0665a1da65c541938c06061".into(),
        path: "tests/_similarity_oracle.py".into(),
        sha256: "9be48166b64ab2052798c67e2f7426c2dd0008fc27b4d668a785877fba7ac44e".into(),
        text: "raise RuntimeError('must never execute')".into(),
    };
    let root = Root::new();
    let missing = root.0.join("must not create.db");
    assert!(
        run(&source, Some(&missing))
            .unwrap_err()
            .to_string()
            .contains("source identity drift")
    );
    assert!(!missing.exists());
    assert_eq!(std::fs::read_dir(&root.0).unwrap().count(), 0);
}
