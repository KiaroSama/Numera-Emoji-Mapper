use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};
pub const UTC: &str = "2026-01-01T00:00:00Z";
static NEXT: AtomicU64 = AtomicU64::new(0);
pub fn project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}
pub struct Root(pub PathBuf);
impl Root {
    pub fn new() -> Self {
        let base = project().join("logs/tmp/baseline-packs");
        std::fs::create_dir_all(&base).unwrap();
        loop {
            let p = base.join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&p) {
                Ok(()) => return Self(p),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("{e}"),
            }
        }
    }
    pub fn write(&self, name: &str, bytes: impl AsRef<[u8]>) -> PathBuf {
        let p = self.0.join(name);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, bytes).unwrap();
        p
    }
    pub fn config(&self, values: &[(&str, &str)]) -> numera_runtime::config::Config {
        let mut e = std::collections::BTreeMap::from([(
            "NUMERA_EMOJI_MAPPER_NO_DOTENV".into(),
            "1".into(),
        )]);
        e.extend(values.iter().map(|(k, v)| (k.to_string(), v.to_string())));
        numera_runtime::config::Config::load_with_environment(self.0.clone(), &e).unwrap()
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
pub fn read(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
pub fn live(ids: &[&str]) -> Vec<Value> {
    ids.iter()
        .map(|c| json!({"custom_emoji_id":c,"file_id":format!("f-{c}")}))
        .collect()
}
pub fn binary() -> PathBuf {
    if let Some(path) = option_env!("CARGO_BIN_EXE_numera-emoji") {
        return PathBuf::from(path);
    }
    let current = std::env::current_exe().unwrap();
    let name = if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    };
    current
        .ancestors()
        .skip(1)
        .map(|directory| directory.join(name))
        .find(|path| path.is_file())
        .expect("Cargo companion executable beside test artifacts")
}
pub mod http;
