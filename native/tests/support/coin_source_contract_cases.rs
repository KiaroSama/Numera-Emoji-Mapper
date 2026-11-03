//! Literal historical architecture traps; static bindings are not runtime object identity.
use _native::source_coin_contracts;
use _native::source_git_inventory::Source;
use sha2::{Digest, Sha256};

fn source(path: &str, text: &str) -> Source {
    Source {
        baseline_revision: "1a36a968b006236bd0665a1da65c541938c06061".into(),
        path: path.into(),
        sha256: Sha256::digest(text.as_bytes())
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect(),
        text: text.into(),
    }
}

fn copied(sources: &[Source]) -> Vec<Source> {
    sources
        .iter()
        .map(|item| source(&item.path, &item.text))
        .collect()
}

fn valid() -> Vec<Source> {
    vec![
        source(
            "coins/_inventory.py",
            "def base_ticker(t): return t\ndef refill_inventory(m, i, o):\n    return f'premium-id: {eid}'\nraise RuntimeError('never execute')\n",
        ),
        source(
            "coins/_paprika_api.py",
            "from coins._inventory import base_ticker\nraise RuntimeError('never execute')\n",
        ),
        source(
            "coins/_dedup_map.py",
            "from coins._inventory import refill_inventory\n",
        ),
        source(
            "coins/fetch_cmc.py",
            "from coins._inventory import base_ticker\nfrom coins.fetch_paprika import refill_inventory\nfrom coins._provider_publish import publish_logos\n",
        ),
        source(
            "coins/fetch_paprika.py",
            "from coins import _inventory\nfrom coins._provider_publish import publish_logos\ndef refill_inventory(ticker_to_id):\n    return _inventory.refill_inventory(ticker_to_id, INV, OUT_INV)\n",
        ),
        source("coins/extra.py", "# no HTTP bypass\n"),
        source(
            "coins/_provider_publish.py",
            "from emojikit.packstate import pack_family_lock_path\nSET_BASE = operator_config.value('COIN_PACK_BASE')\nPACK_LOCK = pack_family_lock_path(SET_BASE)\nSTATE = ROOT / 'rebuild_dedup_state.json'\n",
        ),
        source(
            "coins/_dedup_plan.py",
            "from emojikit.packstate import pack_family_lock_path\nBASE = operator_config.value('COIN_PACK_BASE')\nLOCK = pack_family_lock_path(BASE)\nSTATE = ROOT / 'rebuild_dedup_state.json'\nOLD_STATE = ROOT / 'rebuild_state.json'\n",
        ),
        source(
            "coins/verify_logos.py",
            "from emojikit.packstate import pack_family_lock_path\nSET_BASE = operator_config.value('COIN_PACK_BASE')\nPACK_LOCK = pack_family_lock_path(SET_BASE)\n",
        ),
        source(
            "coins/remap_ids.py",
            "import numpy as np\nfrom emojikit.packstate import pack_family_lock_path\nSET_BASE = operator_config.value('COIN_PACK_BASE')\nPACK_LOCK = pack_family_lock_path(SET_BASE)\n",
        ),
        source(
            "coins/nested/not_scanned.py",
            "urlopen('not a direct coin file')\n",
        ),
    ]
}

