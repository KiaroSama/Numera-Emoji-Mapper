//! Original developer, launcher, CLI and sandbox cases against the real standalone Rust application.
#![allow(non_snake_case)]
use numera_runtime as runtime;
#[allow(dead_code)]
mod baseline_tooling_support;
#[path = "baseline_tooling_support/lifecycle_gates.rs"]
mod lifecycle_gates;
use baseline_tooling_support::*;
use runtime::{catalog::Catalog, config::Config, locks::Lease, tooling_launch, tooling_sandbox};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

mod TheSandboxChildGetsNoCredentials {
    use super::*;
    fn child(extra: &[(&str, &str)]) -> BTreeMap<String, String> {
        let mut original = machine();
        original.extend(extra.iter().map(|(k, v)| ((*k).into(), (*v).into())));
        tooling_launch::scrubbed_environment(&original, include_str!("../../.env.example"))
    }
    #[test]
    fn test_dotenv_reading_is_switched_off_for_the_child() {
        assert_eq!(child(&[])["NUMERA_EMOJI_MAPPER_NO_DOTENV"], "1");
    }
    #[test]
    fn test_an_exported_token_does_not_survive_into_the_child() {
        let env = child(&[
            ("GENERAL_BOT_TOKEN", "111:live"),
            ("COIN_BOT_TOKEN", "222:live"),
        ]);
        assert!(!env.contains_key("GENERAL_BOT_TOKEN"));
        assert!(!env.contains_key("COIN_BOT_TOKEN"));
    }
    #[test]
    fn test_every_key_the_template_names_is_dropped() {
        let template = include_str!("../../.env.example");
        let named = template
            .lines()
            .filter(|line| !line.trim_start().starts_with('#'))
            .filter_map(|line| line.split_once('=').map(|(key, _)| key.trim()))
            .collect::<Vec<_>>();
        assert!(!named.is_empty());
        let extra = named
            .iter()
            .map(|key| (*key, "synthetic"))
            .collect::<Vec<_>>();
        let env = child(&extra);
        assert!(named.iter().all(|key| !env.contains_key(*key)));
    }
    #[test]
    fn test_a_credential_shape_is_dropped_even_when_the_template_is_silent() {
        let env = child(&[
            ("SOME_PRIVATE_TOKEN", "x"),
            ("DEPLOY_SECRET", "y"),
            ("OWNER_ID", "7"),
        ]);
        for key in ["SOME_PRIVATE_TOKEN", "DEPLOY_SECRET", "OWNER_ID"] {
            assert!(!env.contains_key(key));
        }
    }
    #[test]
    fn test_the_ordinary_environment_still_reaches_the_child() {
        let env = child(&[("EMOJI_SANDBOX_CANARY", "kept")]);
        assert_eq!(env["EMOJI_SANDBOX_CANARY"], "kept");
        assert!(env.keys().any(|key| key.eq_ignore_ascii_case("PATH")));
    }
}

