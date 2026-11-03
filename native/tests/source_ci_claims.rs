//! Source CI ownership canaries parse literals only, then validate real workflow run blocks.
use _native::source_ci_claims::{self, Claims};
use _native::source_git_inventory::Source;
use sha2::{Digest, Sha256};

fn source(name: &str, text: &str) -> Source {
    Source {
        baseline_revision: "1a36a968b006236bd0665a1da65c541938c06061".into(),
        path: format!("tests/test_{name}.py"),
        sha256: Sha256::digest(text.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        text: text.into(),
    }
}

#[test]
fn source_suite_claims_distinguish_real_imports_top_level_true_and_windows_skips() {
    let fixtures = [
        (
            "from tests._panel_browser_fixtures import Browser\n",
            true,
            false,
            false,
        ),
        (
            "from tests import _panel_browser_fixtures as f\n",
            true,
            false,
            false,
        ),
        (
            "import tests._panel_browser_fixtures as f\n",
            true,
            false,
            false,
        ),
        (
            "def f():\n    from tests._panel_browser_fixtures import Browser\n",
            true,
            false,
            false,
        ),
        (
            "# import tests._panel_browser_fixtures\nx = '_panel_browser_fixtures'\n",
            false,
            false,
            false,
        ),
        ("RUNS_ON_NATIVE_WINDOWS = True\n", false, true, false),
        (
            "other = RUNS_ON_NATIVE_WINDOWS = True\n",
            false,
            true,
            false,
        ),
        ("RUNS_ON_NATIVE_WINDOWS = 1\n", false, false, false),
        (
            "RUNS_ON_NATIVE_WINDOWS = False\nRUNS_ON_NATIVE_WINDOWS = True\n",
            false,
            false,
            false,
        ),
        (
            "def f():\n    RUNS_ON_NATIVE_WINDOWS = True\n",
            false,
            false,
            false,
        ),
        (
            "# RUNS_ON_NATIVE_WINDOWS = True\nx = 'RUNS_ON_NATIVE_WINDOWS = True'\n",
            false,
            false,
            false,
        ),
        (
            "if sys.platform != 'win32':\n    self.skipTest('Windows')\n",
            false,
            false,
            true,
        ),
        (
            "@unittest.skipUnless(sys.platform == 'win32', 'Windows')\ndef f(): pass\n",
            false,
            false,
            true,
        ),
        (
            "@unittest.skipIf(os.name != 'nt', 'Windows')\ndef f(): pass\n",
            false,
            false,
            true,
        ),
        (
            "RUNS_ON_NATIVE_WINDOWS = True\nif os.name != 'nt':\n    self.skipTest('Windows')\n",
            false,
            true,
            false,
        ),
        (
            "if sys.platform != 'win32':\n    pass\n",
            false,
            false,
            false,
        ),
    ];
    for (index, (text, browser, windows, unmarked)) in fixtures.into_iter().enumerate() {
        let name = format!("fixture{index}");
        let input = source(&name, text);
        let claims = source_ci_claims::inspect(&[input]).unwrap();
        let module = format!("tests.test_{name}");
        assert_eq!(claims.browser.contains(&module), browser, "{text}");
        assert_eq!(claims.windows.contains(&module), windows, "{text}");
        assert_eq!(
            claims.unmarked_windows.contains(&module),
            unmarked,
            "{text}"
        );
    }
    let input = source("ordinary", "raise RuntimeError('must never execute')\n");
    assert_eq!(
        source_ci_claims::inspect(&[input]).unwrap(),
        Claims {
            test_modules: 1,
            ..Claims::default()
        }
    );
    let mut input = source("tampered", "pass\n");
    input.sha256 = "0".repeat(64);
    assert!(
        source_ci_claims::inspect(&[input])
            .unwrap_err()
            .contains("source bytes")
    );
    let input = source("duplicate", "pass\n");
    let duplicate = source("duplicate", "pass\n");
    assert!(source_ci_claims::inspect(&[input, duplicate]).is_err());
    let mut input = source("outside", "pass\n");
    input.path = "../test_outside.py".into();
    assert!(source_ci_claims::inspect(&[input]).is_err());
}

#[test]
fn direct_run_guard_preserves_original_compare_and_descendant_call_grammar() {
    let fixtures = [
        ("if __name__ == '__main__':\n    unittest.main()\n", true),
        ("if __name__ != 'anything':\n    unittest.main()\n", true),
        ("if __name__ in ('anything',):\n    unittest.main()\n", true),
        ("if __name__ == 'a' == 'b':\n    unittest.main()\n", true),
        (
            "if __name__ == '__main__':\n    pass\nelse:\n    unittest.main()\n",
            true,
        ),
        (
            "def outer():\n    if __name__ == '__main__':\n        unittest.main()\n",
            true,
        ),
        (
            "if __name__ == '__main__':\n    def later():\n        unittest.main()\n",
            true,
        ),
        (
            "if __name__ == '__main__':\n    other(unittest.main())\n",
            true,
        ),
        ("# if __name__ == '__main__': unittest.main()\n", false),
        (
            "text = \"if __name__ == '__main__': unittest.main()\"\n",
            false,
        ),
        ("if '__main__' == __name__:\n    unittest.main()\n", false),
        ("if __name__:\n    unittest.main()\n", false),
        (
            "if __name__ == '__main__' and enabled:\n    unittest.main()\n",
            false,
        ),
        ("if __name__ == '__main__':\n    other.main()\n", false),
        (
            "if __name__ == '__main__':\n    unittest.runner.main()\n",
            false,
        ),
        ("if __name__ == '__main__':\n    main()\n", false),
        (
            "if __name__ == '__main__':\n    callback = unittest.main\n",
            false,
        ),
        (
            "unittest.main()\nraise RuntimeError('must never execute')\n",
            false,
        ),
    ];
    let mut sources = fixtures
        .iter()
        .enumerate()
        .map(|(index, (text, _))| source(&format!("direct{index}"), text))
        .collect::<Vec<_>>();
    for path in [
        "tests/nested/test_child.py",
        "tests/helper.py",
        "test_outside.py",
    ] {
        let mut ignored = source("ignored", fixtures[0].0);
        ignored.path = path.into();
        sources.push(ignored);
    }
    let claims = source_ci_claims::inspect(&sources).unwrap();
    assert_eq!(claims.test_modules, fixtures.len());
    let expected = fixtures
        .iter()
        .enumerate()
        .filter(|(_, (_, refuses))| *refuses)
        .map(|(index, _)| format!("test_direct{index}.py"))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(claims.direct_runs, expected);
    assert_eq!(source_ci_claims::inspect(&[]).unwrap().test_modules, 0);
}

fn sample_claims() -> Claims {
    Claims {
        test_modules: 7,
        direct_runs: Default::default(),
        browser: ["tests.test_browser_a", "tests.test_browser_b"]
            .map(str::to_owned)
            .into(),
        windows: [
            "tests.test_w1",
            "tests.test_w2",
            "tests.test_w3",
            "tests.test_w4",
            "tests.test_w5",
        ]
        .map(str::to_owned)
        .into(),
        unmarked_windows: Default::default(),
    }
}

fn workflow() -> String {
    "jobs:\n  panel-browser:\n    steps:\n      - name: Panel browser tests\n        run: python -m unittest tests.test_browser_a tests.test_browser_b\n  build:\n    env:\n      NUMERA_EMOJI_MAPPER_NO_BROWSER_TESTS: '1'\n  windows-safety:\n    steps:\n      - name: Native lock, migration, HTTP persistence, identity and CLI regressions\n        run: >-\n          python -m unittest\n          tests.test_w1 tests.test_w2 tests.test_w3 tests.test_w4 tests.test_w5\n".into()
}

#[test]
fn original_ci_claims_refuse_omissions_comments_substrings_duplicates_and_stale_modules() {
    let claims = sample_claims();
    let original = workflow();
    source_ci_claims::check(&claims, &original).unwrap();
    for broken in [
        original.replace("tests.test_browser_b", ""),
        original.replace("tests.test_browser_b", "tests.test_browser_b_extra"),
        original.replace("tests.test_w2", ""),
        original.replace("tests.test_w2", "tests.test_w2_extra"),
        original.replace("tests.test_w2", "tests.test_w2 tests.test_w2"),
        original.replace("tests.test_w2", "tests.test_w2 tests.test_stale"),
        original.replace("NUMERA_EMOJI_MAPPER_NO_BROWSER_TESTS", "UNRELATED"),
        original.replace("windows-safety:", "windows-unclaimed:"),
        original.replace(
            "      - name: Panel browser tests",
            "      # - name: Panel browser tests",
        ),
        format!("{original}\n# tests.test_browser_a\n"),
    ] {
        assert!(
            source_ci_claims::check(&claims, &broken).is_err(),
            "{broken}"
        );
    }
    let mut empty = sample_claims();
    empty.browser.clear();
    assert!(source_ci_claims::check(&empty, &original).is_err());
    let mut empty = sample_claims();
    empty.test_modules = 0;
    assert!(source_ci_claims::check(&empty, &original).is_err());
    let mut direct = sample_claims();
    direct.direct_runs.insert("test_unsafe.py".into());
    let error = source_ci_claims::check(&direct, &original).unwrap_err();
    assert!(error.contains("test_unsafe.py"));
    assert!(error.contains("unittest.main"));
    let mut unmarked = sample_claims();
    unmarked
        .unmarked_windows
        .insert("tests.test_unclaimed".into());
    assert!(source_ci_claims::check(&unmarked, &original).is_err());
    let commented = original.replace(
        "          tests.test_w1 tests.test_w2 tests.test_w3 tests.test_w4 tests.test_w5",
        "          # tests.test_w1 tests.test_w2 tests.test_w3 tests.test_w4 tests.test_w5",
    );
    assert!(source_ci_claims::check(&claims, &commented).is_err());
}