pub fn canaries() {
    let originals = valid();
    let report = source_coin_contracts::inspect(&originals).unwrap();
    source_coin_contracts::check(&report).unwrap();
    assert_eq!(report.files.len(), 10);
    assert_eq!(report.shared_contracts, 7);
    assert_eq!(report.numpy_users, ["coins/remap_ids.py".to_owned()].into());
    assert_eq!(report.inventory_owners, ["_inventory.py".to_owned()].into());
    assert_eq!(report.bindings, 6);
    for (index, text, diagnostic) in [
        (1, "def base_ticker(t): return 'drift'\n", "binding"),
        (1, "from other import base_ticker\n", "binding"),
        (
            1,
            "from coins._inventory import base_ticker\nbase_ticker = other\n",
            "binding",
        ),
        (
            1,
            "from coins._inventory import base_ticker\nif True:\n    base_ticker = other\n",
            "binding",
        ),
        (
            1,
            "from coins._inventory import base_ticker\nfrom other import *\n",
            "binding",
        ),
        (1, "from ._inventory import base_ticker\n", "binding"),
        (
            3,
            "from coins._inventory import base_ticker\nfrom other import refill_inventory\n",
            "binding",
        ),
        (
            4,
            "from coins import _inventory\ndef refill_inventory(ticker_to_id):\n    return different(ticker_to_id, INV, OUT_INV)\n",
            "binding",
        ),
        (
            4,
            "from coins import _inventory\n@other\ndef refill_inventory(ticker_to_id):\n    return _inventory.refill_inventory(ticker_to_id, INV, OUT_INV)\n",
            "binding",
        ),
        (5, "urlopen ('https://example.test')\n", "urlopen"),
        (
            5,
            "# urlopen\u{1c}( is also an original raw-scan violation\n",
            "urlopen",
        ),
        (5, "# premium-id: {eid}\n", "inventory"),
    ] {
        let mut changed = copied(&originals);
        changed[index] = source(&changed[index].path, text);
        let report = source_coin_contracts::inspect(&changed).unwrap();
        let error = source_coin_contracts::check(&report).unwrap_err();
        assert!(error.contains(diagnostic), "{error}");
    }
    for (index, old, new) in [
        (
            3,
            "coins._provider_publish import publish_logos",
            "other import publish_logos",
        ),
        (
            4,
            "coins._provider_publish import publish_logos",
            "other import publish_logos",
        ),
        (
            6,
            "pack_family_lock_path(SET_BASE)",
            "pack_family_lock_path('other')",
        ),
        (7, "pack_family_lock_path(BASE)", "different(BASE)"),
        (8, "COIN_PACK_BASE", "OTHER_PACK_BASE"),
        (9, "emojikit.packstate", "other"),
        (6, "rebuild_dedup_state.json", "rebuild_state.json"),
        (7, "rebuild_state.json", "rebuild_dedup_state.json"),
    ] {
        let mut changed = copied(&originals);
        assert!(changed[index].text.contains(old));
        changed[index] = source(&changed[index].path, &changed[index].text.replace(old, new));
        let report = source_coin_contracts::inspect(&changed).unwrap();
        assert!(
            source_coin_contracts::check(&report)
                .unwrap_err()
                .contains("shared")
        );
    }
    for extra in [
        "\nPACK_LOCK = other\n",
        "\nif enabled:\n    PACK_LOCK = other\n",
        "\nfrom other import PACK_LOCK\n",
    ] {
        let mut changed = copied(&originals);
        changed[6] = source(&changed[6].path, &format!("{}{extra}", changed[6].text));
        let report = source_coin_contracts::inspect(&changed).unwrap();
        assert!(
            source_coin_contracts::check(&report)
                .unwrap_err()
                .contains("shared")
        );
    }
    for text in [
        "import numpy\n",
        "  import numpyish\n",
        "\u{1c}import numpy as np\n",
        "'''\nimport numpy\n'''\n",
        "pass\n\timport numpy\n",
    ] {
        for path in ["coins/extra.py", "emojikit/extra.py", "extra.py"] {
            let mut changed = copied(&originals);
            changed.push(source(path, text));
            if path == "coins/extra.py" {
                changed.remove(5);
            }
            let error =
                source_coin_contracts::check(&source_coin_contracts::inspect(&changed).unwrap())
                    .unwrap_err();
            assert!(error.contains("NumPy"), "{text}: {error}");
        }
    }
    for (path, text) in [
        ("coins/extra.py", "# import numpy\n"),
        ("coins/extra.py", "from numpy import array\n"),
        ("coins/extra.py", "import  numpy\n"),
        ("coins/extra.py", "import\tnumpy\n"),
        ("coins/extra.py", "pass\u{2028}import numpy\n"),
        ("tests/test_extra.py", "import numpy\n"),
        ("emojikit/nested/extra.py", "import numpy\n"),
    ] {
        let mut changed = copied(&originals);
        changed.push(source(path, text));
        if path == "coins/extra.py" {
            changed.remove(5);
        }
        source_coin_contracts::check(&source_coin_contracts::inspect(&changed).unwrap()).unwrap();
    }
    let mut changed = copied(&originals);
    changed[5] = source(
        "coins/extra.py",
        "# urlopenish( is not the original pattern\n",
    );
    source_coin_contracts::check(&source_coin_contracts::inspect(&changed).unwrap()).unwrap();
    changed[1].sha256 = "0".repeat(64);
    assert!(source_coin_contracts::inspect(&changed).is_err());
    let mut duplicate = copied(&originals);
    duplicate.push(source(&originals[0].path, &originals[0].text));
    assert!(source_coin_contracts::inspect(&duplicate).is_err());
    assert!(source_coin_contracts::inspect(&[]).is_err());
    assert!(source_coin_contracts::inspect(&originals[1..]).is_err());
    let mut missing = copied(&originals);
    missing.remove(1);
    assert!(source_coin_contracts::inspect(&missing).is_err());
    let mut malformed = originals;
    malformed[1] = source("coins/_paprika_api.py", "def broken(\n");
    assert!(source_coin_contracts::inspect(&malformed).is_err());
}
