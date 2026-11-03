//! Expected worker refusals release only their verified envelope; unexpected failures retain it.
use serde_json::Value;
use std::io::Read;
use std::path::{Path, PathBuf};

pub struct Evidence {
    root: PathBuf,
    directory: PathBuf,
    request: Value,
    response: Value,
}

fn read(path: &Path) -> Value {
    _native::tooling_environment::reject_links(path, "linked oracle evidence").unwrap();
    let file = std::fs::File::open(path).unwrap();
    let metadata = file.metadata().unwrap();
    assert!(metadata.is_file() && metadata.len() <= 512 * 1024);
    let mut bytes = Vec::new();
    file.take(512 * 1024 + 1).read_to_end(&mut bytes).unwrap();
    assert!(bytes.len() <= 512 * 1024);
    serde_json::from_slice(&bytes).unwrap()
}

fn inspect(root: &Path, directory: &Path) -> (Value, Value) {
    let parent = _native::tooling_environment::confined_directory(root, "logs/tmp", false).unwrap();
    assert!(directory.is_absolute());
    assert_eq!(directory.parent(), Some(parent.as_path()));
    let prefix = format!("source-oracle-{}-", std::process::id());
    let sequence = directory
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .strip_prefix(&prefix)
        .expect("evidence must belong to this test process");
    assert!(!sequence.is_empty() && sequence.bytes().all(|byte| byte.is_ascii_digit()));
    assert_eq!(
        _native::tooling_environment::physical_root(directory).unwrap(),
        directory
    );
    assert!(matches!(
        std::fs::symlink_metadata(directory.join("owner")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound
    ));
    assert!(!_native::process_owner::cleanup_pending(directory));
    let mut entries = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    entries.sort();
    assert_eq!(
        entries,
        ["request.json", "response.json"].map(std::ffi::OsString::from)
    );
    (
        read(&directory.join("request.json")),
        read(&directory.join("response.json")),
    )
}

pub fn refusal(
    root: &Path,
    oracle: &str,
    fixture: &Value,
    result: Result<Value, String>,
    expected: &str,
) -> Evidence {
    let error = result.expect_err("changed observation must be refused");
    let (message, directory) = error
        .rsplit_once("; source oracle evidence retained at ")
        .expect("expected a completed worker refusal with retained evidence");
    assert!(message.contains(expected), "unexpected refusal: {error}");
    let root = _native::tooling_environment::physical_root(root).unwrap();
    let directory = PathBuf::from(directory);
    let (request, response) = inspect(&root, &directory);
    assert_eq!(request["oracle"], oracle);
    assert_eq!(&request["fixture"], fixture);
    assert_eq!(response["ok"], false);
    assert_eq!(response["error"].as_str(), Some(message));
    Evidence {
        root,
        directory,
        request,
        response,
    }
}

impl Evidence {
    pub fn clean_gallery(self, original: &Value, real: bool) {
        use base64::Engine;
        self.validate();
        assert_eq!(self.request["oracle"], "gallery");
        let (_, path) = self.response["error"]
            .as_str()
            .unwrap()
            .rsplit_once("; historical gallery evidence retained at ")
            .expect("failed gallery must retain its thumbnail evidence");
        let path = PathBuf::from(path);
        let parent =
            _native::tooling_environment::confined_directory(&self.root, "logs/tmp", false)
                .unwrap();
        assert_eq!(path.parent(), Some(parent.as_path()));
        assert_eq!(
            _native::tooling_environment::physical_root(&path).unwrap(),
            path
        );
        let name = path.file_name().unwrap().to_str().unwrap();
        let (pid, sequence) = name
            .strip_prefix("source-gallery-")
            .unwrap()
            .split_once('-')
            .unwrap();
        assert!(
            [pid, sequence]
                .iter()
                .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
        );
        assert!(!_native::process_owner::cleanup_pending(&path));
        let entries = std::fs::read_dir(&path)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<Vec<_>>();
        if real {
            let cid = original["real_static"]["document"]["emoji"][0]["custom_emoji_id"]
                .as_str()
                .unwrap();
            assert!(!cid.is_empty() && cid.bytes().all(|byte| byte.is_ascii_digit()));
            let filename = format!("{cid}.webp");
            assert_eq!(entries, [std::ffi::OsString::from(filename.as_str())]);
            let thumbnail = path.join(filename);
            _native::tooling_environment::reject_links(&thumbnail, "linked gallery evidence")
                .unwrap();
            let file = std::fs::File::open(&thumbnail).unwrap();
            assert!(file.metadata().unwrap().is_file());
            let mut bytes = Vec::new();
            file.take(2 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .unwrap();
            assert!(!bytes.is_empty() && bytes.len() <= 2 * 1024 * 1024);
            let uri = format!(
                "src=\"data:image/webp;base64,{}\"",
                base64::engine::general_purpose::STANDARD.encode(&bytes)
            );
            assert!(
                original["real_static"]["expected"]
                    .as_str()
                    .unwrap()
                    .contains(&uri)
            );
            std::fs::remove_file(thumbnail).unwrap();
        } else {
            assert!(
                entries.is_empty(),
                "early observation drift must not render art"
            );
        }
        std::fs::remove_dir(path).expect("clean verified gallery refusal evidence");
        self.clean();
    }

    fn validate(&self) {
        let (request, response) = inspect(&self.root, &self.directory);
        assert_eq!(request, self.request);
        assert_eq!(response, self.response);
    }

    pub fn clean(self) {
        // No Drop cleanup: an assertion after admission must keep the failed observation.
        self.validate();
        std::fs::remove_dir_all(&self.directory).expect("clean verified expected oracle refusal");
        assert!(matches!(
            std::fs::symlink_metadata(&self.directory),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound
        ));
    }
}

#[test]
fn expected_refusal_cleanup_requires_unchanged_owned_evidence_and_finished_owner() {
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let parent =
        _native::tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
    let root = parent.join(format!(
        "oracle evidence café {} {}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    let root = _native::tooling_environment::physical_root(&root).unwrap();
    let parent = _native::tooling_environment::confined_directory(&root, "logs/tmp", true).unwrap();
    let directory = parent.join(format!("source-oracle-{}-0", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let fixture = json!({"cases":[{"expected":"changed"}]});
    let request = json!({"oracle":"intent","fixture":fixture});
    let response = json!({"ok":false,"error":"intent fixture drift"});
    let request_path = directory.join("request.json");
    let response_path = directory.join("response.json");
    std::fs::write(&request_path, request.to_string().as_bytes()).unwrap();
    std::fs::write(&response_path, response.to_string().as_bytes()).unwrap();
    let error = format!(
        "intent fixture drift; source oracle evidence retained at {}",
        directory.display()
    );
    let admit = || {
        refusal(
            &root,
            "intent",
            &fixture,
            Err(error.clone()),
            "fixture drift",
        )
    };
    drop(admit());
    assert!(response_path.is_file());
    let interrupted = std::panic::catch_unwind(|| {
        let _evidence = admit();
        panic!("controlled assertion after evidence admission");
    });
    assert!(interrupted.is_err());
    for mutation in ["owner", "pending", "response", "request", "unexpected-file"] {
        let blocker = match mutation {
            "owner" => {
                let path = directory.join("owner");
                std::fs::create_dir(&path).unwrap();
                Some(path)
            }
            "pending" | "unexpected-file" => {
                let path = directory.join(if mutation == "pending" {
                    "codec-cleanup-pending.json"
                } else {
                    "unresolved-artifact"
                });
                std::fs::write(&path, b"retain").unwrap();
                Some(path)
            }
            "response" => {
                std::fs::write(&response_path, b"{\"ok\":true}").unwrap();
                None
            }
            "request" => {
                std::fs::write(&request_path, b"{\"oracle\":\"other\"}").unwrap();
                None
            }
            _ => unreachable!(),
        };
        assert!(std::panic::catch_unwind(admit).is_err(), "{mutation}");
        assert!(request_path.is_file() && response_path.is_file());
        if let Some(path) = blocker {
            if mutation == "owner" {
                std::fs::remove_dir(path).unwrap();
            } else {
                std::fs::remove_file(path).unwrap();
            }
        }
        std::fs::write(&request_path, request.to_string().as_bytes()).unwrap();
        std::fs::write(&response_path, response.to_string().as_bytes()).unwrap();
    }
    let evidence = admit();
    std::fs::write(
        &response_path,
        b"{\"ok\":false,\"error\":\"changed after admission\"}",
    )
    .unwrap();
    assert!(std::panic::catch_unwind(|| evidence.clean()).is_err());
    assert!(response_path.is_file());
    std::fs::write(&response_path, response.to_string().as_bytes()).unwrap();
    let sentinel = parent.join("keep");
    std::fs::write(&sentinel, b"unrelated evidence").unwrap();
    admit().clean();
    let original: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/backend/gallery.json")).unwrap();
    let encoded = original["real_static"]["expected"]
        .as_str()
        .unwrap()
        .split_once("src=\"data:image/webp;base64,")
        .unwrap()
        .1
        .split_once('"')
        .unwrap()
        .0;
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .unwrap();
    let gallery = parent.join(format!("source-gallery-{}-0", std::process::id()));
    std::fs::create_dir(&gallery).unwrap();
    let thumbnail = gallery.join("111111111.webp");
    std::fs::create_dir(&directory).unwrap();
    let request = json!({"oracle":"gallery","fixture":original});
    let message = format!(
        "gallery fixture drift; historical gallery evidence retained at {}",
        gallery.display()
    );
    std::fs::write(&request_path, request.to_string().as_bytes()).unwrap();
    std::fs::write(
        &response_path,
        json!({"ok":false,"error":message}).to_string().as_bytes(),
    )
    .unwrap();
    let admit = || {
        refusal(
            &root,
            "gallery",
            &original,
            Err(format!(
                "{message}; source oracle evidence retained at {}",
                directory.display()
            )),
            "gallery fixture drift",
        )
    };
    let mut extra = bytes.clone();
    extra.push(0);
    for changed in [&bytes[..bytes.len() - 3], extra.as_slice()] {
        std::fs::write(&thumbnail, changed).unwrap();
        assert!(std::panic::catch_unwind(|| admit().clean_gallery(&original, true)).is_err());
        assert_eq!(std::fs::read(&thumbnail).unwrap(), changed);
        assert!(response_path.is_file());
    }
    std::fs::write(&thumbnail, &bytes).unwrap();
    admit().clean_gallery(&original, true);
    assert!(!gallery.exists() && !directory.exists());
    assert_eq!(std::fs::read(&sentinel).unwrap(), b"unrelated evidence");
    std::fs::remove_file(sentinel).unwrap();
    std::fs::remove_dir(parent).unwrap();
    std::fs::remove_dir(root.join("logs")).unwrap();
    std::fs::remove_dir(root).unwrap();
}