mod TheCloneSharesNothingWithTheSource {
    use super::*;
    #[test]
    fn test_no_cloned_file_shares_storage_with_its_source() {
        let root = Root::new();
        let source = root.source(&["s:a"], false);
        let session = root.clone(&source);
        let originals = paths(&source);
        let copies = paths(&session.directory);
        assert!(!copies.is_empty());
        assert_eq!(originals.len(), copies.len());
        for (a, b) in originals.iter().zip(&copies) {
            assert_eq!(std::fs::read(a).unwrap(), std::fs::read(b).unwrap());
            std::fs::write(b, b"edited clone").unwrap();
            assert_ne!(std::fs::read(a).unwrap(), std::fs::read(b).unwrap());
            assert_ne!(storage_identity(a), storage_identity(b));
        }
        session.finish().unwrap();
    }
    #[test]
    fn test_the_preview_cache_is_copied_not_shared() {
        let root = Root::new();
        let source = root.source(&["s:a"], false);
        let original = root.file("source/preview/a.webp", b"RIFF-preview");
        let session = root.clone(&source);
        let copy = session.directory.join("preview/a.webp");
        assert_eq!(
            std::fs::read(&copy).unwrap(),
            std::fs::read(&original).unwrap()
        );
        assert_ne!(storage_identity(&copy), storage_identity(&original));
        std::fs::write(&copy, b"changed").unwrap();
        assert_eq!(std::fs::read(&original).unwrap(), b"RIFF-preview");
        session.finish().unwrap();
    }
    // The failed-preview-copy case uses create_with's private I/O seam in the production-module child unit.
    #[test]
    fn test_a_media_file_outside_the_source_is_copied_in_not_referenced() {
        let root = Root::new();
        let source = root.source(&["s:a"], true);
        let session = root.clone(&source);
        let files = paths(&session.directory);
        assert!(!files.is_empty());
        for path in files {
            assert!(
                runtime::migration_bundle::resolved(&path)
                    .unwrap()
                    .starts_with(runtime::migration_bundle::resolved(&session.directory).unwrap())
            );
        }
        session.finish().unwrap();
    }
    #[test]
    fn test_a_project_relative_media_path_is_copied_in() {
        let root = Root::new();
        let source = root.source(&["s:r"], false);
        root.file("tests/__sandbox_rel__/rel.png", b"PNG-relative");
        let db = rusqlite::Connection::open(source.join("catalog.db")).unwrap();
        db.execute(
            "UPDATE items SET file_path='tests/__sandbox_rel__/rel.png'",
            [],
        )
        .unwrap();
        db.close().unwrap();
        let session = root.clone(&source);
        let files = paths(&session.directory);
        assert_eq!(files.len(), 1);
        assert!(files[0].starts_with(&session.directory));
        assert_eq!(std::fs::read(&files[0]).unwrap(), b"PNG-relative");
        session.finish().unwrap();
    }
    #[test]
    fn test_a_key_with_colons_becomes_a_real_file() {
        let root = Root::new();
        let keys = [
            format!("s:{}", "b".repeat(32)),
            format!("s:{}:{}", "c".repeat(32), "d".repeat(32)),
        ];
        let source = root.source(&[&keys[0], &keys[1]], false);
        let session = root.clone(&source);
        let files = paths(&session.directory);
        assert_eq!(files.len(), 2);
        for path in files {
            assert!(!path.file_name().unwrap().to_str().unwrap().contains(':'));
            assert!(path.is_file() && path.metadata().unwrap().len() > 0);
        }
        session.finish().unwrap();
    }
    #[test]
    fn test_rows_still_only_in_the_write_ahead_log_are_cloned() {
        let root = Root::new();
        let source = root.source(&["s:e", "s:f", "s:g"], false);
        let db = rusqlite::Connection::open(source.join("catalog.db")).unwrap();
        db.execute_batch("PRAGMA journal_mode=WAL;").unwrap();
        let late = root.file("source/media/late.png", b"PNG-late-fixture");
        db.execute("INSERT INTO items(content_key,format,file_path,created_utc) VALUES('s:inwal','static',?,'now')",[late.to_str().unwrap()]).unwrap();
        let expected: i64 = db
            .query_row("SELECT COUNT(*) FROM items", [], |row| row.get(0))
            .unwrap();
        assert!(source.join("catalog.db-wal").is_file());
        let session = root.clone(&source);
        assert_eq!(paths(&session.directory).len(), expected as usize);
        db.close().unwrap();
        session.finish().unwrap();
    }
    #[test]
    fn test_the_curation_plan_travels_with_the_clone() {
        let root = Root::new();
        let source = root.source(&["s:a"], false);
        let plan = br#"{"version":1,"targets":[],"known":[],"excluded":[],"moves":[],"held":[]}"#;
        root.file("source/pack_plan.json", plan);
        let session = root.clone(&source);
        assert_eq!(
            std::fs::read(session.directory.join("pack_plan.json")).unwrap(),
            plan
        );
        session.finish().unwrap();
    }
    #[test]
    fn test_a_source_without_a_plan_still_clones() {
        let root = Root::new();
        let source = root.source(&["s:a"], false);
        let session = root.clone(&source);
        assert_eq!(session.count, 1);
        assert!(!session.directory.join("pack_plan.json").exists());
        session.finish().unwrap();
    }
}

