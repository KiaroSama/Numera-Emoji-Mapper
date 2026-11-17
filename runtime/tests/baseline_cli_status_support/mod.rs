use crate::baseline_catalog_support as catalog_support;
pub use catalog_support::{Scratch, UTC, read_json};
use numera_runtime::{config::Config, process_owner::OwnedProcess};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
static RUN: AtomicU64 = AtomicU64::new(0);
pub mod http;
pub fn project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .into()
}
pub fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}
pub fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}
pub fn document(path: &Path, value: &Value) {
    write(path, serde_json::to_vec(value).unwrap());
}
pub struct Fixture {
    pub scratch: Scratch,
    pub root: PathBuf,
    pub binary: PathBuf,
}
pub struct Run {
    pub code: i32,
    pub out: String,
    pub err: String,
}
impl Run {
    pub fn code(&self, expected: i32) {
        assert_eq!(
            self.code, expected,
            "stdout={}\nstderr={}",
            self.out, self.err
        );
    }
}
impl Fixture {
    pub fn new() -> Self {
        let scratch = Scratch::new();
        let root = scratch.0.join("fixture-root");
        write(
            &root.join("assets/panel.html"),
            std::fs::read(project().join("assets/panel.html")).unwrap(),
        );
        let binary = root.join(format!("numera-emoji{}", std::env::consts::EXE_SUFFIX));
        std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), &binary).unwrap();
        Self {
            scratch,
            root,
            binary,
        }
    }
    pub fn data(&self) -> PathBuf {
        self.root.join("collection")
    }
    pub fn config(&self, extra: &[(&str, String)]) -> Config {
        let mut env = BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]);
        env.extend(extra.iter().map(|(k, v)| (k.to_string(), v.clone())));
        Config::load_with_environment(self.root.clone(), &env).unwrap()
    }
    pub fn catalog(&self) -> numera_runtime::catalog::Catalog {
        numera_runtime::catalog::Catalog::open(&self.data().join("catalog.db"), UTC, &self.root)
            .unwrap()
    }
    pub fn run(&self, args: &[&str], extra: &[(&str, String)]) -> Run {
        let capture = self
            .scratch
            .0
            .join(format!("capture-{}", RUN.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir(&capture).unwrap();
        let stdout = capture.join("stdout");
        let stderr = capture.join("stderr");
        let mut cmd = Command::new(&self.binary);
        cmd.args(args)
            .current_dir(&self.scratch.0)
            .stdin(Stdio::null())
            .stdout(std::fs::File::create(&stdout).unwrap())
            .stderr(std::fs::File::create(&stderr).unwrap())
            .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
            .env("BRAND_LOGO_BOTS", "")
            .env("GENERAL_BOT_TOKEN", "test-only-token")
            .env("TELEGRAM_BOT_TOKEN", "test-coin-token")
            .env("COLLECTION_PACK_BASE", "fixture")
            .env("EMOJI_ARCHIVE_DIR", "")
            .env("COIN_EMOJI_DIR", "")
            .env("PACK_OWNER_USER_ID", "111111111")
            .env("PACK_LINKS_CHAT_ID", "")
            .env("WORKER_PUBLISH_URL", "")
            .env("WORKER_PUBLISH_SECRET", "");
        for (k, v) in extra {
            cmd.env(k, v);
        }
        let mut child = OwnedProcess::spawn_session(&mut cmd, capture.clone()).unwrap();
        let status =
            runtime().block_on(child.wait(Duration::from_secs(150), Duration::from_secs(140)));
        let out = std::fs::read_to_string(&stdout).unwrap();
        let err = std::fs::read_to_string(&stderr).unwrap();
        child.finish().unwrap();
        let status = status.unwrap_or_else(|error| panic!("CLI failed {error}; {out}; {err}"));
        Run {
            code: status.code().unwrap_or(-1),
            out,
            err,
        }
    }
    pub fn current_roster(&self) {
        let config = self.config(&[("COLLECTION_PACK_BASE", "fixture".into())]);
        document(
            &self.root.join("packs/index.json"),
            &json!({"inputs":numera_runtime::roster_cli::fingerprint(&config).unwrap()}),
        );
    }
}
pub fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn visit(base: &Path, at: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        if !at.exists() {
            return;
        }
        for entry in std::fs::read_dir(at).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(base, &path, out)
            } else {
                let name = path.file_name().unwrap().to_string_lossy();
                if !name.ends_with("-wal") && !name.ends_with("-shm") {
                    out.insert(
                        path.strip_prefix(base).unwrap().into(),
                        std::fs::read(&path).unwrap(),
                    );
                }
            }
        }
    }
    let mut out = BTreeMap::new();
    visit(root, root, &mut out);
    out
}
