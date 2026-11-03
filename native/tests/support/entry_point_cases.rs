//! Independent original entry-point selection: raising source text must remain inert.
use _native::{source_entry_points as policy, source_git_inventory::Source};
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

pub fn check_canaries() {
    let mut sources = vec![
        source(
            "tests/reference/build_pack.py",
            "raise RuntimeError('never import')\nif __name__ == \"__main__\":\n    main()\n",
        ),
        source(
            "tests/reference/emoji_bot.py",
            "\nif __name__ == \"__main__\":\n    main()\n",
        ),
        source(
            "tests/reference/panel.py",
            "\nif __name__ == \"__main__\":\n    main()\n",
        ),
        source(
            "coins/verify_logos.py",
            "raise RuntimeError('never import')\n",
        ),
        source("coins/rebuild_dedup.py", "pass\n"),
        source("coins/_env.py", "raise RuntimeError('support only')\n"),
        source("coins/__init__.py", "pass\n"),
        source(
            "emojikit/media_bridge.py",
            "raise RuntimeError('worker only')\n",
        ),
        source(
            "tests/reference/not_a_command.py",
            "# if __name__ == \"__main__\":\npass\n",
        ),
        source(
            "tests/reference/nested/build_pack.py",
            "\nif __name__ == \"__main__\":\n    main()\n",
        ),
    ];
    let report = policy::inspect(&sources).unwrap();
    assert_eq!(report.modules.len(), 7);
    assert_eq!(
        report.commands,
        std::collections::BTreeSet::from(
            [
                "build-pack",
                "coin-rebuild",
                "coin-verify-logos",
                "emoji-bot",
                "panel"
            ]
            .map(str::to_owned)
        )
    );
    assert_eq!(
        report.support,
        std::collections::BTreeSet::from(["coins._env".into()])
    );
    assert!(report.root_python.is_empty());
    assert!(policy::check(&report).is_err(), "too few tools must fail");
    for name in [
        "add_media",
        "build_collection",
        "fetch_pack",
        "fetch_emoji_ids",
        "identity_repair",
        "make_emoji_pngs",
        "pack_archive",
        "pack_manifest",
    ] {
        sources.push(source(
            &format!("tests/reference/{name}.py"),
            "\nif __name__ == \"__main__\":\n    main()\n",
        ));
    }
    let report = policy::inspect(&sources).unwrap();
    policy::check(&report).unwrap();
    let mut missing = policy::inspect(&sources).unwrap();
    missing.modules.remove("coins.verify_logos");
    assert!(policy::check(&missing).is_err());
    let mut root = policy::inspect(&sources).unwrap();
    root.root_python.insert("misplaced.py".into());
    assert!(policy::check(&root).is_err());
    assert!(policy::check(&policy::Report::default()).is_err());
    for extra in [
        source(
            "tests/reference/unknown.py",
            "\nif __name__ == \"__main__\":\n    main()\n",
        ),
        source("coins/unknown.py", "pass\n"),
        source("../escape.py", "pass\n"),
        source("coins/_env.py", "pass\n"),
    ] {
        let mut changed = sources
            .iter()
            .map(|item| source(&item.path, &item.text))
            .collect::<Vec<_>>();
        changed.push(extra);
        assert!(policy::inspect(&changed).is_err());
    }
    let mut drift = source("coins/_env.py", "pass\n");
    drift.text.push_str("raise RuntimeError('drift')\n");
    assert!(policy::inspect(&[drift]).is_err());
    assert!(policy::inspect(&[source("coins/_env.py", &" ".repeat(2 * 1024 * 1024 + 1))]).is_err());
}
