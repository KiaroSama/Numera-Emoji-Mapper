//! Historical import ownership is syntax evidence, never executed Python object identity.
use _native::{source_git_inventory::Source, source_publication_contracts as policy};
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

pub fn canaries() {
    let originals = [
        source(
            "tests/reference/build_pack.py",
            "from .announce import announce_packs\nraise RuntimeError('never execute')\n",
        ),
        source(
            "tests/reference/collection_notify.py",
            "from .announce import announce_packs\n",
        ),
        source(
            "tests/reference/build_collection.py",
            "from .collection_notify import notify\ndef ordinary():\n    links_chat_id = 'local only'\n",
        ),
        source(
            "coins/rebuild_dedup.py",
            "from emojikit.announce import announce_packs\n",
        ),
    ];
    policy::check(&originals).unwrap();
    for (index, text) in [
        (0, "from announce import announce_packs\n"),
        (0, "from ..announce import announce_packs\n"),
        (0, "from .other import announce_packs\n"),
        (0, "from .announce import announce_packs as other\n"),
        (
            0,
            "from .announce import announce_packs\nannounce_packs = other\n",
        ),
        (
            1,
            "from .announce import announce_packs\nif enabled:\n    announce_packs = other\n",
        ),
        (
            1,
            "from .announce import announce_packs\ndef announce_packs(): pass\n",
        ),
        (
            1,
            "from .announce import announce_packs\nfrom .announce import announce_packs\n",
        ),
        (2, "links_chat_id = other\n"),
        (2, "if enabled:\n    from .announce import links_chat_id\n"),
        (3, "from .announce import announce_packs\n"),
        (
            3,
            "from emojikit.announce import announce_packs\nfrom other import links_chat_id\n",
        ),
        (
            3,
            "from emojikit.announce import announce_packs\nglobals().update(values)\n",
        ),
        (3, "from emojikit.announce import *\n"),
        (
            3,
            "from emojikit.announce import announce_packs\ndef __getattr__(name): return other\n",
        ),
    ] {
        let mut changed = originals
            .iter()
            .map(|item| source(&item.path, &item.text))
            .collect::<Vec<_>>();
        changed[index] = source(&changed[index].path, text);
        assert!(policy::check(&changed).is_err(), "{text}");
    }
    let mut duplicated = originals
        .iter()
        .map(|item| source(&item.path, &item.text))
        .collect::<Vec<_>>();
    duplicated.push(source(&originals[0].path, &originals[0].text));
    assert!(policy::check(&duplicated).is_err());
    assert!(policy::check(&originals[..3]).is_err());
    assert!(policy::check(&[]).is_err());
    let mut tampered = originals;
    tampered[0].sha256 = "0".repeat(64);
    assert!(policy::check(&tampered).unwrap_err().contains("admitted"));
}
