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
            assert!(path.canonicalize().unwrap().starts_with(&session.directory));
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
        let mut live = Live::wrapper(&root, &source, port());
        live.ready();
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

mod TheLauncherPassesOnlyRealFlags {
    use super::*;
    #[test]
    fn test_the_scan_finds_the_launchers_modules() {
        let root = Root::new();
        root.install();
        let actual = launcher_requests(&root);
        for command in [
            "build-pack",
            "build-collection",
            "panel",
            "fetch-pack",
            "fetch-emoji-ids",
            "sync-order",
            "pack-manifest",
            "pack-archive",
            "status",
            "make-emoji-pngs",
            "add-media",
            "emoji-bot",
        ] {
            assert!(
                actual.contains_key(command),
                "{command} missing from actual launcher actions"
            );
        }
        assert!(actual["panel"].contains("--with-pack"));
        assert!(actual["fetch-pack"].contains("--token-env"));
        assert!(actual.len() >= 13);
        let observed_flags = actual
            .values()
            .flat_map(|flags| flags.iter().cloned())
            .collect::<BTreeSet<_>>();
        let source_flags = launcher_source_flags();
        assert!(!source_flags.is_empty());
        assert_eq!(
            observed_flags, source_flags,
            "actual menu probes must exercise every flag literal in its production builders"
        );
        let modules = source("runtime/src/lib.rs");
        assert!(
            modules.items.iter().any(
                |item| matches!(item,syn::Item::Mod(module)if module.ident=="tooling_menu_cli")
            )
        );
        let menu = root.run(&["menu"], &[]);
        assert_eq!(menu.code, 0, "{}", menu.stderr);
        assert!(menu.stdout.contains("Bye."));
        let doctor = root.run(&["doctor"], &[]);
        assert_eq!(doctor.code, 0, "{}", doctor.stderr);
        assert!(doctor.stdout.contains("Environment check complete"));
    }
    #[test]
    fn test_every_flag_is_in_its_modules_help() {
        let root = Root::new();
        root.install();
        for (command, flags) in launcher_requests(&root) {
            if flags.is_empty() {
                continue;
            }
            let result = root.run(&[&command, "--help"], &[]);
            assert_eq!(result.code, 0, "{command}: {}", result.stderr);
            for flag in flags {
                assert!(
                    result.stdout.contains(&flag),
                    "launcher passed unsupported {command} {flag}"
                );
            }
        }
    }
    #[test]
    fn test_the_coin_rebuild_default_command_still_exists() {
        let root = Root::new();
        root.install();
        assert!(launcher_requests(&root).contains_key("coin-rebuild"));
        let result = root.run(&["coin-rebuild", "--help"], &[]);
        assert_eq!(result.code, 0, "{}", result.stderr);
        assert!(result.stdout.contains("all"));
    }
}

