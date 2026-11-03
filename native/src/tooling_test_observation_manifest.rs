//! Admission and byte identity of the caller's public input closure, not proof of its completeness.
use super::{
    paths::{member_path, public_path},
    refusal, text,
};
use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};
pub(crate) const SMALL_LIMIT: u64 = 8 * 1024 * 1024;
const BINARY_LIMIT: u64 = 1024 * 1024 * 1024;
const CLOSURE_LIMIT: u64 = 64 * 1024 * 1024;

#[cfg(test)]
pub(super) mod fixture {
    use crate as _native;
    include!("../tests/support/tooling_fixture.rs");
}
#[cfg(test)]
const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
#[cfg(test)]
pub(super) fn sample() -> (fixture::Root, PathBuf, PathBuf, PathBuf) {
    let root = fixture::Root::new("libtest observations");
    let files = [
        ("source", "tests/test_original.py"),
        ("target", "native/tests/cases.rs"),
        ("fixture", "tests/fixtures/sample.json"),
        ("support", "native/tests/support/common.rs"),
        ("config", "native/Cargo.toml"),
        ("config", "native/Cargo.lock"),
        ("config", ".cargo/tests.toml"),
    ];
    for (_, name) in files {
        let path = root.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"abc").unwrap();
    }
    let binary = root.0.join("native/target/debug/deps/cases");
    std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
    std::fs::write(&binary, b"abc").unwrap();
    let manifest = root.0.join("logs/tmp/input.json");
    let value = json!({"schema":1,"target_revision":"a".repeat(40),"baseline_revision":"b".repeat(40),
        "compiler_artifact":{"reason":"compiler-artifact","package_id":"path+file:///synthetic#case@1.0.0","manifest_path":root.0.join("native/Cargo.toml"),
            "target":{"name":"cases","kind":["test"],"crate_types":["bin"],"src_path":root.0.join("native/tests/cases.rs"),"edition":"2024","doc":false,"doctest":false,"test":true},
            "profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":true},
            "features":[],"filenames":[binary],"executable":binary,"fresh":false},
        "files":files.map(|(role,path)|json!({"role":role,"path":path,"sha256":ABC}))});
    crate::atomic::write_json(&manifest, &value).unwrap();
    let store = root.0.join("logs/test-observations/invocation");
    std::fs::create_dir_all(&store).unwrap();
    (root, store, manifest, binary)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literal_hash_and_admitted_bytes_refuse_drift_and_protected_paths() {
        let (root, _, manifest, binary) = sample();
        assert_eq!(read_file(&binary, BINARY_LIMIT, false).unwrap().0, ABC);
        let admitted = Admission::new(&root.0, &manifest, &binary).unwrap();
        admitted.recheck().unwrap();
        assert!(read_file(&binary, 2, false).is_err());
        assert!(read_file(&root.0, BINARY_LIMIT, false).is_err());
        for name in [
            ".env",
            "secrets.md",
            "private/logo.png",
            "tests/../.env",
            "tests/.ignoreme/x",
            "tests/fixtures/credentials.pem",
            "native/tests/a\\b.rs",
        ] {
            assert!(member_path("support", name).is_err());
        }
        std::fs::write(root.0.join("tests/fixtures/sample.json"), b"changed").unwrap();
        assert!(admitted.recheck().is_err());
        root.clean().unwrap();
    }
    #[cfg(unix)]
    #[test]
    fn linked_closure_member_is_not_admitted_even_when_bytes_match() {
        let (root, _, manifest, binary) = sample();
        let member = root.0.join("tests/fixtures/sample.json");
        std::fs::remove_file(&member).unwrap();
        std::os::unix::fs::symlink(root.0.join("tests/test_original.py"), &member).unwrap();
        assert!(Admission::new(&root.0, &manifest, &binary).is_err());
        std::fs::remove_file(member).unwrap();
        root.clean().unwrap();
    }
    #[test]
    fn manifest_refuses_unknown_duplicate_missing_and_false_compilation_claims() {
        let (root, _, manifest, binary) = sample();
        let original = std::fs::read_to_string(&manifest).unwrap();
        let value: Value = serde_json::from_str(&original).unwrap();
        let mut explicit = value.clone();
        explicit["compiler_artifact"]["target"]["test"] = json!(false);
        crate::atomic::write_json(&manifest, &explicit).unwrap();
        assert!(Admission::new(&root.0, &manifest, &binary).is_ok());
        for flag in [Value::Null, json!("false")] {
            explicit["compiler_artifact"]["target"]["test"] = flag;
            crate::atomic::write_json(&manifest, &explicit).unwrap();
            assert!(Admission::new(&root.0, &manifest, &binary).is_err());
        }
        for path in [
            "/authenticated",
            "/compiler_artifact/profile/test",
            "/compiler_artifact/target/kind",
            "/compiler_artifact/executable",
            "/files/0/sha256",
            "/target_revision",
        ] {
            let mut bad = value.clone();
            if path == "/authenticated" {
                bad["authenticated"] = json!(true);
            } else {
                *bad.pointer_mut(path).unwrap() = json!(false);
            }
            crate::atomic::write_json(&manifest, &bad).unwrap();
            assert!(
                Admission::new(&root.0, &manifest, &binary).is_err(),
                "{path}"
            );
        }
        let duplicate = original.replacen('{', "{\"schema\":1,", 1);
        std::fs::write(&manifest, duplicate).unwrap();
        assert!(Admission::new(&root.0, &manifest, &binary).is_err());
        let mut missing = value.clone();
        missing["files"].as_array_mut().unwrap().remove(2);
        crate::atomic::write_json(&manifest, &missing).unwrap();
        assert!(Admission::new(&root.0, &manifest, &binary).is_err());
        root.clean().unwrap();
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    schema: u32,
    target_revision: String,
    baseline_revision: String,
    compiler_artifact: Artifact,
    files: Vec<Member>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Member {
    role: String,
    path: String,
    sha256: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    reason: String,
    package_id: String,
    manifest_path: PathBuf,
    target: Target,
    profile: Profile,
    features: Vec<String>,
    filenames: Vec<PathBuf>,
    executable: PathBuf,
    fresh: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Target {
    kind: Vec<String>,
    crate_types: Vec<String>,
    name: String,
    src_path: PathBuf,
    edition: String,
    doc: bool,
    doctest: bool,
    test: bool,
    #[serde(rename = "required-features", default)]
    required_features: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    opt_level: String,
    debuginfo: Value,
    debug_assertions: bool,
    overflow_checks: bool,
    test: bool,
}
fn hex(value: &str, size: usize) -> bool {
    value.len() == size
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}
fn clean_json(value: &Value) -> bool {
    match value {
        Value::String(s) => !s.chars().any(char::is_control),
        Value::Array(a) => a.iter().all(clean_json),
        Value::Object(o) => o.iter().all(|(k, v)| text(k) && clean_json(v)),
        _ => true,
    }
}
#[cfg(windows)]
#[allow(unsafe_code)]
fn identity(file: &std::fs::File) -> Result<(u64, u64), String> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    let mut information = BY_HANDLE_FILE_INFORMATION::default();
    // The borrowed live File pins HANDLE; the exact SDK output is initialized and exclusively borrowed.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut information) } == 0 {
        return Err(refusal("cannot query opened Windows file identity"));
    }
    Ok((
        u64::from(information.dwVolumeSerialNumber),
        (u64::from(information.nFileIndexHigh) << 32) | u64::from(information.nFileIndexLow),
    ))
}
#[cfg(unix)]
fn identity(file: &std::fs::File) -> Result<(u64, u64), String> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file
        .metadata()
        .map_err(|_| refusal("cannot inspect opened file identity"))?;
    Ok((metadata.dev(), metadata.ino()))
}
#[cfg(not(any(unix, windows)))]
fn identity(_: &std::fs::File) -> Result<(u64, u64), String> {
    Err(refusal(
        "opened file identity is unsupported on this platform",
    ))
}
pub(crate) fn regular(path: &Path) -> Result<std::fs::File, String> {
    crate::tooling_environment::reject_links(
        path,
        "observation inputs cannot follow links/reparse points",
    )?;
    let before = std::fs::symlink_metadata(path).map_err(|_| refusal("input is missing"))?;
    if !before.is_file() {
        return Err(refusal("input must be an existing regular file"));
    }
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options
        .open(path)
        .map_err(|_| refusal("cannot open observation input"))?;
    let metadata = file
        .metadata()
        .map_err(|_| refusal("cannot inspect opened input"))?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(refusal("opened input is a reparse point"));
        }
    }
    if !metadata.is_file() {
        return Err(refusal("opened input is not regular"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if (before.dev(), before.ino()) != identity(&file)? {
            return Err(refusal("input path changed its opened file identity"));
        }
    }
    identity(&file)?;
    crate::tooling_environment::reject_links(path, "observation input changed to a link")?;
    Ok(file)
}
pub(crate) fn read_file(
    path: &Path,
    limit: u64,
    keep: bool,
) -> Result<(String, Vec<u8>, u64), String> {
    let mut file = regular(path)?;
    let opened_identity = identity(&file)?;
    let before = file
        .metadata()
        .map_err(|_| refusal("cannot inspect input size"))?;
    if before.len() > limit {
        return Err(refusal("input exceeds its byte budget"));
    }
    let mut hasher = Sha256::new();
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 65536];
    let mut count = 0_u64;
    loop {
        let n = file
            .read(&mut buffer)
            .map_err(|_| refusal("cannot read observation input"))?;
        if n == 0 {
            break;
        }
        count += n as u64;
        if count > limit {
            return Err(refusal("input grew beyond its byte budget"));
        }
        hasher.update(&buffer[..n]);
        if keep {
            bytes.extend_from_slice(&buffer[..n]);
        }
    }
    let after = file
        .metadata()
        .map_err(|_| refusal("cannot recheck input"))?;
    if before.len() != count
        || after.len() != count
        || before.modified().ok() != after.modified().ok()
    {
        return Err(refusal("input changed while being read"));
    }
    crate::tooling_environment::reject_links(path, "observation input changed to a link")?;
    if identity(&regular(path)?)? != opened_identity || identity(&file)? != opened_identity {
        return Err(refusal("input path changed its opened file identity"));
    }
    Ok((format!("{:x}", hasher.finalize()), bytes, count))
}
pub(super) fn sync_existing(path: &Path) -> Result<(), String> {
    crate::tooling_environment::reject_links(path, "sidecar sync cannot follow links")?;
    let expected = identity(&regular(path)?)?;
    let mut options = std::fs::OpenOptions::new();
    options.read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let file = options
        .open(path)
        .map_err(|_| refusal("cannot reopen owned sidecar for durable sync"))?;
    if !file
        .metadata()
        .map_err(|_| refusal("cannot inspect sidecar"))?
        .is_file()
        || identity(&file)? != expected
    {
        return Err(refusal("sidecar identity changed before sync"));
    }
    file.sync_all()
        .map_err(|_| refusal("cannot durably sync existing sidecar"))
}
pub(crate) fn relative(root: &Path, path: &Path) -> Result<String, String> {
    crate::tooling_environment::reject_links(path, "observation paths cannot follow links")?;
    let path = crate::migration_bundle::resolved(path)?;
    let path = path
        .strip_prefix(root)
        .map_err(|_| refusal("observation path escapes the project"))?;
    let name = path
        .to_str()
        .filter(|s| text(s))
        .ok_or_else(|| refusal("path must be nonempty control-free UTF-8"))?;
    Ok(name.replace('\\', "/"))
}
pub(crate) struct Admission {
    pub(super) value: Value,
    pub(super) hash: String,
    pub(super) binary_hash: String,
    pub(super) binary_name: String,
    pub(super) bytes: Vec<u8>,
    root: PathBuf,
    identity: (u64, u64),
    manifest: PathBuf,
    binary: PathBuf,
    members: BTreeMap<String, String>,
}
impl Admission {
    pub(crate) fn new(root: &Path, path: &Path, binary: &Path) -> Result<Self, String> {
        if !path.is_absolute() || !binary.is_absolute() {
            return Err(refusal("manifest and executable paths must be absolute"));
        }
        let name = relative(root, path)?;
        if !public_path(&name) || !name.starts_with("logs/tmp/") || !name.ends_with(".json") {
            return Err(refusal(
                "input manifest must be a public JSON file under logs/tmp",
            ));
        }
        let binary_name = relative(root, binary)?;
        if !public_path(&binary_name) || !binary_name.starts_with("native/target/") {
            return Err(refusal("executable must be a native target artifact"));
        }
        let (hash, bytes, _) = read_file(path, SMALL_LIMIT, true)?;
        let decoded: Manifest = serde_json::from_slice(&bytes).map_err(|_| {
            refusal("strict manifest schema rejects unknown/duplicate fields or invalid JSON")
        })?;
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|_| refusal("manifest is not UTF-8 JSON"))?;
        if !clean_json(&value)
            || decoded.schema != 1
            || !hex(&decoded.target_revision, 40)
            || !hex(&decoded.baseline_revision, 40)
            || decoded.files.is_empty()
            || decoded.files.len() > 4096
        {
            return Err(refusal(
                "manifest needs schema1, exact lowercase revisions and1..4096 members",
            ));
        }
        let mut members = BTreeMap::new();
        let mut roles = BTreeSet::new();
        let mut total = 0;
        for member in &decoded.files {
            member_path(&member.role, &member.path)?;
            if !hex(&member.sha256, 64)
                || members
                    .insert(member.path.clone(), member.sha256.clone())
                    .is_some()
            {
                return Err(refusal("duplicate member or invalid SHA256"));
            }
            let (actual, _, size) = read_file(&root.join(&member.path), SMALL_LIMIT, false)?;
            total += size;
            if actual != member.sha256 || total > CLOSURE_LIMIT {
                return Err(refusal("closure hash drift or aggregate exceeds64MiB"));
            }
            roles.insert(member.role.as_str());
        }
        let has_role = |name: &str, role: &str| {
            decoded
                .files
                .iter()
                .any(|m| m.path == name && m.role == role)
        };
        if ["source", "target", "fixture"]
            .iter()
            .any(|r| !roles.contains(r))
            || [
                "native/Cargo.toml",
                "native/Cargo.lock",
                ".cargo/tests.toml",
            ]
            .iter()
            .any(|p| !has_role(p, "config"))
        {
            return Err(refusal(
                "closure requires source/target/fixture and Cargo manifest/lock/runner config",
            ));
        }
        let a = &decoded.compiler_artifact;
        let t = &a.target;
        // target.test controls default selection; profile.test identifies the actual test build.
        if a.reason != "compiler-artifact"
            || !a.profile.test
            || !text(&a.package_id)
            || !text(&t.name)
            || t.kind.is_empty()
            || t.kind
                .iter()
                .any(|v| !matches!(v.as_str(), "test" | "bin" | "lib" | "rlib" | "cdylib"))
            || t.crate_types.is_empty()
            || t.crate_types
                .iter()
                .any(|v| !matches!(v.as_str(), "bin" | "lib" | "rlib" | "cdylib"))
            || a.filenames.is_empty()
            || a.features
                .iter()
                .chain(&t.required_features)
                .any(|v| !text(v))
            || !text(&a.profile.opt_level)
        {
            return Err(refusal(
                "metadata must identify an ordinary compiled libtest artifact",
            ));
        }
        if !matches!(t.edition.as_str(), "2015" | "2018" | "2021" | "2024")
            || !(a.profile.debuginfo.is_null()
                || a.profile.debuginfo.is_u64()
                || a.profile.debuginfo.is_string())
        {
            return Err(refusal("unsupported compiler profile/edition metadata"));
        }
        for (artifact_path, role) in [(&a.manifest_path, "config"), (&t.src_path, "target")] {
            if !artifact_path.is_absolute() || !has_role(&relative(root, artifact_path)?, role) {
                return Err(refusal("compiler source/manifest is absent from closure"));
            }
        }
        if !a.executable.is_absolute() || relative(root, &a.executable)? != binary_name {
            return Err(refusal("compiler artifact does not bind this executable"));
        }
        for filename in &a.filenames {
            if !filename.is_absolute() || !relative(root, filename)?.starts_with("native/target/") {
                return Err(refusal(
                    "compiler output filename is not a project target artifact",
                ));
            }
        }
        if !a.filenames.iter().any(|p| p == &a.executable) {
            return Err(refusal("compiler filenames omit the executable"));
        }
        // These typed fields are retained verbatim; they are metadata, not proof of compilation.
        let _ = (
            &a.features,
            a.fresh,
            &t.required_features,
            t.doc,
            t.doctest,
            t.test,
            &a.profile.opt_level,
            a.profile.debug_assertions,
            a.profile.overflow_checks,
        );
        let binary_hash = read_file(binary, BINARY_LIMIT, false)?.0;
        let binary_identity = identity(&regular(binary)?)?;
        Ok(Self {
            value,
            hash,
            binary_hash,
            binary_name,
            bytes,
            root: root.into(),
            identity: binary_identity,
            manifest: path.into(),
            binary: binary.into(),
            members,
        })
    }
    pub(super) fn recheck(&self) -> Result<(), String> {
        if read_file(&self.manifest, SMALL_LIMIT, false)?.0 != self.hash
            || read_file(&self.binary, BINARY_LIMIT, false)?.0 != self.binary_hash
            || identity(&regular(&self.binary)?)? != self.identity
        {
            return Err(refusal("manifest or executable changed during observation"));
        }
        for (name, hash) in &self.members {
            if &read_file(&self.root.join(name), SMALL_LIMIT, false)?.0 != hash {
                return Err(refusal("closure member changed during observation"));
            }
        }
        Ok(())
    }
    pub(crate) fn report(&self) -> Value {
        json!({"target_revision":self.value["target_revision"],"baseline_revision":self.value["baseline_revision"],"manifest_sha256":self.hash,"executable_sha256":self.binary_hash,"executable":self.binary_name,"files":self.value["files"]})
    }
}
