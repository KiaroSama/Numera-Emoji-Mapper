use crate::baseline_catalog_support as catalog_support;
pub use catalog_support::{Scratch, UTC, read_json};
use numera_runtime as production;
use production::{config::Config, media_adapter::MediaCodec, migration_apply, migration_restore};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Fake {
    pub names: BTreeMap<String, (String, Option<u64>)>,
    pub bytes: BTreeMap<Vec<u8>, (String, Option<u64>)>,
}
impl Fake {
    pub fn new(data: &Path, values: &[(&str, &str, Option<u64>)]) -> Self {
        let mut out = Self::default();
        for (name, key, hash) in values {
            let value = (key.to_string(), *hash);
            out.names.insert(name.to_string(), value.clone());
            if let Ok(bytes) = std::fs::read(data.join(name)) {
                out.bytes.insert(bytes, value);
            }
        }
        out
    }
    pub fn save(&self, path: &Path) {
        // JSON object keys cannot be arbitrary byte arrays; use entry pairs for worker transport.
        production::atomic::write_json(
            path,
            &json!({"names":self.names,"bytes":self.bytes.iter().collect::<Vec<_>>()}),
        )
        .unwrap();
    }
    pub fn load(path: &Path) -> Self {
        let doc = read_json(path);
        Self {
            names: serde_json::from_value(doc["names"].clone()).unwrap(),
            bytes: serde_json::from_value::<Vec<(Vec<u8>, (String, Option<u64>))>>(
                doc["bytes"].clone(),
            )
            .unwrap()
            .into_iter()
            .collect(),
        }
    }
}
impl MediaCodec for Fake {
    fn with_scratch(&self, _scratch: PathBuf) -> Self {
        self.clone()
    }
    async fn call(&self, request: Value) -> Result<Value, String> {
        if request["operation"] != "fingerprint" {
            return Err(format!("unexpected codec request: {request}"));
        }
        let source = Path::new(request["source"].as_str().ok_or("source missing")?);
        let name = source.file_name().unwrap().to_str().unwrap();
        let bytes = std::fs::read(source).map_err(|e| e.to_string())?;
        let (key, hash) = self
            .names
            .get(name)
            .or_else(|| self.bytes.get(&bytes))
            .ok_or_else(|| format!("undecodable: {name}"))?;
        Ok(json!({"content_key":key,"phash":hash}))
    }
}
pub fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}
pub struct Fixture {
    pub root: Scratch,
    pub data: PathBuf,
    pub config: Config,
}
impl Fixture {
    pub fn new() -> Self {
        Self::at("data", false)
    }
    pub fn real() -> Self {
        Self::at("collection", true)
    }
    fn at(name: &str, real: bool) -> Self {
        let root = Scratch::new();
        let data = root.0.join(name);
        std::fs::create_dir(&data).unwrap();
        if real {
            drop(
                production::catalog::Catalog::open(&data.join("catalog.db"), UTC, &root.0).unwrap(),
            );
        } else {
            drop(schema(&data.join("catalog.db"), true));
        }
        let config = Config::load_with_environment(
            root.0.clone(),
            &BTreeMap::from([
                ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
                ("BRAND_LOGO_BOTS".into(), "".into()),
                ("GENERAL_BOT_TOKEN".into(), "test-only-token".into()),
                ("COLLECTION_PACK_BASE".into(), "newfamily".into()),
                ("EMOJI_ARCHIVE_DIR".into(), "archive".into()),
            ]),
        )
        .unwrap();
        Self { root, data, config }
    }
    pub fn db(&self) -> Connection {
        Connection::open(self.data.join("catalog.db")).unwrap()
    }
    pub fn media(&self, name: &str) -> PathBuf {
        let path = self.data.join(name);
        let mut bytes = b"\x1a\x45\xdf\xa3".to_vec();
        bytes.extend_from_slice(name.as_bytes());
        std::fs::write(&path, bytes).unwrap();
        path
    }
    pub fn add(
        &self,
        key: &str,
        path: &Path,
        fmt: &str,
        hash: Option<u64>,
        cid: Option<&str>,
        fuid: Option<&str>,
    ) {
        add_to(
            &self.db(),
            key,
            path.to_str().unwrap(),
            fmt,
            hash,
            cid,
            fuid,
        );
    }
    pub fn col(&self, table: &str, column: &str) -> Vec<String> {
        col(&self.data.join("catalog.db"), table, column)
    }
    pub fn state(&self, base: &str, doc: &Value) -> PathBuf {
        let path = self.data.join(format!("publish_{base}.json"));
        production::atomic::write_json(&path, doc).unwrap();
        path
    }
    pub fn plan(&self, base: &str, doc: &Value) -> PathBuf {
        let path = self.data.join(format!("publish_plan_{base}.json"));
        production::atomic::write_json(&path, doc).unwrap();
        path
    }
    pub fn fake(&self, values: &[(&str, &str, Option<u64>)]) -> Fake {
        Fake::new(&self.data, values)
    }
    pub fn apply(&self, media: &impl MediaCodec) -> Result<Value, String> {
        migration_apply::apply(&self.config, &self.data, media, &runtime(), None)
    }
    pub fn recover_apply(&self, media: &impl MediaCodec, backup: &Path) -> Result<Value, String> {
        migration_apply::apply(&self.config, &self.data, media, &runtime(), Some(backup))
    }
    pub fn restore(&self) -> Result<(), String> {
        migration_restore::restore(&self.data, &self.root.0, &self.manifest(), true)
    }
    pub fn journal(&self) -> PathBuf {
        self.data.join("identity-migration.journal.json")
    }
    pub fn backups(&self) -> Vec<PathBuf> {
        self.matching(".db", "catalog.before-video-identity-")
    }
    fn matching(&self, suffix: &str, prefix: &str) -> Vec<PathBuf> {
        std::fs::read_dir(&self.data)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_str()
                    .is_some_and(|n| n.ends_with(suffix) && n.starts_with(prefix))
            })
            .collect()
    }
    pub fn manifest(&self) -> PathBuf {
        let files = self.matching(".rollback.json", "");
        assert_eq!(files.len(), 1);
        files[0].clone()
    }
    pub fn signature(&self) -> String {
        production::migration_signature::signature(&self.data.join("catalog.db"), &[]).unwrap()
    }
    pub fn pending(&self) -> (PathBuf, PathBuf, Fake) {
        let src = self.media("007_video_aaaaaaaaaaaa.webm");
        self.add(
            "v:aaaaaaaaaaaaaaaa",
            &src,
            "video",
            None,
            Some("CID"),
            Some("FUID"),
        );
        self.state("audit", &json!({"sets":[{"keys":["v:aaaaaaaaaaaaaaaa"]}]}));
        self.plan("audit", &json!({"mixed":["v:aaaaaaaaaaaaaaaa"]}));
        let dest = src.with_file_name("007_video_bbbbbbbbbbbb.webm");
        let fake = self.fake(&[
            ("007_video_aaaaaaaaaaaa.webm", "v:bbbbbbbbbbbbbbbb", Some(7)),
            ("007_video_bbbbbbbbbbbb.webm", "v:bbbbbbbbbbbbbbbb", Some(7)),
        ]);
        (src, dest, fake)
    }
}
pub fn schema(path: &Path, wal: bool) -> Connection {
    let db = Connection::open(path).unwrap();
    if wal {
        db.pragma_update(None, "journal_mode", "WAL").unwrap();
    }
    db.execute_batch("CREATE TABLE items(content_key TEXT PRIMARY KEY,file_path TEXT,format TEXT,phash INTEGER);CREATE TABLE publications(base TEXT,content_key TEXT,set_name TEXT,custom_emoji_id TEXT);CREATE TABLE seen_files(file_unique_id TEXT PRIMARY KEY,content_key TEXT);").unwrap();
    db
}
pub fn add_to(
    db: &Connection,
    key: &str,
    path: &str,
    fmt: &str,
    hash: Option<u64>,
    cid: Option<&str>,
    fuid: Option<&str>,
) {
    db.execute(
        "INSERT INTO items VALUES(?,?,?,?)",
        params![key, path, fmt, hash.map(|h| h as i64)],
    )
    .unwrap();
    if let Some(cid) = cid {
        db.execute(
            "INSERT INTO publications VALUES('base',?,'set1',?)",
            params![key, cid],
        )
        .unwrap();
    }
    if let Some(fuid) = fuid {
        db.execute("INSERT INTO seen_files VALUES(?,?)", params![fuid, key])
            .unwrap();
    }
}
pub fn col(path: &Path, table: &str, column: &str) -> Vec<String> {
    let db = Connection::open(path).unwrap();
    let mut query = db
        .prepare(&format!("SELECT {column} FROM {table} ORDER BY {column}"))
        .unwrap();
    query
        .query_map([], |r| r.get(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}
pub fn preexisting() -> (Fixture, PathBuf, Fake) {
    let f = Fixture::new();
    let archived = f.data.join("007_video_aaaaaaaaaaaa.webm");
    std::fs::write(&archived, b"\x1a\x45\xdf\xa3one").unwrap();
    let plain = f.media("loose.webm");
    let backup = f.data.join("catalog.before-video-identity-1.db");
    let old = schema(&backup, true);
    add_to(
        &old,
        "v:aaaaaaaaaaaaaaaa",
        archived.to_str().unwrap(),
        "video",
        None,
        Some("CID-archive"),
        Some("FUID-archive"),
    );
    add_to(
        &old,
        "v:cccccccccccccccc",
        plain.to_str().unwrap(),
        "video",
        None,
        Some("CID-loose"),
        Some("FUID-loose"),
    );
    drop(old);
    let renamed = archived.with_file_name("007_video_bbbbbbbbbbbb.webm");
    std::fs::rename(archived, &renamed).unwrap();
    f.add(
        "v:bbbbbbbbbbbbbbbb",
        &renamed,
        "video",
        None,
        Some("CID-archive"),
        Some("FUID-archive"),
    );
    f.add(
        "v:dddddddddddddddd",
        &plain,
        "video",
        None,
        Some("CID-loose"),
        Some("FUID-loose"),
    );
    f.plan(
        "pk",
        &json!({"mixed":["v:aaaaaaaaaaaaaaaa","v:cccccccccccccccc"]}),
    );
    let fake = f.fake(&[
        ("007_video_bbbbbbbbbbbb.webm", "v:bbbbbbbbbbbbbbbb", Some(1)),
        ("loose.webm", "v:dddddddddddddddd", Some(2)),
    ]);
    (f, backup, fake)
}
pub fn relative_rows() -> (Fixture, Fake) {
    let f = Fixture::new();
    std::fs::write(
        f.data.join("007_video_0123456789ab.webm"),
        b"\x1a\x45\xdf\xa3moving",
    )
    .unwrap();
    f.media("keep.webm");
    let db = f.db();
    add_to(
        &db,
        "v:0123456789abcdef",
        "./007_video_0123456789ab.webm",
        "video",
        None,
        None,
        None,
    );
    add_to(&db, "v:keep", "./keep.webm", "video", None, None, None);
    drop(db);
    let fake = f.fake(&[
        ("007_video_0123456789ab.webm", "v:fedcba9876543210", None),
        ("keep.webm", "v:keep", None),
    ]);
    (f, fake)
}
pub fn paths(f: &Fixture) -> BTreeMap<String, String> {
    let db = f.db();
    let mut q = db
        .prepare("SELECT content_key,file_path FROM items")
        .unwrap();
    q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}
