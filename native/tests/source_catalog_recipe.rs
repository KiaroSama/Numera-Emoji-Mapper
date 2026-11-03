//! Synthetic refusal only; positive admission and three-source tampering use the pinned Git corpus.
use _native::source_git_inventory::Source;

fn untrusted() -> [Source; 3] {
    [
        (
            "tests/reference/catalog.py",
            "5dab2e475754dc219dfe3fd8e98012fffd59ab0c760f646752ff9d35f7188ee2",
        ),
        (
            "emojikit/media_paths.py",
            "0947331cd018503884b2a3c0b33f401476f9b69a0f4865ee337b2d4d90fa0ff3",
        ),
        (
            "tests/test_native_catalog.py",
            "2680e795ff159b3fc1a61f3b5c959c689902adc62311da3d51fe518939453a96",
        ),
    ]
    .map(|(path, sha256)| Source {
        baseline_revision: "1a36a968b006236bd0665a1da65c541938c06061".into(),
        path: path.into(),
        sha256: sha256.into(),
        text: "raise RuntimeError('untrusted source must never execute')".into(),
    })
}

#[test]
fn original_catalog_test_replay_admits_every_source_before_creating_scratch() {
    let sources = [
        (
            "tests/reference/catalog.py",
            "5dab2e475754dc219dfe3fd8e98012fffd59ab0c760f646752ff9d35f7188ee2",
        ),
        (
            "emojikit/media_paths.py",
            "0947331cd018503884b2a3c0b33f401476f9b69a0f4865ee337b2d4d90fa0ff3",
        ),
        (
            "tests/test_catalog.py",
            "33391dab27bc8d005844839047b8683a14212c910cf0240f76f7c08fb26ea062",
        ),
        (
            "tests/reference/ingest.py",
            "6654f129fba59b600d8498c7d734a36973fd9761276075d1f23a28974ba69162",
        ),
        (
            "tests/_similarity_oracle.py",
            "9be48166b64ab2052798c67e2f7426c2dd0008fc27b4d668a785877fba7ac44e",
        ),
        (
            "emojikit/errors.py",
            "6c29243e2ec976606174605f1deaa3d23ef8d94fea47e886465c27f4f21f2026",
        ),
        (
            "tests/test_catalog_order.py",
            "4cf89c5ae515a92b7bbdc397642a691ff0f6879e64169fd6b798a8ef607010b0",
        ),
        (
            "tests/test_media_paths.py",
            "dda79d4365c8864be58bf82900ad20b78e292502d71896fd44e26c0203f17bd0",
        ),
        (
            "emojikit/similarity.py",
            "7f849f7bebc5837b100de00afe7eedbe275045359377087fa0187445a4587892",
        ),
        (
            "tests/test_native_similarity.py",
            "1ab3bc95d93fa181d7214b65ff83cdf9efa89bc00b08aa15761987248cbe3b7e",
        ),
        (
            "tests/test_native_catalog.py",
            "2680e795ff159b3fc1a61f3b5c959c689902adc62311da3d51fe518939453a96",
        ),
        (
            "tests/reference/maintenance.py",
            "d145a381f6da57a03551eeec76d1812e5e521af707e6dc7e7925dc0852b86278",
        ),
        (
            "tests/reference/packstate.py",
            "d6539619a473dd86d7706bb929a8cad759d29639cd8006a1e083a0504693f9e0",
        ),
        (
            "tests/test_native_real_catalog.py",
            "62a4a58199f9204deae3455fc7e4cf31ab1c01a9f7a6b3572c711c14023aa72b",
        ),
        (
            "emojikit/sqlite_snapshot.py",
            "8e3dd41f6607f1f1e4da4b777192088b9b1b370cf429774968a0771a1f78ca4d",
        ),
    ]
    .map(|(path, hash)| Source {
        baseline_revision: "1a36a968b006236bd0665a1da65c541938c06061".into(),
        path: path.into(),
        sha256: hash.into(),
        text: "raise RuntimeError('must never execute')".into(),
    });
    let refs = sources.iter().collect::<Vec<_>>();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let absent = root.join(format!(
        "logs/tmp/untrusted catalog test {}",
        std::process::id()
    ));
    assert!(!absent.exists());
    assert!(
        _native::source_catalog_tests_oracle::source_identity(&refs)
            .unwrap_err()
            .contains("source identity drift")
    );
    assert!(
        _native::source_catalog_tests_oracle::observe(root, &absent, &refs)
            .unwrap_err()
            .contains("source identity drift")
    );
    assert!(!absent.exists());
    for count in 0..sources.len() {
        assert!(_native::source_catalog_tests_oracle::source_identity(&refs[..count]).is_err());
    }
}

#[test]
fn claimed_original_hashes_never_authorize_untrusted_source_or_workspace_creation() {
    let source = untrusted();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let absent = root.join("logs/tmp").join(format!(
        "untrusted-catalog-must-not-create-{}",
        std::process::id()
    ));
    assert!(!absent.exists());
    assert!(
        _native::source_catalog_oracle::source_identity(&source[0], &source[1], &source[2])
            .unwrap_err()
            .contains("source identity drift")
    );
    assert!(
        _native::source_catalog_oracle::observe(root, &absent, &source[0], &source[1], &source[2])
            .unwrap_err()
            .contains("source identity drift")
    );
    assert!(!absent.exists());
}
