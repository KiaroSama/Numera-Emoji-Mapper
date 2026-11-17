use numera_runtime::developer;
use std::collections::BTreeMap;

// native/tests/tooling_similarity_benchmark.rs::contributor_options_and_logged_failure_keep_source_output_order
// Parent approved the historical --c catalog alias and empty-path '.' spelling.
// Preserve the exact historical rows until actual RED then narrow correction.
#[test]
fn historical_benchmark_catalog_options_preserve_all_original_variants() {
    use std::path::PathBuf;
    let parse = |tail: &[&str]| {
        let mut args = vec!["benchmark-similarity".into()];
        args.extend(tail.iter().map(|value| (*value).to_owned()));
        developer::arguments(&args)
    };
    for (args, expected) in [
        (
            vec!["--catalog", "first.db", "--catalog=second file.db"],
            Some(PathBuf::from("second file.db")),
        ),
        (vec!["--catalog="], Some(PathBuf::from("."))),
        (
            vec!["--c", "abbreviated.db"],
            Some(PathBuf::from("abbreviated.db")),
        ),
        (vec!["--catalog", "-1"], Some(PathBuf::from("-1"))),
        (vec![], None),
    ] {
        let developer::Command::Benchmark {
            catalog,
            count,
            seed,
            repeats,
        } = parse(&args).unwrap()
        else {
            panic!("benchmark command expected");
        };
        assert_eq!(catalog, expected);
        assert_eq!((count, seed, repeats), (1500, 321, 5));
    }
    for args in [
        vec!["--catalog"],
        vec!["--unknown"],
        vec!["--catalog", "--other"],
    ] {
        assert!(parse(&args).is_err());
    }
    assert!(matches!(
        developer::arguments(&["--help".into()]).unwrap(),
        developer::Command::Help
    ));
}
#[test]
fn build_worker_ceiling_preserves_source_positive_overrides() {
    for (cores, hook, cargo, expected) in [
        (None, None, None, 1),
        (Some(16), None, None, 4),
        (Some(8), Some("2"), Some("3"), 2),
        (Some(1), Some("0"), Some("-1"), 1),
        (Some(8), Some("bad"), Some(" 1 "), 1),
    ] {
        let mut environment = BTreeMap::new();
        if let Some(value) = hook {
            environment.insert("HOOKMAKER_MAX_TEST_WORKERS".into(), value.into());
        }
        if let Some(value) = cargo {
            environment.insert("CARGO_BUILD_JOBS".into(), value.into());
        }
        assert_eq!(developer::worker_limit(cores, &environment), expected);
    }
}
