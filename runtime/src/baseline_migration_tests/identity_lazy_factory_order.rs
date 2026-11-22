use super::identity_support::*;
use crate::{identity_cli, logging::RunLog};
use std::cell::Cell;
pub(super) fn help_does_not_construct_the_codec() {
    let f = Fixture::new();
    let log = RunLog::open(&f.root.0, "lazy-help", Vec::new());
    let calls = Cell::new(0);
    assert_eq!(
        identity_cli::run_with_codec_factory(&f.config, &["--help".into()], &log, |_, _| {
            calls.set(calls.get() + 1);
            Ok(Fake::default())
        })
        .unwrap(),
        0
    );
    assert_eq!(calls.get(), 0);
}
pub(super) fn missing_catalog_does_not_construct_the_codec() {
    let f = Fixture::new();
    let log = RunLog::open(&f.root.0, "lazy-missing", Vec::new());
    let calls = Cell::new(0);
    assert_eq!(
        identity_cli::run_with_codec_factory(
            &f.config,
            &["report".into(), "--data-dir".into(), "missing".into()],
            &log,
            |_, _| {
                calls.set(calls.get() + 1);
                Ok(Fake::default())
            }
        )
        .unwrap(),
        2
    );
    assert_eq!(calls.get(), 0);
}
pub(super) fn missing_restore_bundle_does_not_construct_the_codec() {
    let f = Fixture::new();
    let log = RunLog::open(&f.root.0, "lazy-restore", Vec::new());
    let calls = Cell::new(0);
    assert_eq!(
        identity_cli::run_with_codec_factory(
            &f.config,
            &[
                "restore".into(),
                "--data-dir".into(),
                f.data.to_str().unwrap().into()
            ],
            &log,
            |_, _| {
                calls.set(calls.get() + 1);
                Ok(Fake::default())
            }
        )
        .unwrap(),
        2
    );
    assert_eq!(calls.get(), 0);
}
pub(super) fn successful_restore_does_not_construct_the_codec() {
    let f = Fixture::new();
    let (_, _, fake) = f.pending();
    f.apply(&fake).unwrap();
    let manifest = f.manifest();
    let log = RunLog::open(&f.root.0, "lazy-real-restore", Vec::new());
    let calls = Cell::new(0);
    assert_eq!(
        identity_cli::run_with_codec_factory(
            &f.config,
            &[
                "restore".into(),
                "--apply".into(),
                "--data-dir".into(),
                f.data.to_str().unwrap().into(),
                "--bundle".into(),
                manifest.to_str().unwrap().into()
            ],
            &log,
            |_, _| {
                calls.set(calls.get() + 1);
                Ok(Fake::default())
            }
        )
        .unwrap(),
        0
    );
    assert_eq!(calls.get(), 0);
}
