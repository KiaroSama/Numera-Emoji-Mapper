//! Forged historical state sources must refuse before scratch creation or Python execution.
use _native::source_git_inventory::Source;

#[test]
fn original_state_compatibility_replay_admits_all_six_source_bodies_first() {
    let sources = [
        (
            "emojikit/packstate.py",
            "a498ad8bcbc621f3184a95e77a9d98ba79ac39b2c73177c36eec91aaa5a5169a",
        ),
        (
            "tests/reference/packstate.py",
            "d6539619a473dd86d7706bb929a8cad759d29639cd8006a1e083a0504693f9e0",
        ),
        (
            "tests/test_native_state_compat.py",
            "f656a4216951aa4da735c8550e5448fc31383bd7d1d03b8289fc6e11c8b65b2e",
        ),
        (
            "tests/test_pack_locks.py",
            "91b84ceda3c046d3c084454f327028abf4be1f65c943a5965c096e98674f2c7d",
        ),
        (
            "tests/test_resume_safety.py",
            "0af175ef5368ed7a6613ce487f65ff5b8f42e1751fcfa9e787b17faea956b3d4",
        ),
        (
            "tests/test_native_storage_recovery.py",
            "80afa262b5b2d0c9ed2e8dd2309c6cac3f2ec7af0bc3eb443205f9bd681d80f4",
        ),
    ]
    .map(|(path, hash)| Source {
        baseline_revision: "1a36a968b006236bd0665a1da65c541938c06061".into(),
        path: path.into(),
        sha256: hash.into(),
        text: "raise RuntimeError('untrusted source must never execute')".into(),
    });
    let refs = sources.iter().collect::<Vec<_>>();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let missing = root.join(format!(
        "logs/tmp/state compatibility refusal {}",
        std::process::id()
    ));
    assert!(!missing.exists());
    assert!(
        _native::source_state_compat_oracle::source_identity(&refs)
            .unwrap_err()
            .contains("source identity drift")
    );
    assert!(
        _native::source_state_compat_oracle::observe(root, &missing, &refs)
            .unwrap_err()
            .contains("source identity drift")
    );
    assert!(!missing.exists());
    for count in 0..sources.len() {
        assert!(_native::source_state_compat_oracle::source_identity(&refs[..count]).is_err());
    }
}
