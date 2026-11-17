//! Four-path parser and explicit fixture custody tails against the actual standalone implementation.
use crate::tooling_fixture;
use numera_runtime::tooling_convert_cli;
use tooling_fixture::Root;
fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}
#[test]
fn fixture_cleanup_preserves_unresolved_children_and_unreadable_child_storage() {
    let root = Root::new("converter cli");
    let children = root.0.join("logs/tmp/tooling");
    let child = children.join("synthetic child");
    std::fs::create_dir_all(&child).unwrap();
    let marker = child.join("codec-cleanup-pending.json");
    let evidence = b"synthetic ownership evidence; no process started";
    std::fs::write(&marker, evidence).unwrap();
    assert!(root.clean().unwrap_err().contains("unresolved"));
    assert_eq!(std::fs::read(&marker).unwrap(), evidence);
    // Remove only this test's synthetic evidence after proving the retention branch.
    std::fs::remove_file(marker).unwrap();
    std::fs::remove_dir(child).unwrap();
    std::fs::remove_dir(&children).unwrap();
    std::fs::write(&children, evidence).unwrap();
    assert!(root.clean().unwrap_err().contains("Cannot inspect"));
    assert_eq!(std::fs::read(&children).unwrap(), evidence);
    std::fs::remove_file(children).unwrap();
    root.clean().unwrap();
    assert!(!root.0.exists());
}

#[test]
fn all_four_original_paths_are_required_without_losing_argparse_defaults() {
    use tooling_convert_cli::Arguments;
    let complete = args(&[
        "--in",
        "input with spaces",
        "--out",
        "output café",
        "--stdout",
        "stdout #.txt",
        "--stderr",
        "stderr %.txt",
    ]);
    assert_eq!(
        tooling_convert_cli::arguments(&complete).unwrap(),
        Arguments::Convert {
            source: "input with spaces".into(),
            output: "output café".into(),
            stdout: "stdout #.txt".into(),
            stderr: "stderr %.txt".into(),
        }
    );
    for omitted in 0..4 {
        let mut missing = complete.clone();
        missing.drain(omitted * 2..omitted * 2 + 2);
        assert!(tooling_convert_cli::arguments(&missing).is_err());
    }
    assert_eq!(
        tooling_convert_cli::arguments(&args(&[
            "--i=-1",
            "--o=",
            "--stdou=first",
            "--stderr=errors",
            "--stdout=last",
        ]))
        .unwrap(),
        Arguments::Convert {
            source: "-1".into(),
            output: "".into(),
            stdout: "last".into(),
            stderr: "errors".into(),
        }
    );
    for invalid in [
        vec![],
        vec!["--in"],
        vec!["--st=x"],
        vec!["--unknown"],
        vec!["positional"],
    ] {
        assert!(tooling_convert_cli::arguments(&args(&invalid)).is_err());
    }
    assert_eq!(
        tooling_convert_cli::arguments(&args(&["--help"])).unwrap(),
        Arguments::Exit(0)
    );
}
