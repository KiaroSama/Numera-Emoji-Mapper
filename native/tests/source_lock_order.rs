//! Original known lock-order traps stay literal, independent of the native ownership implementation.
use _native::source_git_inventory::Source;
use _native::source_lock_order::{self, MAP, PACK};
use sha2::{Digest, Sha256};

fn source(path: &str, text: &str) -> Source {
    Source {
        baseline_revision: "1a36a968b006236bd0665a1da65c541938c06061".into(),
        path: path.into(),
        sha256: Sha256::digest(text.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
        text: text.into(),
    }
}

#[test]
fn original_known_traps_preserve_nested_combined_async_and_bare_helper_semantics() {
    for (text, expected) in [
        (
            "def f():\n    with canonical_map_lock():\n        with exclusive_lock(P):\n            pass\n",
            vec![(MAP, PACK)],
        ),
        (
            "def f():\n    with canonical_map_lock():\n        with canonical_map_lock():\n            pass\n",
            vec![(MAP, MAP)],
        ),
        (
            "def f():\n    with exclusive_lock(P), canonical_map_lock():\n        pass\n",
            vec![(PACK, MAP)],
        ),
        (
            "def f():\n    with exclusive_lock(P):\n        with canonical_map_lock():\n            pass\n",
            vec![(PACK, MAP)],
        ),
        (
            "async def f():\n    async with c.exclusive_lock(P), c.canonical_map_lock():\n        pass\n",
            vec![(PACK, MAP)],
        ),
    ] {
        let functions = source_lock_order::inspect_functions(text).unwrap();
        assert_eq!(functions.len(), 1);
        assert_eq!(functions[0].name, "f");
        assert_eq!(functions[0].line, 1);
        assert_eq!(
            functions[0].pairs,
            expected
                .into_iter()
                .map(|(a, b)| (a.to_owned(), b.to_owned()))
                .collect::<Vec<_>>()
        );
    }
    let nested = "def outer():\n    with exclusive_lock(P):\n        def inner():\n            with canonical_map_lock():\n                pass\n        helper()\n        object.method()\n";
    let functions = source_lock_order::inspect_functions(nested).unwrap();
    assert_eq!(functions.len(), 2);
    assert!(
        functions[0].pairs.is_empty(),
        "nested definition does not acquire under caller"
    );
    assert_eq!(
        functions[0].acquired,
        [MAP.to_owned(), PACK.to_owned()].into()
    );
    assert!(functions[0].calls.contains("helper"));
    assert!(!functions[0].calls.contains("method"));
    assert!(functions[1].pairs.is_empty());
    assert_eq!(functions[1].acquired, [MAP.to_owned()].into());
    assert!(source_lock_order::inspect_functions("def broken(").is_err());
    assert!(source_lock_order::inspect_functions(&" ".repeat(2 * 1024 * 1024 + 1)).is_err());
}

#[test]
fn real_source_selection_and_helper_reentry_cannot_pass_on_comments_empty_or_tampered_input() {
    let safe = "def f():\n    with exclusive_lock(P), canonical_map_lock():\n        pass\n\nif __name__ == \"__main__\":\n    raise RuntimeError('never execute')\n";
    let mut sources = vec![
        source("emojikit/build_pack.py", safe),
        source("coins/fetch_paprika.py", "pass\n"),
        source("coins/verify_logos.py", "pass\n"),
        source("coins/rebuild_dedup.py", "pass\n"),
        source("coins/remap_ids.py", "pass\n"),
        source(
            "emojikit/not_executable.py",
            "def f():\n    with canonical_map_lock(), exclusive_lock(P): pass\n",
        ),
        source("tests/reference/not_executable.py", "pass\n"),
        source(
            "coins/nested/not_top_level.py",
            "def f():\n    with canonical_map_lock(), exclusive_lock(P): pass\n",
        ),
    ];
    let report = source_lock_order::inspect(&sources).unwrap();
    assert_eq!(report.files.len(), 5);
    source_lock_order::check(&report).unwrap();
    for (text, expected) in [
        (
            "def bad():\n    with canonical_map_lock(), exclusive_lock(P): pass\n",
            "inversions",
        ),
        (
            "def bad():\n    with canonical_map_lock(), canonical_map_lock(): pass\n",
            "reentrant",
        ),
        (
            "def helper():\n    with canonical_map_lock(): pass\ndef caller():\n    with canonical_map_lock(): helper()\n",
            "helper_reentry",
        ),
    ] {
        sources[1] = source("coins/fetch_paprika.py", text);
        let report = source_lock_order::inspect(&sources).unwrap();
        let found = match expected {
            "inversions" => &report.inversions,
            "reentrant" => &report.reentrant,
            _ => &report.helper_reentry,
        };
        assert_eq!(found.len(), 1, "{report:?}");
        assert!(found[0].starts_with("fetch_paprika.py:"));
        assert!(source_lock_order::check(&report).is_err());
    }
    sources[1] = source(
        "coins/fetch_paprika.py",
        "def helper():\n    with canonical_map_lock(): pass\ndef caller():\n    with canonical_map_lock(): object.helper()\n",
    );
    assert!(
        source_lock_order::inspect(&sources)
            .unwrap()
            .helper_reentry
            .is_empty()
    );
    sources[1].sha256 = "0".repeat(64);
    assert!(
        source_lock_order::inspect(&sources)
            .unwrap_err()
            .contains("source bytes")
    );
    assert!(source_lock_order::check(&source_lock_order::Report::default()).is_err());
}
