//! Interpreter-free coverage of historical launcher, environment and sandbox contracts.
//! Expected requests are literal public CLI arguments or pinned independent source captures.
use numera_runtime::{tooling_doctor, tooling_environment, tooling_launch, tooling_menu};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Root(PathBuf);
impl Root {
    fn new(label: &str) -> Self {
        let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let base = tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
        let path = base.join(format!(
            "standalone {label} café {} {}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self(tooling_environment::physical_root(&path).unwrap())
    }

    fn state(&self, relative: &str, bytes: &[u8]) {
        let path = self.0.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    fn session(&self, input: &str, codes: &[i32]) -> (Vec<tooling_menu::Request>, String) {
        let mut requests = Vec::new();
        let mut output = Vec::new();
        tooling_menu::run(
            &self.0,
            &mut Cursor::new(input.as_bytes()),
            &mut output,
            false,
            |request| {
                assert!(requests.len() < 16, "launcher repeated an action");
                let code = *codes.get(requests.len()).unwrap_or(&0);
                requests.push(request.clone());
                Ok(code)
            },
        )
        .unwrap();
        (requests, String::from_utf8(output).unwrap())
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("tooling fixture retained: {}", self.0.display());
            return;
        }
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

fn command(values: &[&str]) -> tooling_menu::Request {
    tooling_menu::Request::Command(values.iter().map(|value| (*value).into()).collect())
}

fn map(values: &[(&str, &str)]) -> BTreeMap<String, String> {
    values
        .iter()
        .map(|(key, value)| ((*key).into(), (*value).into()))
        .collect()
}

#[test]
fn build_environment_targets_the_standalone_crate_and_keeps_all_caches_local() {
    let root = Root::new("build environment");
    let original = map(&[
        ("PATH", "machine tools"),
        ("CARGO_TARGET_DIR", "foreign target"),
    ]);
    let before = original.clone();
    let child = tooling_environment::environment(&root.0, &original, true).unwrap();
    assert_eq!(
        child["CARGO_TARGET_DIR"],
        root.0.join("runtime/target").to_str().unwrap()
    );
    for (key, relative) in [
        ("TEMP", "logs/tmp"),
        ("TMP", "logs/tmp"),
        ("TMPDIR", "logs/tmp"),
        ("XDG_CACHE_HOME", ".cache"),
        ("CARGO_HOME", ".cache/cargo"),
        ("RUSTUP_HOME", ".cache/rustup"),
        ("CCACHE_DIR", ".cache/ccache"),
        ("SCCACHE_DIR", ".cache/sccache"),
        ("HOMEBREW_CACHE", ".cache/homebrew"),
        ("HOMEBREW_TEMP", "logs/tmp"),
        ("npm_config_cache", ".cache/npm"),
    ] {
        assert_eq!(child[key], root.0.join(relative).to_str().unwrap());
        assert!(root.0.join(relative).is_dir(), "missing {relative}");
    }
    assert_eq!(child["PATH"], "machine tools");
    assert_eq!(original, before);
    assert!(!root.0.join("native").exists());
}

#[test]
fn runtime_environment_scrubs_interpreter_settings_without_mutating_the_caller() {
    let root = Root::new("runtime environment");
    let original = map(&[
        ("PATH", "machine tools"),
        ("SystemRoot", "machine root"),
        ("PYTHONHOME", "unusable interpreter"),
        ("pythonpath", "foreign modules"),
        ("PYO3_PYTHON", "foreign interpreter"),
        ("PIP_CACHE_DIR", "foreign cache"),
        ("VIRTUAL_ENV", "foreign venv"),
        ("__PYVENV_LAUNCHER__", "foreign launcher"),
        ("KEEP", "café 😀"),
    ]);
    let before = original.clone();
    let child = tooling_environment::environment(&root.0, &original, false).unwrap();
    assert_eq!(child.len(), 7);
    assert_eq!(child["PATH"], "machine tools");
    assert_eq!(child["SystemRoot"], "machine root");
    assert_eq!(child["KEEP"], "café 😀");
    for key in ["TEMP", "TMP", "TMPDIR"] {
        assert_eq!(child[key], root.0.join("logs/tmp").to_str().unwrap());
    }
    assert_eq!(
        child["XDG_CACHE_HOME"],
        root.0.join(".cache").to_str().unwrap()
    );
    assert_eq!(original, before);
    assert!(!root.0.join("runtime").exists());
}

#[test]
fn storage_refuses_cross_platform_traversal_before_creating_anything() {
    let root = Root::new("traversal");
    for relative in [
        "../escape",
        "safe/../../escape",
        "safe\\..\\escape",
        "/absolute",
        "C:\\foreign",
        "safe:foreign",
    ] {
        assert!(
            tooling_environment::confined_directory(&root.0, relative, true).is_err(),
            "accepted {relative}"
        );
    }
    assert_eq!(std::fs::read_dir(&root.0).unwrap().count(), 0);
    let expected = root.0.join("missing/child");
    assert_eq!(
        tooling_environment::confined_directory(&root.0, "missing/child", false).unwrap(),
        expected
    );
    assert!(!expected.exists());
    assert_eq!(
        tooling_environment::confined_directory(&root.0, "missing/child", true).unwrap(),
        expected
    );
    assert!(expected.is_dir());
}

#[test]
fn linked_cache_refuses_the_entire_environment_before_creating_temp_storage() {
    let root = Root::new("linked cache");
    let outside = Root::new("outside cache");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&outside.0, root.0.join(".cache")).unwrap();
    #[cfg(windows)]
    {
        // Exercise a real reparse point without requiring symlink privileges.
        junction::create(&outside.0, root.0.join(".cache")).unwrap();
    }
    assert!(tooling_environment::environment(&root.0, &BTreeMap::new(), false).is_err());
    assert!(!root.0.join("logs").exists());
    assert_eq!(std::fs::read_dir(&outside.0).unwrap().count(), 0);
    #[cfg(unix)]
    std::fs::remove_file(root.0.join(".cache")).unwrap();
    #[cfg(windows)]
    std::fs::remove_dir(root.0.join(".cache")).unwrap();
}

#[test]
fn sandbox_scrub_and_arguments_match_independent_pinned_source() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/tooling-sandbox-source.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["sourceHash"],
        "a37b7609521a78ebca9a6d0a2d1dae7b70fccce2fa50af8d5a16f2d60dfef2ae"
    );
    let original = fixture["original"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(key, value)| (key.clone(), value.as_str().unwrap().into()))
        .collect();
    assert_eq!(
        serde_json::to_value(tooling_launch::scrubbed_environment(
            &original,
            fixture["template"].as_str().unwrap()
        ))
        .unwrap(),
        fixture["scrubbed"]
    );
    let cases = fixture["arguments"].as_array().unwrap();
    assert_eq!(cases.len(), 5);
    for case in cases {
        let input = case["input"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().into())
            .collect::<Vec<_>>();
        let (source, options) = tooling_launch::sandbox_options(&input, 9450).unwrap();
        assert_eq!(source, Path::new("collection"));
        assert_eq!(options.port, case["port"].as_i64().unwrap());
        assert_eq!(options.all, case["all"].as_bool().unwrap());
        assert_eq!(
            serde_json::to_value(&options.with_pack).unwrap(),
            case["with_pack"]
        );
        assert_eq!(options.bot_username, case["bot_username"].as_str().unwrap());
        assert_eq!(
            serde_json::to_value(
                &tooling_launch::panel_arguments(&options, Path::new("$CLONE"), 9450).unwrap()[1..]
            )
            .unwrap(),
            case["expected"]
        );
    }
}

#[test]
fn sandbox_refuses_real_port_forwarded_data_directory_and_abbreviated_options() {
    for values in [
        vec!["--data-dir", "collection"],
        vec!["--sour", "collection"],
        vec!["--port", "9450"],
        vec!["--port", "0"],
        vec!["--port", "65536"],
        vec!["--port", "bad"],
        vec!["--port"],
    ] {
        let args = values.into_iter().map(str::to_owned).collect::<Vec<_>>();
        assert!(
            tooling_launch::sandbox_options(&args, 9450).is_err(),
            "accepted {args:?}"
        );
    }
    let (_, options) = tooling_launch::sandbox_options(&[], 12000).unwrap();
    assert_eq!(options.port, 12001);
    let args =
        tooling_launch::panel_arguments(&options, Path::new("clone café with spaces"), 12000)
            .unwrap();
    assert_eq!(
        args,
        [
            "panel",
            "--data-dir",
            "clone café with spaces",
            "--port",
            "12001",
            "--no-open"
        ]
    );
}

#[test]
fn standalone_launch_forwards_help_and_settles_the_child_with_no_interpreter_or_path() {
    let root = Root::new("native launch");
    // Exercise an installed companion independently of Cargo's test-artifact
    // directory layout; the fixture contains a real production executable.
    std::fs::copy(
        env!("CARGO_BIN_EXE_numera-emoji"),
        root.0.join(if cfg!(windows) {
            "numera-emoji.exe"
        } else {
            "numera-emoji"
        }),
    )
    .unwrap();
    let original = map(&[
        ("PATH", ""),
        ("PYTHONHOME", "unusable interpreter"),
        ("PYO3_PYTHON", "unusable interpreter"),
        ("VIRTUAL_ENV", "unusable interpreter"),
    ]);
    #[cfg(windows)]
    let original = {
        let mut environment = original;
        environment.insert("SystemRoot".into(), std::env::var("SystemRoot").unwrap());
        environment
    };
    let before = original.clone();
    let launch = tooling_launch::plan(&root.0, &["--help".into()], &original).unwrap();
    assert_eq!(launch.root, root.0);
    assert_eq!(launch.argv.len(), 2);
    assert_eq!(launch.argv[1], "--help");
    assert_eq!(
        Path::new(&launch.argv[0]).file_name().unwrap(),
        if cfg!(windows) {
            "numera-emoji.exe"
        } else {
            "numera-emoji"
        }
    );
    assert_eq!(launch.environment["PATH"], "");
    assert!(!launch.environment.contains_key("PYTHONHOME"));
    assert!(!launch.environment.contains_key("PYO3_PYTHON"));
    assert!(!launch.environment.contains_key("VIRTUAL_ENV"));
    assert_eq!(original, before);
    let log = numera_runtime::logging::RunLog::open(&root.0, "standalone-launch-test", vec![]);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let result = runtime
        .block_on(tooling_launch::run(
            &launch,
            std::time::Duration::from_secs(10),
            std::time::Duration::from_secs(5),
            &log,
        ))
        .unwrap();
    assert_eq!(result.code, 0, "{}", result.stderr);
    assert!(result.stdout.contains("Usage: numera-emoji COMMAND"));
    assert!(result.stdout.contains("panel doctor menu check"));
    assert_eq!(
        std::fs::read_dir(root.0.join("logs/tmp/tooling"))
            .unwrap()
            .count(),
        0
    );
    drop(log);
}

#[test]
fn original_menu_keys_quit_and_eof_never_start_a_command() {
    let root = Root::new("menu quit");
    for input in ["exit\n", "q\n", "0\n", ""] {
        let (calls, output) = root.session(input, &[]);
        assert!(calls.is_empty());
        for key in [
            "A1", "A2", "A3", "B0", "B1", "B2", "B3", "B4", "B5", "C1", "D1", "D2", "D3", "D4",
        ] {
            assert!(output.contains(key), "missing {key}");
        }
        assert!(output.contains("Bye."));
        assert!(!output.contains('\u{1b}'));
    }
}

#[test]
fn conversion_wizard_back_reasks_one_field_and_keeps_paths_as_single_arguments() {
    let root = Root::new("menu conversion");
    std::fs::create_dir(root.0.join("input with spaces")).unwrap();
    let (calls, output) = root.session(
        "a2\ninput with spaces\n0\ninput with spaces\noutput café\nexit\n",
        &[],
    );
    assert_eq!(
        calls,
        [command(&[
            "make-emoji-pngs",
            "--in",
            "input with spaces",
            "--out",
            "output café"
        ])]
    );
    assert!(output.contains("Conversion finished."));
    let (calls, _) = root.session("a2\ninput with spaces\n\nexit\n", &[]);
    assert_eq!(
        calls,
        [command(&["make-emoji-pngs", "--in", "input with spaces"])]
    );
}

#[test]
fn general_build_stops_after_each_failed_step_and_never_skips_dry_run() {
    let root = Root::new("menu build");
    std::fs::create_dir(root.0.join("input with spaces")).unwrap();
    for (codes, expected) in [(&[1][..], 1), (&[0, 2][..], 2), (&[0, 0, 3][..], 3)] {
        let (calls, output) =
            root.session("a1\ninput with spaces\nmyset\nMy Emoji\n\n\nexit\n", codes);
        assert_eq!(calls.len(), expected);
        assert_eq!(
            calls[0],
            command(&[
                "make-emoji-pngs",
                "--in",
                "input with spaces",
                "--out",
                "build/myset"
            ])
        );
        if expected > 1 {
            assert_eq!(
                calls[1],
                command(&[
                    "build-pack",
                    "--base",
                    "myset",
                    "--title",
                    "My Emoji",
                    "--source-dir",
                    "build/myset",
                    "--token-env",
                    "GENERAL_BOT_TOKEN",
                    "--emoji",
                    "😀",
                    "--dry-run"
                ])
            );
        }
        if expected == 3 {
            assert_eq!(
                calls[2],
                command(&[
                    "build-pack",
                    "--base",
                    "myset",
                    "--title",
                    "My Emoji",
                    "--source-dir",
                    "build/myset",
                    "--token-env",
                    "GENERAL_BOT_TOKEN",
                    "--emoji",
                    "😀"
                ])
            );
            assert!(output.contains("partially finished"));
        }
    }
    assert!(
        root.session("a1\ninput with spaces\nmyset\nMy Emoji\n\nn\nexit\n", &[])
            .0
            .is_empty()
    );
}

#[test]
fn collection_keeps_recorded_mode_and_requires_dry_run_then_preflight() {
    let root = Root::new("menu collection");
    root.state(
        "collection/publish_myset.json",
        br#"{"sets":[{"fmt":"mixed","index":1}]}"#,
    );
    let (calls, _) = root.session("b3\nmyset\nMy Set\n\n\nexit\n", &[0, 0, 0]);
    assert_eq!(calls.len(), 3);
    for (index, call) in calls.iter().enumerate() {
        let tooling_menu::Request::Command(args) = call else {
            panic!("command expected")
        };
        assert_eq!(
            &args[..7],
            [
                "build-collection",
                "--base",
                "myset",
                "--title",
                "My Set",
                "--token-env",
                "GENERAL_BOT_TOKEN"
            ]
        );
        assert!(args.contains(&"--mixed".into()));
        assert_eq!(args.contains(&"--dry-run".into()), index == 0);
        assert_eq!(args.contains(&"--preflight".into()), index == 1);
    }
    let (calls, output) = root.session("b3\nmyset\nMy Set\n\n\nexit\n", &[0, 4]);
    assert_eq!(calls.len(), 2);
    assert!(output.contains("Nothing was published"));
    root.state(
        "collection/publish_myset.json",
        br#"{"sets":[{"fmt":"mixed"},{"fmt":"static"}]}"#,
    );
    let (calls, output) = root.session("b3\nmyset\nMy Set\n\nexit\n", &[]);
    assert!(calls.is_empty());
    assert!(output.contains("ambiguous"));
}

#[test]
fn destructive_rebuild_reorder_and_archive_default_to_no() {
    let root = Root::new("menu destructive consent");
    root.state("coins/rebuild_state.json", br#"{"sets":[{},{}]}"#);
    let (calls, output) = root.session("a3\n\nb5\nmyset\n\n\nd4\n\nexit\n", &[0, 3]);
    assert_eq!(
        calls,
        [
            command(&["sync-order", "--base", "myset"]),
            command(&["pack-archive", "--check"])
        ]
    );
    assert!(output.contains("DELETES the 2 pack(s)"));
    let (calls, _) = root.session("a3\ny\nb5\nmyset\n2\ny\nd4\ny\nexit\n", &[0, 0, 0, 3, 0]);
    assert_eq!(
        calls,
        [
            command(&["coin-rebuild"]),
            command(&["sync-order", "--base", "myset", "--pack", "2"]),
            command(&["sync-order", "--base", "myset", "--pack", "2", "--apply"]),
            command(&["pack-archive", "--check"]),
            command(&["pack-archive", "--sync"])
        ]
    );
    assert_eq!(root.session("b5\nmyset\n\ny\nexit\n", &[4]).0.len(), 1);
    assert!(
        root.session("a3\n", &[]).0.is_empty(),
        "EOF accepted consent"
    );
}

#[test]
fn collection_lists_undo_one_entry_without_duplicating_it() {
    let root = Root::new("menu undo");
    let (calls, _) = root.session(
        "b1\nremoved\n0\nkept pack\n\n\nb0\n111111111\n0\n222222222\n\nexit\n",
        &[],
    );
    assert_eq!(
        calls,
        [
            command(&[
                "fetch-pack",
                "kept pack",
                "--token-env",
                "GENERAL_BOT_TOKEN"
            ]),
            command(&["fetch-emoji-ids", "--id", "222222222"])
        ]
    );
    let (calls, output) = root.session("b0\n111111111\n0\n\nexit\n", &[]);
    assert!(calls.is_empty());
    assert!(output.contains("No ids entered"));
}

#[test]
fn panel_roster_status_bot_checks_and_local_ingest_keep_their_distinct_requests() {
    let root = Root::new("menu routing");
    std::fs::create_dir(root.0.join("input with spaces")).unwrap();
    root.state(
        "collection/publish_first.json",
        br#"{"sets":[{"index":2},{"index":1},{"index":2}]}"#,
    );
    let (calls, _) = root.session(
        "b4\ny\nd3\n\nd2\nc1\nd1\nb2\ninput with spaces\n\nexit\n",
        &[0, 3, 0, 3, 0, 0, 0],
    );
    assert_eq!(
        calls,
        [
            command(&["panel", "--with-pack", "1", "--with-pack", "2"]),
            command(&["pack-manifest", "--check"]),
            command(&["pack-manifest", "--refresh"]),
            command(&["status"]),
            command(&["emoji-bot"]),
            tooling_menu::Request::Checks,
            command(&["add-media", "--in", "input with spaces", "--emoji", "😀"])
        ]
    );
}

#[test]
fn invalid_utf8_oversized_input_and_executor_errors_are_not_silent_success() {
    let root = Root::new("menu invalid input");
    for bytes in [
        vec![255, b'\n'],
        vec![b'a'; 16 * 1024 + 1],
        b"d2\x1b[2J\n".to_vec(),
    ] {
        assert!(
            tooling_menu::run(
                &root.0,
                &mut Cursor::new(bytes),
                &mut Vec::new(),
                false,
                |_| panic!("invalid input executed")
            )
            .is_err()
        );
    }
    let mut calls = 0;
    let mut output = Vec::new();
    let result = tooling_menu::run(
        &root.0,
        &mut Cursor::new(b"d2\nd2\nexit\n"),
        &mut output,
        false,
        |_| {
            calls += 1;
            Err("synthetic execution refusal".into())
        },
    );
    assert_eq!(calls, 1);
    assert_eq!(result.unwrap_err(), "synthetic execution refusal");
    assert!(!String::from_utf8(output).unwrap().contains("Bye."));
    assert_eq!(std::fs::read_dir(&root.0).unwrap().count(), 0);
}

#[test]
fn doctor_reports_only_unset_valid_unique_template_keys() {
    let keys = tooling_doctor::unset_keys(
        "# OMIT=\n SET =ignored\nMISSING=\nMISSING=again\nnot a key=\n=empty\nNO_EQUALS\nALSO_MISSING=\n",
        |key| key == "SET",
    );
    assert_eq!(keys, ["ALSO_MISSING", "MISSING"]);
    assert_eq!(
        tooling_doctor::unset_keys("A=\n", |_| true),
        Vec::<String>::new()
    );
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