mod EntryPointsImport {
    use super::*;
    #[test]
    fn test_discovery_found_the_expected_tools() {
        let commands = commands();
        assert!(commands.len() >= 15);
        for command in [
            "build-pack",
            "emoji-bot",
            "panel",
            "coin-verify-logos",
            "coin-rebuild",
        ] {
            assert!(commands.contains(command));
        }
        assert_eq!(
            std::fs::read_dir(project())
                .unwrap()
                .filter(|entry| entry
                    .as_ref()
                    .unwrap()
                    .path()
                    .extension()
                    .is_some_and(|suffix| suffix == "py"))
                .count(),
            0
        );
        let metadata = metadata();
        let package = metadata["packages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|value| value["name"] == "numera-emoji-runtime")
            .unwrap();
        let bins = package["targets"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|target| {
                target["kind"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|kind| kind == "bin")
            })
            .collect::<Vec<_>>();
        let names = bins
            .iter()
            .map(|target| target["name"].as_str().unwrap())
            .collect::<BTreeSet<_>>();
        for required in ["numera-emoji", "numera-runtime-tests", "numera-dev"] {
            assert!(
                names.contains(required),
                "required actual Rust target is missing: {required}"
            );
        }
        assert_eq!(
            names.len(),
            bins.len(),
            "duplicate Cargo binary target names"
        );
        for target in bins {
            assert!(target["src_path"].as_str().unwrap().ends_with(".rs"));
        }
    }
    #[test]
    fn test_command_modules_preserve_project_data_paths() {
        let root = Root::new();
        root.install();
        root.raster("coins/logos/png/red.png", [240, 20, 20, 255]);
        let foreign = Root::new();
        let result = root.run_at(&foreign.0, &["make-emoji-pngs"], &[]);
        assert_eq!(result.code, 0, "{}", result.stderr);
        assert!(root.0.join("coins/logos/emoji/red.png").is_file());
        assert!(!foreign.0.join("coins").exists());
        assert!(root.0.join("logs").is_dir());
        struct RootJoin(bool);
        impl<'ast> syn::visit::Visit<'ast> for RootJoin {
            fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
                if node.method=="join"&&node.args.first().is_some_and(|arg|matches!(arg,syn::Expr::Lit(value)if matches!(&value.lit,syn::Lit::Str(text)if text.value()=="state_emoji_bot.json")))&&matches!(&*node.receiver,syn::Expr::Field(field)if matches!(&field.member,syn::Member::Named(name)if name=="root")){self.0=true;}
                syn::visit::visit_expr_method_call(self, node);
            }
        }
        let file =
            syn::parse_file(&std::fs::read_to_string(owned_module("emoji_bot")).unwrap()).unwrap();
        let mut found = RootJoin(false);
        syn::visit::Visit::visit_file(&mut found, &file);
        assert!(
            found.0,
            "actual bot offset must remain under configured project root"
        );
    }
    #[test]
    fn test_every_entry_point_imports() {
        let root = Root::new();
        root.install();
        for command in commands() {
            let result = root.run(&[&command, "--help"], &[]);
            if command == "emoji-bot" {
                assert_eq!(result.code, 2);
            } else {
                assert_eq!(result.code, 0, "{command}: {}", result.stderr);
            }
            assert!(!result.stderr.contains("panicked"));
        }
        let file = source("runtime/src/lib.rs");
        assert!(
            file.items
                .iter()
                .filter(|item| matches!(item, syn::Item::Mod(_)))
                .count()
                >= 100
        );
    }
}

mod SuiteIsHermetic {
    use super::*;
    #[test]
    fn test_the_guard_was_installed_before_the_test_modules() {
        assert_eq!(
            std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
            Ok("1"),
            "run through numera-runtime-tests; no guard may be enabled midway"
        );
        assert!(
            runtime::telegram::Telegram::new("synthetic".into(), "https://api.telegram.org".into())
                .is_err()
        );
    }
    #[test]
    fn test_the_real_dotenv_values_are_not_loaded() {
        let root = Root::new();
        root.file(
            ".env",
            b"SYNTHETIC_DO_NOT_LOAD=synthetic-private-value-12345\n",
        );
        let values = BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]);
        let config = Config::load_with_environment(root.0.clone(), &values).unwrap();
        assert!(config.value("SYNTHETIC_DO_NOT_LOAD").is_empty());
        assert!(!std::env::vars().any(|(key, _)| key == "SYNTHETIC_DO_NOT_LOAD"));
    }
    #[test]
    fn test_load_env_is_a_no_op_under_the_guard() {
        let root = Root::new();
        root.file(".env", b"SYNTHETIC_LOAD_CANARY=changed\n");
        let before = std::env::vars().collect::<BTreeMap<_, _>>();
        let input = BTreeMap::from([
            ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
            ("KEEP".into(), "untouched".into()),
        ]);
        let config = Config::load_with_environment(root.0.clone(), &input).unwrap();
        assert_eq!(config.value("KEEP"), "untouched");
        assert_eq!(config.value("SYNTHETIC_LOAD_CANARY"), "");
        assert!(std::env::vars().collect::<BTreeMap<_, _>>() == before);
    }
    #[test]
    fn test_outbound_connections_are_refused() {
        assert_eq!(
            std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref(),
            Ok("1")
        );
        for base in [
            "https://api.telegram.org",
            "https://8.8.8.8",
            "http://localhost:443",
        ] {
            let error = runtime::telegram::Telegram::new("synthetic".into(), base.into())
                .err()
                .unwrap();
            assert!(error.to_string().contains("only to numeric loopback"));
        }
    }
    #[test]
    fn test_loopback_still_works() {
        let server = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let stream = std::net::TcpStream::connect_timeout(
            &server.local_addr().unwrap(),
            std::time::Duration::from_secs(5),
        )
        .unwrap();
        drop(stream);
    }
}

