//! Historical media ownership syntax is distinct from imported Python object identity.
use _native::{source_git_inventory::Source, source_media_contracts as policy};
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
            "emojikit/media.py",
            "def fit_100(image): return image\ndef _trim(image): return image\nraise RuntimeError('never execute')\n",
        ),
        source(
            "emojikit/repaint.py",
            "def parse_tint(text): return text\ndef repaint_in_place(path): return True\n",
        ),
        source(
            "tests/reference/make_emoji_pngs.py",
            "from emojikit import media\n_fit_100 = media.fit_100\n_trim = media._trim\n",
        ),
        source(
            "coins/_paprika_api.py",
            "from emojikit.media import fit_100 as _fit_100\n",
        ),
        source(
            "tests/reference/collection_state.py",
            "BRAND_LOGO_EMOJI = '✅'\ndef ordinary():\n    BRAND_LOGO_BOTS = 'local only'\n",
        ),
    ];
    policy::check(&originals).unwrap();
    for (index, text) in [
        (0, "def fit_100(image): return image\n_trim = other\n"),
        (
            0,
            "def fit_100(image): return image\ndef _trim(image): return image\nparse_tint = other\n",
        ),
        (
            1,
            "def parse_tint(text): return text\nrepaint_in_place = other\n",
        ),
        (
            1,
            "@other\ndef parse_tint(text): return text\ndef repaint_in_place(path): pass\n",
        ),
        (2, "_fit_100 = other.fit_100\n_trim = media._trim\n"),
        (
            2,
            "_fit_100 = media.fit_100\n_trim = media._trim\n_fit_100 = other\n",
        ),
        (3, "from other import fit_100 as _fit_100\n"),
        (
            3,
            "from emojikit.media import fit_100 as _fit_100\n_fit_100 = other\n",
        ),
        (4, "BRAND_LOGO_DEFAULT = 'never a repository default'\n"),
        (4, "if enabled:\n    BRAND_LOGO_BOTS = []\n"),
        (4, "from other import BRAND_LOGO_KW\n"),
        (4, "from other import *\n"),
        (4, "globals().update(values)\n"),
        (4, "def __getattr__(name): return other\n"),
    ] {
        let mut changed = originals
            .iter()
            .map(|item| source(&item.path, &item.text))
            .collect::<Vec<_>>();
        changed[index] = source(&changed[index].path, text);
        assert!(policy::check(&changed).is_err(), "{text}");
    }
    let mut duplicate = originals
        .iter()
        .map(|item| source(&item.path, &item.text))
        .collect::<Vec<_>>();
    duplicate.push(source(&originals[0].path, &originals[0].text));
    assert!(policy::check(&duplicate).is_err());
    assert!(policy::check(&originals[..4]).is_err());
    assert!(policy::check(&[]).is_err());
    let mut tampered = originals;
    tampered[0].sha256 = "0".repeat(64);
    assert!(policy::check(&tampered).unwrap_err().contains("admitted"));
}