mod TheCloneRefusesRatherThanLie {
    use super::*;
    #[test]
    fn test_an_occupied_destination_is_never_erased() {
        let root = Root::new();
        let source = root.source(&["s:a"], false);
        let path = root.file("panel-sandbox-dest/someone-elses-work.txt", b"precious");
        assert!(tooling_sandbox::Session::create(&root.0, &source, &root.destination()).is_err());
        assert_eq!(std::fs::read(path).unwrap(), b"precious");
    }
    #[test]
    fn test_a_destination_inside_the_source_is_refused() {
        let root = Root::new();
        let source = root.source(&["s:a"], false);
        assert!(tooling_sandbox::Session::create(&root.0, &source, &source.join("inner")).is_err());
        assert!(!source.join("inner").exists());
    }
    #[test]
    fn test_a_source_with_no_catalog_is_refused() {
        let root = Root::new();
        let source = root.0.join("empty");
        std::fs::create_dir(&source).unwrap();
        assert!(tooling_sandbox::Session::create(&root.0, &source, &root.destination()).is_err());
        assert!(!root.destination().exists());
    }
    #[test]
    fn test_an_interrupted_migration_refuses_and_leaves_nothing_behind() {
        let root = Root::new();
        let source = root.source(&["s:a"], false);
        root.file("source/identity-migration.journal.json", b"{}");
        let error = tooling_sandbox::Session::create(&root.0, &source, &root.destination())
            .err()
            .unwrap();
        assert!(error.contains("interrupted identity migration"));
        assert!(!root.destination().exists());
    }
    #[test]
    fn test_a_source_another_writer_holds_is_refused() {
        let root = Root::new();
        let source = root.source(&["s:a"], false);
        let _owner = Catalog::open(&source.join("catalog.db"), UTC, &root.0).unwrap();
        let project = root.0.clone();
        let target = root.destination();
        let source2 = source.clone();
        let result = std::thread::spawn(move || {
            tooling_sandbox::Session::create(&project, &source2, &target)
                .map(|session| session.finish())
        })
        .join()
        .unwrap();
        let error = result.err().unwrap();
        assert!(error.contains("ownership refused"));
        assert!(!root.destination().exists());
    }
    #[test]
    fn test_a_missing_media_file_abandons_the_clone_entirely() {
        let root = Root::new();
        let source = root.source(&["s:a"], false);
        for path in paths(&source) {
            std::fs::remove_file(path).unwrap();
        }
        assert!(tooling_sandbox::Session::create(&root.0, &source, &root.destination()).is_err());
        assert!(!root.destination().exists());
        assert!(source.join("catalog.db").is_file());
    }
}

mod TheWrapperRefusesAnythingOutsideItsAllowlist {
    use super::*;
    fn refused(args: &[&str]) {
        assert!(
            tooling_launch::sandbox_options(
                &args.iter().map(|value| (*value).into()).collect::<Vec<_>>(),
                9450
            )
            .is_err()
        );
    }
    #[test]
    fn test_a_data_directory_override_cannot_be_smuggled_through() {
        refused(&["--data-dir", "collection"]);
    }
    #[test]
    fn test_an_unknown_option_is_not_forwarded() {
        refused(&["--nonsense"]);
    }
    #[test]
    fn test_abbreviations_are_refused() {
        refused(&["--sou", "x"]);
        refused(&["--wi", "3"]);
    }
    #[test]
    fn test_the_presentation_options_are_accepted_and_reach_the_panel() {
        let (_, options) = tooling_launch::sandbox_options(
            &[
                "--all",
                "--with-pack",
                "3",
                "--with-pack",
                "4",
                "--bot-username",
                "b",
            ]
            .map(str::to_owned),
            9450,
        )
        .unwrap();
        let args = tooling_launch::panel_arguments(&options, Path::new("/clone"), 9450).unwrap();
        assert_eq!(&args[1..3], ["--data-dir", "/clone"]);
        assert!(args.contains(&"--all".into()));
        assert_eq!(args.iter().filter(|arg| *arg == "--with-pack").count(), 2);
        assert!(args.contains(&"b".into()));
        assert_eq!(args.iter().filter(|arg| *arg == "--data-dir").count(), 1);
    }
    #[test]
    fn test_the_real_panels_port_is_refused() {
        refused(&["--port", "9450"]);
    }
    #[test]
    fn test_a_number_that_is_not_a_port_is_refused() {
        for value in ["0", "70000"] {
            refused(&["--port", value]);
        }
    }
}