mod FetchPackExitCodes {
    use super::*;
    #[test]
    fn test_every_pack_failing_exits_nonzero() {
        let root = Root::new();
        root.install();
        let server = FailureServer::new();
        let result = root.run(
            &[
                "fetch-pack",
                "--data-dir",
                root.0.join("collection").to_str().unwrap(),
                "gone_by_bot",
                "https://t.me/addemoji/also_gone",
            ],
            &server.env(),
        );
        assert_eq!(
            *server.lookups.lock().unwrap(),
            ["gone_by_bot", "also_gone"]
        );
        assert_eq!(result.code, 4);
        assert!(result.stdout.contains("packs_failed=2"));
    }
    #[test]
    fn test_negative_limit_is_rejected_before_any_api_call() {
        let root = Root::new();
        root.install();
        let server = FailureServer::new();
        let result = root.run(&["fetch-pack", "somepack", "--limit", "-1"], &server.env());
        assert_eq!(result.code, 2);
        assert!(server.lookups.lock().unwrap().is_empty());
    }
    #[test]
    fn test_out_of_range_phash_threshold_is_a_usage_error() {
        let root = Root::new();
        root.install();
        let server = FailureServer::new();
        let result = root.run(
            &["fetch-pack", "somepack", "--phash-threshold", "64"],
            &server.env(),
        );
        assert_eq!(result.code, 2);
        assert!(result.stderr.contains("out of range"));
        assert!(server.lookups.lock().unwrap().is_empty());
    }
    #[test]
    fn test_usable_phash_threshold_is_still_accepted() {
        let root = Root::new();
        root.install();
        let server = FailureServer::new();
        let result = root.run(
            &["fetch-pack", "somepack", "--phash-threshold", "4"],
            &server.env(),
        );
        assert_eq!(result.code, 4);
        assert_eq!(*server.lookups.lock().unwrap(), ["somepack"]);
    }
}

