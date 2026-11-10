//! Pinned read-only Git corpus: enumerate independently, hash bytes, never import source modules.
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT: AtomicU64 = AtomicU64::new(0);
const SOURCE_LIMIT: usize = 2 * 1024 * 1024;
const CORPUS_LIMIT: u64 = 32 * 1024 * 1024;

pub struct Source {
    pub baseline_revision: String,
    pub path: String,
    pub sha256: String,
    pub text: String,
}

fn revision(value: &str) -> Result<(), String> {
    if value.len() != 40
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
    {
        return Err("source snapshot requires an exact lowercase40hex revision".into());
    }
    Ok(())
}

pub async fn snapshot(root: &Path, baseline: &str) -> Result<Vec<Source>, String> {
    let environment = std::env::vars_os().collect();
    snapshot_with_environment(root, baseline, &environment).await
}

pub fn git_environment(
    inherited: &BTreeMap<std::ffi::OsString, std::ffi::OsString>,
) -> BTreeMap<std::ffi::OsString, std::ffi::OsString> {
    // A foreign GIT_DIR can report this worktree while reading another object database.
    let mut environment = inherited
        .iter()
        .filter(|(key, _)| {
            !key.to_string_lossy()
                .to_ascii_uppercase()
                .starts_with("GIT_")
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<BTreeMap<_, _>>();
    for (key, value) in [
        ("GIT_NO_LAZY_FETCH", "1"),
        ("GIT_OPTIONAL_LOCKS", "0"),
        ("GIT_TERMINAL_PROMPT", "0"),
    ] {
        environment.insert(key.into(), value.into());
    }
    environment
}

async fn repository(
    root: &Path,
    environment: &BTreeMap<std::ffi::OsString, std::ffi::OsString>,
) -> Result<(std::path::PathBuf, std::path::PathBuf), String> {
    let root = crate::tooling_environment::physical_root(root)?;
    let git = crate::codec_process::executable("git")?;
    let args = ["--no-replace-objects", "rev-parse", "--show-toplevel"].map(str::to_owned);
    let location = crate::codec_process::capture_with_environment(
        &root,
        &git,
        &args,
        16 * 1024,
        None,
        environment,
    )
    .await?;
    let location = std::str::from_utf8(&location.stdout)
        .map_err(|_| "Git root is not UTF-8")?
        .trim();
    if crate::tooling_environment::physical_root(Path::new(location))? != root {
        return Err("source snapshot root must be the owning Git repository".into());
    }
    Ok((root, git))
}

/// Retrieve one regular historical source without parsing or executing the rest of the corpus.
pub async fn read_source(root: &Path, baseline: &str, path: &str) -> Result<Source, String> {
    revision(baseline)?;
    crate::source_test_inventory::source_path(path)?;
    if path.chars().any(char::is_control) {
        return Err("historical source path cannot contain control characters".into());
    }
    let environment = crate::tooling_package_cli::git_environment(&std::env::vars_os().collect());
    let (root, git) = repository(root, &environment).await?;
    let args = [
        "--no-replace-objects",
        "--literal-pathspecs",
        "ls-tree",
        "-z",
        "--full-tree",
        baseline,
        "--",
        path,
    ]
    .map(str::to_owned);
    let tree = crate::codec_process::capture_with_environment(
        &root,
        &git,
        &args,
        16 * 1024,
        None,
        &environment,
    )
    .await?;
    let record = tree
        .stdout
        .strip_suffix(&[0])
        .ok_or("historical source is missing")?;
    let record = std::str::from_utf8(record).map_err(|_| "historical source entry is not UTF-8")?;
    let (header, actual) = record
        .split_once('\t')
        .ok_or("invalid historical source entry")?;
    let fields = header.split_whitespace().collect::<Vec<_>>();
    if actual != path
        || fields.len() != 3
        || !matches!(fields[0], "100644" | "100755")
        || fields[1] != "blob"
    {
        return Err("historical source must be one regular nonlinked blob".into());
    }
    revision(fields[2])?;
    let args = ["--no-replace-objects", "cat-file", "blob", fields[2]].map(str::to_owned);
    let captured = crate::codec_process::capture_with_environment(
        &root,
        &git,
        &args,
        SOURCE_LIMIT as u64,
        None,
        &environment,
    )
    .await?;
    let sha256 = Sha256::digest(&captured.stdout)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    let text = String::from_utf8(captured.stdout).map_err(|_| "historical source is not UTF-8")?;
    Ok(Source {
        baseline_revision: baseline.into(),
        path: path.into(),
        sha256,
        text,
    })
}

pub async fn snapshot_with_environment(
    root: &Path,
    baseline: &str,
    inherited: &BTreeMap<std::ffi::OsString, std::ffi::OsString>,
) -> Result<Vec<Source>, String> {
    revision(baseline)?;
    let environment = git_environment(inherited);
    let (root, git) = repository(root, &environment).await?;
    let args = [
        "--no-replace-objects",
        "ls-tree",
        "-rz",
        "--full-tree",
        baseline,
    ]
    .map(str::to_owned);
    let tree = crate::codec_process::capture_with_environment(
        &root,
        &git,
        &args,
        1024 * 1024,
        None,
        &environment,
    )
    .await?;
    let files = tree_sources(&tree.stdout, None)?;
    read_blobs(&root, &git, baseline, files, &environment).await
}

fn tree_sources(
    bytes: &[u8],
    selected: Option<&std::collections::BTreeSet<&str>>,
) -> Result<BTreeMap<String, String>, String> {
    let records = bytes
        .strip_suffix(&[0])
        .filter(|records| !records.is_empty())
        .ok_or("Git source tree is empty or lacks its NUL terminator")?;
    let mut files = BTreeMap::new();
    for record in records.split(|byte| *byte == 0) {
        let record = std::str::from_utf8(record).map_err(|_| "Git source tree is not UTF-8")?;
        let (header, path) = record.split_once('\t').ok_or("invalid Git tree record")?;
        if selected.is_some_and(|paths| !paths.contains(path)) {
            return Err("historical source tree returned an unrequested path".into());
        }
        if !path.ends_with(".py") {
            continue;
        }
        crate::source_test_inventory::source_path(path)?;
        if path.chars().any(char::is_control) {
            return Err("Git source path cannot contain control characters".into());
        }
        let parts = header.split_whitespace().collect::<Vec<_>>();
        if parts.len() != 3 || !matches!(parts[0], "100644" | "100755") || parts[1] != "blob" {
            return Err("source snapshot refuses linked or non-blob Python sources".into());
        }
        revision(parts[2])?;
        if files.insert(path.to_owned(), parts[2].to_owned()).is_some() || files.len() > 4096 {
            return Err("source tree repeats paths or exceeds4096files".into());
        }
    }
    if files.is_empty() {
        return Err("pinned source tree has no Python sources".into());
    }
    if selected.is_some_and(|paths| files.len() != paths.len()) {
        return Err("historical source tree omitted a requested path".into());
    }
    Ok(files)
}

/// Read an explicit nonempty set of historical files with one admitted tree and blob batch.
pub async fn read_sources(
    root: &Path,
    baseline: &str,
    paths: &[&str],
) -> Result<Vec<Source>, String> {
    revision(baseline)?;
    if paths.is_empty() || paths.len() > 4096 {
        return Err("historical source selection requires1..4096paths".into());
    }
    let mut selected = std::collections::BTreeSet::new();
    for path in paths {
        crate::source_test_inventory::source_path(path)?;
        if path.chars().any(char::is_control) || !selected.insert(*path) {
            return Err("historical source selection repeats paths or contains controls".into());
        }
    }
    let environment = git_environment(&std::env::vars_os().collect());
    let (root, git) = repository(root, &environment).await?;
    let mut arguments = [
        "--no-replace-objects",
        "--literal-pathspecs",
        "ls-tree",
        "-rz",
        "--full-tree",
        baseline,
        "--",
    ]
    .map(str::to_owned)
    .to_vec();
    arguments.extend(selected.iter().map(|path| (*path).to_owned()));
    let tree = crate::codec_process::capture_with_environment(
        &root,
        &git,
        &arguments,
        1024 * 1024,
        None,
        &environment,
    )
    .await?;
    let files = tree_sources(&tree.stdout, Some(&selected))?;
    read_blobs(&root, &git, baseline, files, &environment).await
}

async fn read_blobs(
    root: &Path,
    git: &Path,
    baseline: &str,
    files: BTreeMap<String, String>,
    environment: &BTreeMap<std::ffi::OsString, std::ffi::OsString>,
) -> Result<Vec<Source>, String> {
    let parent = root.join("logs/tmp");
    crate::codec_worker::physical_directory(root, &parent)?;
    let marker = parent.join(format!(
        "source-inventory-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let input = crate::codec_video_encode::Staged::new(&marker, "stdin")?;
    let prepared = (|| {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&input.0)
            .map_err(|_| "cannot open source request")?;
        for oid in files.values() {
            writeln!(file, "{oid}").map_err(|_| "cannot write source request")?;
        }
        file.sync_all().map_err(|_| "cannot persist source request")
    })();
    if let Err(error) = prepared {
        input.clean()?;
        return Err(error.into());
    }
    let args = ["--no-replace-objects", "cat-file", "--batch"].map(str::to_owned);
    let captured = crate::codec_process::capture_with_environment(
        root,
        git,
        &args,
        CORPUS_LIMIT,
        Some(&input.0),
        environment,
    )
    .await;
    input.clean()?;
    let captured = captured?;
    parse_blobs(baseline, files, &captured.stdout)
}

fn parse_blobs(
    baseline: &str,
    files: BTreeMap<String, String>,
    bytes: &[u8],
) -> Result<Vec<Source>, String> {
    let mut remaining = bytes;
    let mut sources = Vec::new();
    for (path, oid) in files {
        let end = remaining
            .iter()
            .position(|byte| *byte == b'\n')
            .ok_or("truncated Git blob header")?;
        if end > 256 {
            return Err("Git blob header exceeds256bytes".into());
        }
        let header =
            std::str::from_utf8(&remaining[..end]).map_err(|_| "invalid Git blob header")?;
        let parts = header.split_whitespace().collect::<Vec<_>>();
        if parts.len() != 3 || parts[0] != oid || parts[1] != "blob" {
            return Err("Git source blob identity mismatch".into());
        }
        let size = parts[2]
            .parse::<usize>()
            .map_err(|_| "invalid Git source byte length")?;
        if size > SOURCE_LIMIT {
            return Err("source blob exceeds2MiB".into());
        }
        remaining = &remaining[end + 1..];
        if remaining.len() <= size || remaining[size] != b'\n' {
            return Err("truncated Git source blob".into());
        }
        let bytes = &remaining[..size];
        let text = std::str::from_utf8(bytes)
            .map_err(|_| format!("source is not UTF-8: {path}"))?
            .to_owned();
        let sha256 = Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        sources.push(Source {
            baseline_revision: baseline.into(),
            path,
            sha256,
            text,
        });
        remaining = &remaining[size + 1..];
    }
    if !remaining.is_empty() {
        return Err("unexpected trailing Git source data".into());
    }
    Ok(sources)
}

pub fn inventory(sources: &[Source]) -> Result<Value, String> {
    let baseline = sources
        .first()
        .ok_or("source corpus is empty")?
        .baseline_revision
        .as_str();
    revision(baseline)?;
    let mut files = Vec::new();
    let discoveries = crate::source_module_discovery::discover(sources)?;
    let mut complete = true;
    let mut seen = std::collections::BTreeSet::new();
    for source in sources {
        crate::source_test_inventory::source_path(&source.path)?;
        if !seen.insert(&source.path) {
            return Err("source corpus repeats a path".into());
        }
        let digest = Sha256::digest(source.text.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        if digest != source.sha256 {
            return Err("source corpus text/hash mismatch".into());
        }
        if source.baseline_revision != baseline {
            return Err("source corpus mixes revisions".into());
        }
        let metadata = crate::source_test_inventory::parse(&source.path, &source.text)?;
        let discovery = discoveries
            .get(&source.path)
            .ok_or("source corpus discovery omitted a file")?;
        let assertions = crate::source_assertions::parse(&source.path, &source.text)?;
        complete &= discovery["discovery_complete"] == true;
        let mut cases = discovery["case_origins"]
            .as_array()
            .ok_or("source discovery returned no case origins")?
            .iter()
            .map(|case| {
                format!(
                    "{}.{}",
                    case["binding"].as_str().unwrap_or(""),
                    case["method"].as_str().unwrap_or("")
                )
            })
            .collect::<Vec<_>>();
        // Preserve declared cases even where decorators/imported bases prevent static collection.
        // An unresolved class is not permission to erase its original assertions from coverage.
        for class in metadata["classes"]
            .as_array()
            .ok_or("source metadata requires classes")?
        {
            for method in class["methods"]
                .as_array()
                .ok_or("source metadata requires methods")?
            {
                let name = method["name"]
                    .as_str()
                    .ok_or("source metadata requires method name")?;
                if name.starts_with("test") {
                    cases.push(format!(
                        "{}.{}",
                        class["name"]
                            .as_str()
                            .ok_or("source metadata requires class name")?,
                        name
                    ));
                }
            }
        }
        cases.sort();
        cases.dedup();
        files.push(
            serde_json::json!({"source":source.path,"sha256":source.sha256,
            "test_cases":cases,"metadata":metadata,"discovery":discovery,"assertions":assertions}),
        );
    }
    let case_assertions = crate::source_assertions::resolve_cases(&files)?;
    for file in &mut files {
        let path = file["source"]
            .as_str()
            .ok_or("source inventory requires file path")?;
        let evidence = case_assertions
            .get(path)
            .ok_or("source assertion join omitted a file")?
            .clone();
        file["case_assertions"] = evidence;
    }
    Ok(
        serde_json::json!({"baseline_revision":baseline,"files":files,"discovery_complete":complete}),
    )
}

pub fn verify(sources: &[Source], inventory: &Value) -> Result<(), String> {
    revision(
        inventory["baseline_revision"]
            .as_str()
            .ok_or("source inventory requires pinned revision")?,
    )?;
    let mut expected = BTreeMap::new();
    for file in inventory["files"]
        .as_array()
        .ok_or("source inventory requires files")?
    {
        let path = file["source"]
            .as_str()
            .ok_or("source inventory requires path")?;
        crate::source_test_inventory::source_path(path)?;
        let digest = file["sha256"]
            .as_str()
            .ok_or("source inventory requires hash")?;
        if expected.insert(path, digest).is_some() {
            return Err("source inventory repeats a path".into());
        }
    }
    let actual = sources
        .iter()
        .map(|source| (source.path.as_str(), source.sha256.as_str()))
        .collect::<BTreeMap<_, _>>();
    if sources.iter().any(|source| {
        source.baseline_revision != inventory["baseline_revision"].as_str().unwrap_or("")
    }) || sources.len() != actual.len()
        || actual.is_empty()
        || actual != expected
    {
        return Err("pinned source inventory has omitted, extra or changed source files".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    const OID: &str = "1111111111111111111111111111111111111111";

    #[test]
    fn tree_admission_requires_exact_regular_unique_terminated_selection() {
        let record = |mode: &str, kind: &str, oid: &str, path: &str| {
            format!("{mode} {kind} {oid}\t{path}\0").into_bytes()
        };
        let valid = record("100644", "blob", OID, "tests/a.py");
        let selected = BTreeSet::from(["tests/a.py"]);
        assert_eq!(
            tree_sources(&valid, Some(&selected)).unwrap(),
            BTreeMap::from([("tests/a.py".to_owned(), OID.to_owned())])
        );
        let executable = record("100755", "blob", OID, "tests/a.py");
        assert!(tree_sources(&executable, Some(&selected)).is_ok());
        let other = record("100644", "blob", OID, "README.md");
        assert_eq!(
            tree_sources(&[valid.clone(), other.clone()].concat(), None)
                .unwrap()
                .len(),
            1
        );
        let missing = BTreeSet::from(["tests/a.py", "tests/b.py"]);
        assert!(tree_sources(&valid, Some(&missing)).is_err());
        for invalid in [
            Vec::new(),
            vec![0],
            valid[..valid.len() - 1].to_vec(),
            [valid.clone(), vec![0]].concat(),
            [valid.clone(), valid.clone()].concat(),
            [valid.clone(), other].concat(),
            record("100644", "blob", OID, "tests/b.py"),
            record("120000", "blob", OID, "tests/a.py"),
            record("040000", "tree", OID, "tests/a.py"),
            record("100644", "commit", OID, "tests/a.py"),
            record("100644", "blob", &"A".repeat(40), "tests/a.py"),
            record("100644", "blob", "123", "tests/a.py"),
            b"bad record\0".to_vec(),
            vec![0xff, 0],
        ] {
            assert!(
                tree_sources(&invalid, Some(&selected)).is_err(),
                "{invalid:?}"
            );
        }
        for path in [
            "../a.py",
            "/a.py",
            "tests/a\n.py",
            "tests/a\t.py",
            "tests\\a.py",
        ] {
            assert!(
                tree_sources(&record("100644", "blob", OID, path), None).is_err(),
                "{path:?}"
            );
        }
    }

    #[test]
    fn blob_batch_preserves_exact_bytes_and_refuses_missing_mismatched_or_partial_records() {
        let files = || BTreeMap::from([("tests/a.py".to_owned(), OID.to_owned())]);
        let response = |oid: &str, kind: &str, size: &str, bytes: &[u8]| {
            [format!("{oid} {kind} {size}\n").as_bytes(), bytes, b"\n"].concat()
        };
        let content = b"a\r\n";
        let valid = response(OID, "blob", "3", content);
        let sources = parse_blobs(OID, files(), &valid).unwrap();
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].path, "tests/a.py");
        assert_eq!(sources[0].baseline_revision, OID);
        assert_eq!(sources[0].text.as_bytes(), content);
        assert_eq!(sources[0].sha256, crate::codec_raster::digest_hex(content));
        assert_eq!(
            parse_blobs(OID, files(), &response(OID, "blob", "0", b"")).unwrap()[0].text,
            ""
        );
        let repeated = BTreeMap::from([
            ("tests/a.py".to_owned(), OID.to_owned()),
            ("tests/b.py".to_owned(), OID.to_owned()),
        ]);
        assert_eq!(
            parse_blobs(OID, repeated, &[valid.clone(), valid.clone()].concat())
                .unwrap()
                .len(),
            2
        );
        for invalid in [
            Vec::new(),
            format!("{OID} missing\n").into_bytes(),
            response(&"2".repeat(40), "blob", "3", content),
            response(OID, "tree", "3", content),
            response(OID, "blob", "invalid", content),
            response(OID, "blob", "-1", content),
            response(OID, "blob", &(SOURCE_LIMIT + 1).to_string(), b""),
            response(OID, "blob", "4", content),
            response(OID, "blob", "2", content),
            response(OID, "blob", "1", &[0xff]),
            valid[..valid.len() - 1].to_vec(),
            [valid.clone(), b"trailing".to_vec()].concat(),
            [vec![b'x'; 257], b"\n".to_vec()].concat(),
        ] {
            assert!(parse_blobs(OID, files(), &invalid).is_err(), "{invalid:?}");
        }
    }
}
