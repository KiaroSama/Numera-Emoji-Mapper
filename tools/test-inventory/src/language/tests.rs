use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
pub(super) struct Tree {
    pub(super) root: PathBuf,
    pub(super) policy: Policy,
}
impl Tree {
    pub(super) fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "numera-language-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let result = std::process::Command::new("git")
            .args(["init", "--quiet"])
            .arg(&root)
            .output()
            .unwrap();
        assert!(result.status.success());
        let mut tree = Self {
            root,
            policy: Policy {
                schema: 1,
                vendor: Vendor {
                    root: "runtime/vendor/rlottie".into(),
                    upstream: "https://github.com/laggykiller/rlottie".into(),
                    revision: "47b71a7e9072ac67b2e18c408d3668fed278fb93".into(),
                    provenance: "runtime/vendor/rlottie/NUMERA-SOURCE.md".into(),
                    notice: "THIRD_PARTY_NOTICES.md".into(),
                    files: BTreeMap::new(),
                },
                frozen_data: BTreeMap::new(),
                launchers: BTreeMap::new(),
            },
        };
        for name in [
            "COPYING",
            "NUMERA-SOURCE.md",
            "licenses/COPYING.MIT",
            "licenses/COPYING.MPL",
            "licenses/COPYING.SKIA",
            "licenses/COPYING.FTL",
            "licenses/COPYING.PIX",
            "licenses/COPYING.RPD",
            "licenses/COPYING.STB",
            "src/renderer.cpp",
        ] {
            let path = format!("runtime/vendor/rlottie/{name}");
            let bytes = if name == "NUMERA-SOURCE.md" {
                b"https://github.com/laggykiller/rlottie\n47b71a7e9072ac67b2e18c408d3668fed278fb93\n".as_slice()
            } else {
                b"// approved third-party fixture\n".as_slice()
            };
            tree.put(&path, bytes);
            tree.policy.vendor.files.insert(path, digest(bytes));
        }
        tree.put(
            "THIRD_PARTY_NOTICES.md",
            b"# Notices\nrlottie 47b71a7e9072ac67b2e18c408d3668fed278fb93\n",
        );
        tree
    }
    pub(super) fn put(&self, path: &str, bytes: &[u8]) {
        let file = self.root.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, bytes).unwrap();
    }
    pub(super) fn run(&self) -> Report {
        check(
            &self.root,
            &self.policy,
            &git_candidates(&self.root).unwrap(),
        )
    }
    fn rule(&self, name: &str) -> bool {
        self.run().findings.iter().any(|f| f.rule == name)
    }
}
impl Drop for Tree {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}
#[test]
fn real_candidate_tree_accepts_rust_browser_worker_and_declarative_data() {
    let tree = Tree::new();
    for (file, text) in [
        ("runtime/src/main.rs", "fn main() {}"),
        ("tools/ci/src/main.rs", "fn main() {}"),
        ("assets/panel-grid.js", "export const x = 1;"),
        (
            "assets/tests/page.spec.js",
            "import { test } from '@playwright/test';",
        ),
        ("assets/panel.html", "<!doctype html><main></main>"),
        ("assets/panel.css", "main { color: red; }"),
        (
            "assets/panel-globals.d.ts",
            "interface Window { fixture: string; }",
        ),
        (
            "worker/src/index.ts",
            "export default { fetch(): Response { return new Response('ok'); } };",
        ),
        ("worker/test/input.test.ts", "const x: number = 1;"),
        (
            "worker/migrations/001.sql",
            "CREATE TABLE messages(id INTEGER);",
        ),
        ("runtime/data/python311-decimal.json", "[[48,57]]"),
        (
            "tests/fixtures/receipt.json",
            "{\"source\":\"tests/test_original.py\",\"sha256\":\"abc\"}",
        ),
        ("coins/keywords.csv", "ticker,keywords\nbtc,coin"),
        ("runtime/Cargo.toml", "[package]\nname='fixture'\n"),
        (
            "runtime/tests/fixture.rs.in",
            "#[test] fn actual_rust_fixture() { assert_eq!(1, 1); }",
        ),
        (
            "tests/fixtures/source.sha256",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa  tests/original.py\n",
        ),
        (
            "assets/package.json",
            "{\"scripts\":{\"test\":\"playwright test\"}}",
        ),
        (
            "worker/package.json",
            "{\"scripts\":{\"test\":\"vitest run\"}}",
        ),
        (
            "runtime/licenses/notices.md.hbs",
            "# Notices\n{{#each crates}}\n{{package.name}}\n{{/each}}\n",
        ),
    ] {
        tree.put(file, text.as_bytes());
    }
    let report = tree.run();
    assert!(report.findings.is_empty(), "{:?}", report.findings);
    assert_eq!(report.categories["first-party-rust"], 3);
    assert!(!report.proves_behavioral_retirement);
}
#[test]
fn legacy_extensions_are_rejected_even_inside_fixture_browser_or_worker_directories() {
    let tree = Tree::new();
    for file in [
        "runtime/main.py",
        "assets/test.ps1",
        "worker/scripts/deploy.sh",
        "tests/fixtures/example.py",
        "tools/check.cmd",
    ] {
        tree.put(file, b"# intentionally forbidden source\n");
    }
    let report = tree.run();
    assert_eq!(
        report
            .findings
            .iter()
            .filter(|f| f.rule == "first-party-language")
            .count(),
        5
    );
}
#[test]
fn reviewed_windows_launchers_require_exact_digest_receipts() {
    let mut tree = Tree::new();
    let script = b"#Requires -Version 7.0\n& $binary menu\n";
    tree.put("run.ps1", script);
    tree.policy
        .launchers
        .insert("run.ps1".into(), digest(script));
    let ok = tree.run();
    assert!(ok.findings.is_empty(), "{:?}", ok.findings);
    assert_eq!(ok.categories["windows-launcher"], 1);

    tree.put("run.ps1", b"#Requires -Version 7.0\n& $binary doctor\n");
    assert!(tree.rule("windows-launcher"));

    tree.put("extra.ps1", script);
    assert!(tree.rule("first-party-language"));
}
#[test]
fn renamed_python_powershell_and_shell_programs_are_not_treated_as_data() {
    let tree = Tree::new();
    for (file, text) in [
        (
            "tests/fixtures/oracle.json",
            "from pathlib import Path\ndef main():\n    pass\n",
        ),
        ("tools/launcher.txt", "param($Token)\nWrite-Host $Token"),
        ("runtime/payload.data", "#!/bin/sh\necho application\n"),
        (
            "docs/renamed.md",
            "# old app\nimport os\ndef main():\n    pass\n",
        ),
    ] {
        tree.put(file, text.as_bytes());
    }
    let report = tree.run();
    for file in [
        "tests/fixtures/oracle.json",
        "tools/launcher.txt",
        "runtime/payload.data",
        "docs/renamed.md",
    ] {
        assert!(report.findings.iter().any(|f| f.path == file), "{file}");
    }
    tree.put("runtime/template.rs.in", b"import os\nprint('hidden app')");
    tree.put(
        "tests/fixtures/payload.sha256",
        b"#!/bin/sh\necho hidden app\n",
    );
    assert!(tree.rule("rust-source"));
    assert!(tree.rule("interpreter-shebang"));
}
#[test]
fn executable_json_payload_is_refused_but_exact_reviewed_parser_fixture_is_preserved() {
    let mut tree = Tree::new();
    for source in [
        r#"{"script":"$name = 'synthetic'; Write-Host $name"}"#,
        r#"{"script":"printf synthetic"}"#,
        r#"{"script":"exec('print(1)')"}"#,
    ] {
        tree.put("tests/fixtures/parser.json", source.as_bytes());
        assert!(tree.rule("json-program"), "{source}");
    }
    let source = br#"{"source":"import os\ndef fixture():\n    pass\n"}"#;
    tree.put("tests/fixtures/parser.json", source);
    assert!(tree.rule("json-program"));
    tree.policy
        .frozen_data
        .insert("tests/fixtures/parser.json".into(), digest(source));
    assert!(tree.run().findings.is_empty());
    tree.put(
        "tests/fixtures/parser.json",
        br#"{"source":"import subprocess\ndef application():\n    pass\n"}"#,
    );
    assert!(tree.rule("frozen-data"));
}
#[test]
fn frozen_corrupt_media_and_nonfinite_json_remain_test_data_without_an_execution_exception() {
    let mut tree = Tree::new();
    for (path, bytes) in [
        ("tests/fixtures/nonfinite.json", b"{\"x\":NaN}".as_slice()),
        ("tests/fixtures/corrupt.tgs", b"not gzip".as_slice()),
    ] {
        tree.put(path, bytes);
        tree.policy.frozen_data.insert(path.into(), digest(bytes));
    }
    assert!(tree.run().findings.is_empty());
    tree.put("tests/fixtures/disguised.py", b"print('application')");
    tree.policy.frozen_data.insert(
        "tests/fixtures/disguised.py".into(),
        digest(b"print('application')"),
    );
    assert!(tree.rule("first-party-language"));
}
#[test]
fn documentation_code_fences_do_not_become_active_script_oracle_files() {
    let tree = Tree::new();
    tree.put(
        "README.md",
        b"# Docs\n```python\nimport os\ndef example():\n    pass\n```\nPlain explanation.\n",
    );
    assert!(tree.run().findings.is_empty());
    tree.put(
        "docs/raw.md",
        b"# Looks like docs\nfrom pathlib import Path\n",
    );
    assert!(tree.rule("disguised-document"));
}
#[test]
fn real_rust_interpreter_handoffs_and_bindings_fail_but_quoted_rejection_examples_do_not() {
    let tree = Tree::new();
    tree.put("runtime/src/good.rs",br##"fn fixture(){ let historical="tests/reference/catalog.py";let rejected=r#"use pyo3::Python; Command::new(\"python\")"#; assert!(!historical.is_empty() && !rejected.is_empty()); }"##);
    assert!(tree.run().findings.is_empty());
    tree.put("runtime/src/bad.rs",b"use std::process::Command as Process;fn main(){let executable=\"python3\";let _=Process::new(executable);}");
    assert!(tree.rule("rust-interpreter-handoff"));
    tree.put("runtime/src/bad.rs", b"use pyo3::prelude::*; fn main(){} ");
    assert!(tree.rule("rust-interpreter-handoff"));
    tree.put(
        "runtime/Cargo.toml",
        b"[target.'cfg(windows)'.dependencies]\nbridge={package='pyo3',version='1'}\n",
    );
    assert!(tree.rule("configuration-launcher"));
    tree.put(
        "runtime/Cargo.toml",
        b"[package.metadata]\nold_binding='pyo3'\n",
    );
    assert!(!tree.rule("configuration-launcher"));
}
#[test]
fn indexed_argument_vectors_and_owned_capture_cannot_hide_an_interpreter() {
    let tree = Tree::new();
    tree.put("runtime/src/main.rs",b"fn main(){let argv=vec![\"powershell.exe\",\"-File\",\"payload.txt\"];let _=std::process::Command::new(&argv[0]);}");
    assert!(tree.rule("rust-interpreter-handoff"));
    tree.put("runtime/src/main.rs",b"fn main(){let argv=[\"python\",\"oracle.data\"];tooling_process::capture(root,&argv,env,wall,idle,log);}");
    assert!(tree.rule("rust-interpreter-handoff"));
}
#[test]
fn real_const_static_and_command_type_aliases_cannot_hide_a_retired_interpreter() {
    let tree = Tree::new();
    for source in [
        "const EXEC:&str=\"python3\";fn main(){let _=std::process::Command::new(EXEC);}",
        "fn main(){let _=std::process::Command::new(EXEC);}static EXEC:&str=\"powershell.exe\";",
        "type Runner=std::process::Command;fn main(){let _=Runner::new(\"python3\");}",
        "use std::process::Command as Process;type Runner=Process;fn main(){const EXEC:&str=\"sh\";let _=Runner::new(EXEC);}",
        "const ARGV:[&str;2]=[\"python\",\"oracle.data\"];fn main(){tooling_process::capture(root,&ARGV,env,wall,idle,log);}",
    ] {
        tree.put("runtime/src/main.rs", source.as_bytes());
        assert!(tree.rule("rust-interpreter-handoff"), "{source}");
    }
    tree.put("runtime/src/main.rs",b"const EXEC:&str=\"historical python path\";type Runner=std::process::Command;fn main(){let EXEC=\"cargo\";let _=Runner::new(EXEC);}");
    assert!(tree.run().findings.is_empty());
}
#[test]
fn vendor_is_an_exact_licensed_subset_not_a_directory_wide_language_bypass() {
    let tree = Tree::new();
    assert!(tree.run().findings.is_empty());
    tree.put(
        "runtime/vendor/rlottie/launcher.py",
        b"print('application')",
    );
    assert!(tree.rule("vendor-receipt"));
    std::fs::remove_file(tree.root.join("runtime/vendor/rlottie/launcher.py")).unwrap();
    tree.put(
        "runtime/vendor/rlottie/src/renderer.cpp",
        b"// changed source",
    );
    assert!(tree.rule("vendor-receipt"));
}
#[test]
fn missing_native_license_or_real_redistribution_notice_is_rejected() {
    let tree = Tree::new();
    std::fs::remove_file(tree.root.join("runtime/vendor/rlottie/COPYING")).unwrap();
    assert!(tree.rule("vendor-receipt"));
    tree.put("THIRD_PARTY_NOTICES.md", b"# no vendor notice\n");
    assert!(tree.rule("vendor-notice"));
}
#[test]
fn browser_and_worker_exceptions_do_not_allow_unrelated_first_party_javascript_or_typescript() {
    let tree = Tree::new();
    for path in [
        "tools/helper.js",
        "runtime/src/logic.ts",
        "worker/scripts/admin.ts",
        "assets/backend.ts",
    ] {
        tree.put(path, b"const x = 1;\n");
    }
    assert_eq!(
        tree.run()
            .findings
            .iter()
            .filter(|f| f.rule == "unclassified-artifact")
            .count(),
        4
    );
}
#[test]
fn declarative_node_and_ci_configuration_cannot_launch_old_project_scripts() {
    let tree = Tree::new();
    tree.put(
        "assets/package.json",
        br#"{"scripts":{"test":"python tests/oracle.data"}}"#,
    );
    assert!(tree.rule("json-launcher"));
    tree.put(
        "assets/package.json",
        br#"{"scripts":{"test":"playwright test"}}"#,
    );
    tree.put(
        ".github/workflows/ci.yml",
        b"name: CI\njobs:\n  check:\n    steps:\n      - run: |\n          python3 oracle.txt\n",
    );
    assert!(tree.rule("configuration-launcher"));
    tree.put(".github/workflows/ci.yml",b"name: CI\njobs:\n  check:\n    steps:\n      - shell: pwsh\n        run: |\n          rustup default 1.100.0\n      - shell: bash\n        run: cargo test --locked\n");
    assert!(tree.run().findings.is_empty());
}
#[test]
fn ignored_private_files_are_not_candidates_and_published_private_paths_are_not_read() {
    let tree = Tree::new();
    tree.put(".gitignore", b"private/\n.env\n");
    tree.put("private/secret.txt", b"not real credentials");
    tree.put(".env", b"not real credentials");
    assert!(tree.run().findings.is_empty());
    let report = check(
        &tree.root,
        &tree.policy,
        &[Candidate {
            path: ".env".into(),
            executable: false,
            git_link: false,
        }],
    );
    assert!(
        report
            .findings
            .iter()
            .any(|f| f.rule == "private-candidate")
    );
    let report = check(
        &tree.root,
        &tree.policy,
        &[Candidate {
            path: "../outside.json".into(),
            executable: false,
            git_link: false,
        }],
    );
    assert!(report.findings.iter().any(|f| f.rule == "physical-file"));
}
#[test]
#[cfg_attr(not(unix), ignore = "Unix symlink fixture permissions")]
fn linked_candidate_cannot_escape_into_an_unread_private_tree() {
    #[cfg(unix)]
    {
        let tree = Tree::new();
        tree.put("outside-private.txt", b"not real credentials");
        std::os::unix::fs::symlink(
            tree.root.join("outside-private.txt"),
            tree.root.join("README.md"),
        )
        .unwrap();
        assert!(tree.rule("physical-file"));
    }
}