mod MakeEmojiPngsContracts {
    use super::*;
    #[test]
    fn test_negative_limit_is_rejected() {
        let root = Root::new();
        root.install();
        root.raster("in/a.png", [240, 20, 20, 255]);
        let result = root.run(
            &[
                "make-emoji-pngs",
                "--in",
                root.0.join("in").to_str().unwrap(),
                "--out",
                root.0.join("out").to_str().unwrap(),
                "--limit",
                "-1",
            ],
            &[],
        );
        assert_eq!(result.code, 2);
        assert!(!root.0.join("out").exists());
    }
    #[test]
    fn test_blank_svg_falls_back_to_the_healthy_raster() {
        let root = Root::new();
        root.install();
        root.file("in/foo.svg", EMPTY_SVG.as_bytes());
        root.raster("in/foo.png", [240, 20, 20, 255]);
        let result = root.run(
            &[
                "make-emoji-pngs",
                "--in",
                root.0.join("in").to_str().unwrap(),
                "--out",
                root.0.join("out").to_str().unwrap(),
            ],
            &[],
        );
        assert_eq!(result.code, 0, "{}", result.stderr);
        let image =
            runtime::native_raster::decode(&std::fs::read(root.0.join("out/foo.png")).unwrap())
                .unwrap();
        assert_eq!(image.rgba.dimensions(), (100, 100));
        let pixel = image.rgba.get_pixel(50, 50);
        assert!(pixel[0] > pixel[2]);
        assert!(!runtime::native_raster::is_blank(&image.rgba));
    }
    #[test]
    fn test_group_with_no_usable_source_still_fails() {
        let root = Root::new();
        root.install();
        root.file("in/foo.svg", EMPTY_SVG.as_bytes());
        root.raster("in/foo.png", [0, 0, 0, 0]);
        let result = root.run(
            &[
                "make-emoji-pngs",
                "--in",
                root.0.join("in").to_str().unwrap(),
                "--out",
                root.0.join("out").to_str().unwrap(),
            ],
            &[],
        );
        assert_eq!(result.code, 4);
        assert!(!root.0.join("out/foo.png").exists());
    }
    #[test]
    fn test_preferred_source_is_still_tried_first() {
        let root = Root::new();
        root.install();
        root.file("in/foo.svg",br#"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><rect width="64" height="64" fill="lime"/></svg>"#);
        root.raster("in/foo.png", [240, 20, 20, 255]);
        let result = root.run(
            &[
                "make-emoji-pngs",
                "--in",
                root.0.join("in").to_str().unwrap(),
                "--out",
                root.0.join("out").to_str().unwrap(),
            ],
            &[],
        );
        assert_eq!(result.code, 0);
        let image =
            runtime::native_raster::decode(&std::fs::read(root.0.join("out/foo.png")).unwrap())
                .unwrap();
        let pixel = image.rgba.get_pixel(50, 50);
        assert!(
            pixel[1] > pixel[0],
            "actual SVG priority must beat the healthy red fallback"
        );
    }
}

mod MakeEmojiPngsLegacyFallback {
    use super::*;
    #[test]
    fn test_blank_svg_with_healthy_png_fallback_exits_ok() {
        let root = Root::new();
        root.install();
        root.file("coins/logos/svg/btc.svg", EMPTY_SVG.as_bytes());
        root.raster("coins/logos/png/btc.png", [240, 20, 20, 255]);
        let result = root.run(&["make-emoji-pngs"], &[]);
        assert_eq!(result.code, 0, "{}", result.stderr);
        assert!(result.stdout.contains("failed=0"));
        assert_eq!(
            runtime::native_raster::decode(
                &std::fs::read(root.0.join("coins/logos/emoji/btc.png")).unwrap()
            )
            .unwrap()
            .rgba
            .dimensions(),
            (100, 100)
        );
    }
    #[test]
    fn test_stem_with_no_usable_source_anywhere_still_fails() {
        let root = Root::new();
        root.install();
        root.file("coins/logos/svg/bad.svg", EMPTY_SVG.as_bytes());
        let result = root.run(&["make-emoji-pngs"], &[]);
        assert_eq!(result.code, 4);
        assert!(!root.0.join("coins/logos/emoji/bad.png").exists());
    }
    #[test]
    fn test_blank_png_fallback_is_counted_once_not_twice() {
        let root = Root::new();
        root.install();
        root.file("coins/logos/svg/bad.svg", EMPTY_SVG.as_bytes());
        root.raster("coins/logos/png/bad.png", [0, 0, 0, 0]);
        root.raster("coins/logos/png/good.png", [240, 20, 20, 255]);
        let result = root.run(&["make-emoji-pngs"], &[]);
        assert_eq!(result.code, 3);
        assert!(result.stdout.contains("failed=1"));
    }
}

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

mod TheCliReferenceMatchesTheTools {
    use super::*;
    #[test]
    fn test_the_scan_finds_the_sections() {
        let sections = sections();
        assert_eq!(sections["12.2"].0, BTreeSet::from(["build-pack".into()]));
        assert!(sections["12.8"].0.len() >= 5);
        assert_eq!(
            sections["12.8"].0,
            [
                "coin-fetch-logos",
                "coin-build-keywords",
                "coin-fetch-paprika",
                "coin-fetch-cmc",
                "coin-rebuild",
                "coin-rebuild-map",
                "coin-remap-ids",
                "coin-verify-logos",
                "coin-check-packs",
                "coin-write-manifests",
                "coin-enhance-map",
                "coin-alias-map"
            ]
            .map(str::to_owned)
            .into_iter()
            .collect()
        );
    }
    #[test]
    fn test_every_flag_is_documented_and_every_documented_flag_exists() {
        let root = Root::new();
        root.install();
        for (id, (tools, body)) in sections() {
            if tools.is_empty() {
                continue;
            }
            let mut actual = BTreeSet::new();
            for command in tools {
                let result = root.run(&[&command, "--help"], &[]);
                assert_eq!(result.code, 0, "{command}: {}", result.stderr);
                actual.extend(flags(&result.stdout));
            }
            actual.remove("--help");
            let mut documented = flags(&body);
            documented.remove("--help");
            let mentioned = match id.as_str() {
                "12.6" => vec!["--new-set", "--per-set", "--source"],
                "12.5e" => vec!["--check"],
                "12.5d" => vec!["--refresh"],
                _ => vec![],
            }
            .into_iter()
            .map(str::to_owned)
            .collect::<BTreeSet<_>>();
            assert_eq!(
                actual.difference(&documented).collect::<Vec<_>>(),
                Vec::<&String>::new(),
                "§{id} undocumented flags"
            );
            assert!(
                documented
                    .difference(&actual)
                    .all(|flag| mentioned.contains(flag)),
                "§{id} stale flags: {:?}",
                documented.difference(&actual).collect::<Vec<_>>()
            );
        }
    }
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
