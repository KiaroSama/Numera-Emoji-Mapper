use numera_runtime::{catalog::Catalog, config::Config};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const UTC: &str = "2026-01-01T00:00:00Z";
pub const HOSTILE: &str = "left|right\nnext line";
pub const ESCAPED: &str = "left\\|right next line";
pub const OLD: &str =
    "ticker,name,format,file,keywords\nbtc,Bitcoin,svg,logos/svg/btc.svg,\"btc, Bitcoin\"\n";
static NEXT: AtomicU64 = AtomicU64::new(0);

pub struct Scratch(pub PathBuf);
impl Scratch {
    pub fn new() -> Self {
        loop {
            let path = std::env::temp_dir().join(format!(
                "numera-baseline-units-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("create isolated scratch: {e}"),
            }
        }
    }
    pub fn join(&self, path: &str) -> PathBuf {
        self.0.join(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).expect("remove owned scratch");
    }
}
pub fn config(root: &Path, values: &[(&str, &str)]) -> Config {
    let mut env = BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]);
    env.extend(values.iter().map(|(k, v)| ((*k).into(), (*v).into())));
    Config::load_with_environment(root.into(), &env).unwrap()
}
pub fn file(path: &Path, bytes: &[u8]) -> PathBuf {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
    path.into()
}
pub fn catalog(data: &Path, project: &Path) -> Catalog {
    Catalog::open(&data.join("catalog.db"), UTC, project).unwrap()
}
pub fn add(cat: &mut Catalog, key: &str, path: &Path) {
    assert!(
        cat.insert(
            &json!({"content_key":key,"fmt":"static","file_path":path}),
            UTC
        )
        .unwrap()
    );
}
pub fn raw_paths(data: &Path) -> BTreeMap<String, String> {
    let db = rusqlite::Connection::open(data.join("catalog.db")).unwrap();
    db.prepare("SELECT content_key,file_path FROM items")
        .unwrap()
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap()
}
pub struct Legacy {
    pub scratch: Scratch,
    pub data: PathBuf,
    pub inside: PathBuf,
    pub project_file: PathBuf,
}
impl Legacy {
    pub fn new() -> Self {
        let scratch = Scratch::new();
        let data = scratch.join("collection");
        let inside = file(&data.join("media/a.webp"), b"a");
        let project_file = file(&scratch.join("tests/__media_paths_rel__/p.webp"), b"p");
        drop(catalog(&data, &scratch.0));
        let db = rusqlite::Connection::open(data.join("catalog.db")).unwrap();
        db.execute("DELETE FROM meta WHERE key='media_paths'", [])
            .unwrap();
        for (key, stored) in [
            ("s:a", inside.to_str().unwrap()),
            ("s:p", "tests/__media_paths_rel__/p.webp"),
        ] {
            db.execute("INSERT INTO items(content_key,format,file_path,created_utc,position) VALUES(?,'static',?,'2026-01-01',1)",rusqlite::params![key,stored]).unwrap();
        }
        Self {
            scratch,
            data,
            inside,
            project_file,
        }
    }
}
pub fn csv_rows(path: &Path) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut reader = csv::Reader::from_path(path).unwrap();
    let headers = reader.headers().unwrap().clone();
    reader
        .records()
        .map(|row| {
            let row = row.unwrap();
            let values = headers
                .iter()
                .zip(row.iter())
                .map(|(k, v)| (k.into(), v.into()))
                .collect::<BTreeMap<String, String>>();
            (values["ticker"].clone(), values)
        })
        .collect()
}
pub fn similarity() -> Value {
    serde_json::from_str(include_str!("../../../tests/fixtures/similarity.json")).unwrap()
}
pub fn assert_row(text: &str, header: &str) {
    assert!(
        text.contains(header),
        "a writer's column headers changed: {text}"
    );
    assert!(text.contains(ESCAPED), "escaped keyword missing: {text}");
    assert!(
        !text.contains("left|right"),
        "a raw | breaks the table: {text}"
    );
}

mod production_link;
pub use production_link::ProductionLink;