mod TheSweepReclaimsOnlyWhatItCanProveIsAbandoned {
    use super::*;
    #[test]
    fn test_an_abandoned_sandbox_is_reclaimed() {
        let root = Root::new();
        let old = abandoned(&root, "dead", "valid");
        assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 1);
        assert!(!old.join("catalog.db").exists());
    }
    #[test]
    fn test_a_sandbox_whose_lease_is_held_survives() {
        let root = Root::new();
        let live = abandoned(&root, "live", "valid");
        let _lease =
            Lease::acquire(&tooling_sandbox::lifetime_lock_path(&live).unwrap(), UTC).unwrap();
        assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 0);
        assert!(live.join("catalog.db").exists());
    }
    #[test]
    fn test_an_unmarked_directory_is_left_alone() {
        let root = Root::new();
        let path = abandoned(&root, "unmarked", "unmarked");
        assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 0);
        assert!(path.join("catalog.db").exists());
    }
    #[test]
    fn test_a_marker_naming_another_directory_is_foreign() {
        let root = Root::new();
        let path = abandoned(&root, "foreign", "foreign");
        assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 0);
        assert!(path.join("catalog.db").exists());
    }
    #[test]
    fn test_a_malformed_marker_is_not_proof_of_abandonment() {
        let root = Root::new();
        let path = abandoned(&root, "broken", "malformed");
        assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 0);
        assert!(path.join("catalog.db").exists());
    }
    #[test]
    fn test_a_reclaimed_directory_keeps_its_lock_file() {
        let root = Root::new();
        let old = abandoned(&root, "dead", "valid");
        let path = tooling_sandbox::lifetime_lock_path(&old).unwrap();
        drop(Lease::acquire(&path, UTC).unwrap());
        assert!(path.exists());
        assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 1);
        assert!(!old.join("catalog.db").exists());
        assert!(path.exists());
    }
    #[test]
    fn test_a_symlink_is_never_followed() {
        let root = Root::new();
        let outside = root.file("real-data/precious.txt", b"keep");
        let link = root.0.join("panel-sandbox-link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(outside.parent().unwrap(), &link).unwrap();
        #[cfg(windows)]
        {
            junction::create(outside.parent().unwrap(), &link).unwrap();
        }
        assert_eq!(tooling_sandbox::sweep_stale(&root.0).unwrap(), 0);
        assert_eq!(std::fs::read(outside).unwrap(), b"keep");
        #[cfg(unix)]
        std::fs::remove_file(link).unwrap();
        #[cfg(windows)]
        std::fs::remove_dir(link).unwrap();
    }
}

mod TheScrubLeavesTheProcessAbleToRun {
    use super::*;
    #[test]
    fn test_the_machine_environment_survives_inside_the_context() {
        let mut before = machine();
        for key in ["A", "B", "C", "D", "E", "F"] {
            before.insert(key.into(), "synthetic".into());
        }
        before.insert("GENERAL_BOT_TOKEN".into(), "111:live".into());
        let env = tooling_launch::scrubbed_environment(&before, "");
        assert_eq!(env["NUMERA_EMOJI_MAPPER_NO_DOTENV"], "1");
        assert!(!env.contains_key("GENERAL_BOT_TOKEN"));
        assert!(env.keys().any(|key| key.eq_ignore_ascii_case("PATH")));
        #[cfg(windows)]
        assert!(env.keys().any(|key| key.eq_ignore_ascii_case("SystemRoot")));
        assert!(env.len() > 5);
    }
    #[test]
    fn test_the_environment_is_restored_afterwards() {
        let before = std::env::vars().collect::<BTreeMap<_, _>>();
        let input = machine();
        let copy = input.clone();
        let _ = tooling_launch::scrubbed_environment(&input, include_str!("../../.env.example"));
        assert_eq!(input, copy);
        assert!(std::env::vars().collect::<BTreeMap<_, _>>() == before);
    }
}

