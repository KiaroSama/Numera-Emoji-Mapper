use super::identity_support::*;
use crate::{identity_cli, logging::RunLog};
use serde_json::json;
fn cli(f: &Fixture, media: &Fake, args: &[&str]) -> u8 {
    let log = RunLog::open(&f.root.0, "baseline-identity", Vec::new());
    let mut args = args.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    args.extend(["--data-dir".into(), f.data.to_str().unwrap().into()]);
    identity_cli::run_with_codec_factory(&f.config, &args, &log, |_, _| Ok(media.clone())).unwrap()
}
// tests.test_identity_migration.NothingIsAppliedFromAnIncompletePicture.test_report_mode_says_incomplete_rather_than_clean
#[test]
fn identity_report_mode_says_incomplete_rather_than_clean() {
    let f = Fixture::new();
    f.add(
        "v:gone",
        &f.data.join("absent.webm"),
        "video",
        None,
        None,
        None,
    );
    assert_eq!(cli(&f, &Fake::default(), &["report"]), 4);
}
// tests.test_identity_migration.NothingIsAppliedFromAnIncompletePicture.test_an_empty_catalog_is_clean_not_pending
#[test]
fn identity_an_empty_catalog_is_clean_not_pending() {
    let f = Fixture::new();
    assert_eq!(cli(&f, &Fake::default(), &["report"]), 0);
}
// tests.test_identity_migration.NothingIsAppliedFromAnIncompletePicture.test_a_clean_catalog_reports_clean
#[test]
fn identity_a_clean_catalog_reports_clean() {
    let f = Fixture::new();
    f.add(
        "v:same",
        &f.media("clip.webm"),
        "video",
        Some(7),
        None,
        None,
    );
    assert_eq!(
        cli(
            &f,
            &f.fake(&[("clip.webm", "v:same", Some(7))]),
            &["report"]
        ),
        0
    );
}
// tests.test_identity_migration.NothingIsAppliedFromAnIncompletePicture.test_a_pending_catalog_reports_stale_and_writes_nothing
#[test]
fn identity_a_pending_catalog_reports_stale_and_writes_nothing() {
    let f = Fixture::new();
    let before = std::fs::read(f.data.join("catalog.db")).unwrap();
    f.add("v:old", &f.media("clip.webm"), "video", None, None, None);
    assert_eq!(
        cli(&f, &f.fake(&[("clip.webm", "v:new", Some(1))]), &["report"]),
        3
    );
    assert_eq!(f.col("items", "content_key"), ["v:old"]);
    assert!(!before.is_empty());
}
// tests.test_identity_migration.NothingIsAppliedFromAnIncompletePicture.test_only_obsolete_plan_entries_are_informational
#[test]
fn identity_only_obsolete_plan_entries_are_informational() {
    let f = Fixture::new();
    f.add(
        "v:same",
        &f.media("clip.webm"),
        "video",
        Some(1),
        None,
        None,
    );
    f.plan("pk", &json!({"mixed":["v:ghost"]}));
    assert_eq!(
        cli(
            &f,
            &f.fake(&[("clip.webm", "v:same", Some(1))]),
            &["report"]
        ),
        0
    );
}
