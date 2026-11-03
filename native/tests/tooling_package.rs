//! Public archive builder behavior; source schema1 selection is retained until primary cutover.
use _native::tooling_package;
use serde_json::Value;
use std::io::Read;
use std::path::Path;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;
use tooling_fixture::Root;

#[test]
fn package_allowlist_and_published_checksum_match_pinned_source_manifest() {
    let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/native/tooling-package-source.json");
    let fixture: Value = serde_json::from_slice(
        &std::fs::read(fixture_path).expect("primary must capture source package fixture"),
    )
    .unwrap();
    assert_eq!(
        fixture["baselineRevision"],
        "1a36a968b006236bd0665a1da65c541938c06061"
    );
    assert_eq!(
        fixture["sourceHashes"]["scripts/local_environment.py"],
        "b568feffc0c75e27b7f08cb69aaf546be4ce544427234379316b8f708b619766"
    );
    assert_eq!(
        fixture["sourceHashes"]["scripts/package_native.py"],
        "cb504e8f0e22789a278129f893bb14cf1954483d9a3f620254e6ac5da7a326b0"
    );
    assert_eq!(fixture["expected"]["files"].as_object().unwrap().len(), 26);
    assert_eq!(
        fixture["expected"]["platform"], "windows-x64",
        "source capture platform must not be relabeled to match the hosted runner"
    );
    let root = Root::new("tooling package");
    for (relative, text) in fixture["inputs"].as_object().unwrap() {
        let path = root.0.join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text.as_str().unwrap().as_bytes()).unwrap();
    }
    let output = root.0.join("dist");
    let target = fixture["expected"]["platform"].as_str().unwrap();
    let archive = tooling_package::package(
        &root.0,
        &root.0.join("build/executable"),
        &root.0.join(fixture["wheel_input"].as_str().unwrap()),
        &root.0.join("dependencies"),
        &output,
        fixture["revision"].as_str().unwrap(),
        target,
    )
    .unwrap();
    let mut zip = zip::ZipArchive::new(std::fs::File::open(&archive).unwrap()).unwrap();
    let stem = archive.file_stem().unwrap().to_str().unwrap();
    let mut file = zip.by_name(&format!("{stem}/package.json")).unwrap();
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).unwrap();
    let actual: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(actual, fixture["expected"]);
    drop(file);
    let names = (0..zip.len())
        .map(|index| zip.by_index(index).unwrap().name().to_owned())
        .collect::<Vec<_>>();
    assert!(!names.iter().any(|name| name.contains("private")
        || name.ends_with("secret-test.txt")
        || name.contains("tests/")));
    let expected_files = fixture["expected"]["files"].as_object().unwrap();
    let mut expected_names = expected_files
        .keys()
        .map(|name| format!("{stem}/{name}"))
        .collect::<std::collections::BTreeSet<_>>();
    expected_names.insert(format!("{stem}/package.json"));
    assert_eq!(
        names.len(),
        27,
        "no duplicate or unlisted archive member is accepted"
    );
    assert_eq!(
        names
            .iter()
            .cloned()
            .collect::<std::collections::BTreeSet<_>>(),
        expected_names
    );
    for (relative, expected_hash) in expected_files {
        use sha2::{Digest, Sha256};
        let mut member = zip.by_name(&format!("{stem}/{relative}")).unwrap();
        assert!(
            member.is_file(),
            "payload must be a regular file: {relative}"
        );
        let expected_mode = if relative.starts_with("native/runtime/") {
            0o755
        } else {
            0o644
        };
        assert_eq!(
            member.unix_mode().unwrap() & 0o777,
            expected_mode,
            "{relative}"
        );
        let mut hash = Sha256::new();
        let mut buffer = [0_u8; 64 * 1024];
        loop {
            // Ordinary EOF reads verify CRC; the central directory/outer checksum alone cannot.
            let count = member.read(&mut buffer).unwrap();
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
        }
        let observed = hash
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            observed,
            expected_hash.as_str().unwrap(),
            "archived bytes changed: {relative}"
        );
    }
    assert_eq!(
        std::fs::read_to_string(archive.with_extension("zip.sha256")).unwrap(),
        format!(
            "{}  {}\n",
            _native::tooling_manifest::digest(&archive).unwrap(),
            archive.file_name().unwrap().to_str().unwrap()
        )
    );
    let before = std::fs::read(&archive).unwrap();
    assert!(
        tooling_package::package(
            &root.0,
            &root.0.join("build/executable"),
            &root.0.join(fixture["wheel_input"].as_str().unwrap()),
            &root.0.join("dependencies"),
            &output,
            fixture["revision"].as_str().unwrap(),
            target
        )
        .unwrap_err()
        .contains("Archive already exists")
    );
    assert_eq!(std::fs::read(&archive).unwrap(), before);
}