mod ThePanelCanRunInProcessWithoutAdoptingAListener {
    use super::*;
    #[test]
    fn test_reuse_disabled_reaches_neither_reopen_call_site() {
        let root = Root::new();
        root.install();
        let source = root.source(&["s:a"], false);
        let legacy = Legacy::new();
        let before = std::fs::read(source.join("catalog.db")).unwrap();
        let result = root.dev(&[
            "panel-sandbox",
            "--source",
            source.to_str().unwrap(),
            "--port",
            &legacy.port.to_string(),
        ]);
        assert_eq!(
            result.code, 2,
            "owned sandbox must refuse an occupied listener, never adopt it: {}{}",
            result.stdout, result.stderr
        );
        assert!(
            legacy.seen.lock().unwrap().is_empty(),
            "sandbox contacted an unrelated existing service"
        );
        assert_eq!(std::fs::read(source.join("catalog.db")).unwrap(), before);
    }
    #[test]
    fn test_main_still_reads_sys_argv_when_given_none() {
        let file = source("runtime/src/entry.rs");
        let function = file
            .items
            .iter()
            .find_map(|item| match item {
                syn::Item::Fn(function) if function.sig.ident == "run" => Some(function),
                _ => None,
            })
            .unwrap();
        assert!(function.sig.inputs.is_empty());
        struct Arguments(bool);
        impl<'ast> syn::visit::Visit<'ast> for Arguments {
            fn visit_expr_call(&mut self, node: &'ast syn::ExprCall) {
                if let syn::Expr::Path(path) = &*node.func {
                    let names = path
                        .path
                        .segments
                        .iter()
                        .map(|segment| segment.ident.to_string())
                        .collect::<Vec<_>>();
                    if names == ["std", "env", "args"] {
                        self.0 = true;
                    }
                }
                syn::visit::visit_expr_call(self, node);
            }
        }
        let mut arguments = Arguments(false);
        syn::visit::Visit::visit_block(&mut arguments, &function.block);
        assert!(arguments.0);
        let root = Root::new();
        root.install();
        let source = root.source(&["s:a"], false);
        let legacy = Legacy::new();
        let result = root.run(
            &[
                "panel",
                "--data-dir",
                source.to_str().unwrap(),
                "--port",
                &legacy.port.to_string(),
                "--no-open",
            ],
            &[],
        );
        assert_eq!(
            result.code, 0,
            "ordinary command-line panel reuse must remain enabled: {}",
            result.stderr
        );
        assert_eq!(
            *legacy.seen.lock().unwrap(),
            ["/api/session", "/api/ping", "/"]
        );
    }
}

mod TheSandboxHoldsItsLeaseForTheLifeOfTheServer {
    use super::*;
    #[test]
    fn test_the_lease_is_held_while_the_panel_runs_and_the_sweep_skips_it() {
        let root = Root::new();
        root.install();
        let source = root.source(&["s:a"], false);
        let image = image::RgbaImage::from_pixel(4096, 4096, image::Rgba([240, 20, 20, 255]));
        let media_bytes = runtime::native_raster::encode_png(&image).unwrap();
        drop(image);
        root.file("source/media/0.png", &media_bytes);
        let before = std::env::vars().collect::<BTreeMap<_, _>>();
        let source_bytes = std::fs::read(source.join("catalog.db")).unwrap();
        let mut live = Live::wrapper(&root, &source, port());
        live.ready();
        let clone = active_clone(&root);
        let account_connections = live.account.as_ref().unwrap().connections.clone();
        assert_eq!(
            tooling_sandbox::sweep_stale(clone.parent().unwrap()).unwrap(),
            0
        );
        assert!(clone.join("catalog.db").is_file());
        assert!(
            Lease::acquire(&tooling_sandbox::lifetime_lock_path(&clone).unwrap(), UTC).is_err()
        );
        assert!(
            live.account
                .as_ref()
                .unwrap()
                .lookups
                .lock()
                .unwrap()
                .is_empty(),
            "isolated panel attempted an account request with exported credentials"
        );
        let (_, page) = live.get("/");
        assert!(!String::from_utf8_lossy(&page).contains("111:live"));
        let url = format!("{}/preview/s%3Aa?size=104", live.url);
        let request = std::thread::spawn(move || get(&url));
        let codec = super::lifecycle_gates::observe_active_codec(&root);
        assert_eq!(live.graceful(), 130);
        assert!(
            codec.gone(),
            "the original native codec process survived clone cleanup"
        );
        let _ = request.join().unwrap();
        assert_eq!(
            std::fs::read(source.join("media/0.png")).unwrap(),
            media_bytes
        );
        assert_eq!(
            account_connections.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "exported credentials reached the account API during the actual server lifetime"
        );
        assert!(!clone.exists());
        assert!(std::env::vars().collect::<BTreeMap<_, _>>() == before);
        assert_eq!(
            std::fs::read(source.join("catalog.db")).unwrap(),
            source_bytes
        );
    }
}

