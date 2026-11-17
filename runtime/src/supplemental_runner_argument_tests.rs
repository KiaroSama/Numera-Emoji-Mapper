//! Unwired until the private standalone runner argument correction is applied.
//! Retains the live native runner selection/admission boundary, not a Python API.
use super::test_arguments;
use std::ffi::OsString;
fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

// Native source: native/tests/tooling_test_runner.rs::libtest_arguments_preserve_selection_and_only_lower_parallelism
#[test]
fn libtest_arguments_preserve_selection_and_reject_invalid_parallelism_before_spawn() {
    for (input, expected) in [
        (
            vec!["--exact", "named"],
            vec!["--exact", "named", "--test-threads=1"],
        ),
        (
            vec!["--test-threads=8", "named"],
            vec!["--test-threads=1", "named"],
        ),
        (vec!["--test-threads", "1"], vec!["--test-threads", "1"]),
        (
            vec![
                "--test-threads=4",
                "--exact",
                "named",
                "--test-threads",
                "2",
            ],
            vec![
                "--test-threads=1",
                "--exact",
                "named",
                "--test-threads",
                "1",
            ],
        ),
    ] {
        let original = args(&input);
        let before = original.clone();
        assert_eq!(test_arguments(&original).unwrap(), args(&expected));
        assert_eq!(original, before);
    }
    for input in [
        vec!["--test-threads=0"],
        vec!["--test-threads"],
        vec!["--test-threads=bad"],
        vec!["--test-threads", "named"],
        vec!["--test-threads=184467440737095516160"],
    ] {
        assert!(
            test_arguments(&args(&input)).is_err(),
            "malformed selection {input:?} was silently discarded"
        );
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        let name = OsString::from_vec(vec![b'n', 0xff]);
        assert_eq!(
            test_arguments(std::slice::from_ref(&name)).unwrap(),
            [name, "--test-threads=1".into()]
        );
    }
}