mod TheLifetimeLeaseDoesNotBlockThePanelItStarts {
    use super::*;
    #[test]
    fn test_the_panel_can_open_its_catalog_while_the_lease_is_held() {
        let root = Root::new();
        let source = root.source(&["s:a"], false);
        let session = root.clone(&source);
        let cat = Catalog::open(&session.directory.join("catalog.db"), UTC, &root.0).unwrap();
        assert_eq!(cat.all(None).unwrap().len(), 1);
        drop(cat);
        session.finish().unwrap();
    }
    #[test]
    fn test_the_wrapper_lets_the_real_panel_reach_its_catalog() {
        let root = Root::new();
        root.install();
        let source = root.source(&["s:a"], false);
        let live = Live::wrapper(&root, &source, port());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        live.ready_for_catalog(deadline);
        let clone = active_clone(&root);
        let cat = Catalog::open(&clone.join("catalog.db"), UTC, &root.0).unwrap();
        assert_eq!(cat.all(None).unwrap().len(), 1);
        drop(cat);
        assert_eq!(live.get("/api/session").0, 200);
        assert_eq!(live.graceful(), 130);
        assert!(!clone.exists());
    }
    #[test]
    fn test_the_lifetime_lock_is_not_the_catalogs_writer_lock() {
        let root = Root::new();
        let path = tooling_sandbox::lifetime_lock_path(&root.0).unwrap();
        assert_ne!(path.file_name().unwrap(), ".maintenance.lock");
        assert_ne!(path, root.0.join(".maintenance.lock"));
    }
}

#[path = "baseline_tooling_cases/launcher_flags.rs"]
mod launcher_flags;

#[path = "baseline_tooling_cases/entry_points.rs"]
mod EntryPointsImport;

#[path = "baseline_tooling_cases/hermetic_suite.rs"]
mod SuiteIsHermetic;

#[path = "baseline_tooling_cases/fetch_pack_exit_codes.rs"]
mod FetchPackExitCodes;

#[path = "baseline_tooling_cases/image_commands.rs"]
mod image_commands;

mod PanelLottieBound {
    use super::*;
    #[test]
    fn test_oversized_tgs_is_rejected() {
        let root = Root::new();
        root.install();
        let data = lottie(&root);
        let mut live = Live::panel(&root, &data, port());
        live.ready();
        let (code, body) = live.get("/preview/bomb");
        assert_eq!(code, 404);
        assert!(body.len() < 1024);
    }
    #[test]
    fn test_normal_tgs_is_served_as_animated_webp() {
        let root = Root::new();
        root.install();
        let data = lottie(&root);
        let mut live = Live::panel(&root, &data, port());
        live.ready();
        let (code, body) = live.get("/preview/good");
        assert_eq!(code, 200);
        assert_eq!(&body[..4], b"RIFF");
        assert_eq!(&body[8..12], b"WEBP");
    }
    #[test]
    fn test_unknown_key_is_404() {
        let root = Root::new();
        root.install();
        let data = lottie(&root);
        let mut live = Live::panel(&root, &data, port());
        live.ready();
        assert_eq!(live.get("/preview/nope").0, 404);
    }
}

#[path = "baseline_tooling_cases/cli_reference.rs"]
mod TheCliReferenceMatchesTheTools;

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
