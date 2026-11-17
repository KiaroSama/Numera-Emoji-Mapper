use numera_runtime as production;
use production::{catalog::Catalog, config::Config, media_adapter::MediaCodec};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

pub const UTC: &str = "2026-11-16 00:00:00 UTC";
static NEXT: AtomicU64 = AtomicU64::new(0);
pub struct Scratch(pub PathBuf);
impl Scratch {
    pub fn new() -> Self {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("logs/tmp")
            .join(format!(
                "baseline-catalog-{}-{}-{}",
                module_path!().replace("::", "_"),
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
    pub fn database(&self) -> PathBuf {
        self.0.join("data/catalog.db")
    }
    pub fn catalog(&self) -> Catalog {
        Catalog::open(&self.database(), UTC, &self.0).unwrap()
    }
    pub fn media(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        path
    }
    pub fn item(&self, key: &str, name: &str) -> Value {
        json!({"content_key":key,"fmt":"static","file_path":self.media(name,b"x")})
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        if std::thread::panicking() {
            eprintln!("retained fixture {}", self.0.display());
        } else {
            std::fs::remove_dir_all(&self.0).unwrap();
        }
    }
}
pub fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap()
}
pub fn add(cat: &mut Catalog, item: Value, threshold: i32) -> (String, bool) {
    runtime()
        .block_on(production::ingest::add_verified(
            cat,
            item,
            UTC,
            threshold,
            async |a: &Path, b: &Path, _fmt: &str| {
                Ok(Some(
                    std::fs::read(a).map_err(|e| e.to_string())?
                        == std::fs::read(b).map_err(|e| e.to_string())?,
                ))
            },
            async |_path: &Path, _key: &str| Err("unexpected content collision".into()),
        ))
        .unwrap()
}
pub fn seen(cat: &mut Catalog, fuid: &str) -> Value {
    cat.dispatch(json!({"operation":"seen","file_unique_id":fuid}))
        .unwrap()
}
pub fn upload(cat: &mut Catalog, key: &str, base: Option<&str>, cid: Option<&str>) {
    cat.uploaded(&json!({"content_key":key,"base":base,"custom_emoji_id":cid,"set_name":base.map(|b|format!("{b}1"))}),UTC).unwrap();
}
pub fn keys(cat: &Catalog) -> Vec<String> {
    cat.all(None)
        .unwrap()
        .into_iter()
        .map(|row| row["content_key"].as_str().unwrap().to_owned())
        .collect()
}
pub fn labels(row: &Value, field: &str) -> BTreeSet<String> {
    row[field]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_owned())
        .collect()
}
pub fn order_fixture() -> (Scratch, Vec<String>) {
    let root = Scratch::new();
    let mut cat = root.catalog();
    let mut out = Vec::new();
    for i in 0..5 {
        let key = format!("s:k{i:030}");
        let path = root.0.join(format!("m{i}.png"));
        // The original uses identical real 8x8 RGBA PNG files. No hash is supplied.
        image::RgbaImage::from_pixel(8, 8, image::Rgba([1, 2, 3, 255]))
            .save(&path)
            .unwrap();
        assert!(cat.insert(&json!({"content_key":key,"fmt":"static","file_path":path,"keywords":[format!("k{i}")]}),UTC).unwrap());
        out.push(key);
    }
    drop(cat);
    (root, out)
}
pub fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
pub fn atomic_fixture() -> (Scratch, PathBuf) {
    let root = Scratch::new();
    let path = root.media("state.json", br#"{"old":true}"#);
    root.media("state.json.tmp", b"pre-existing unrelated bytes");
    (root, path)
}
pub fn atomic_members(root: &Scratch) {
    assert_eq!(
        std::fs::read(root.0.join("state.json.tmp")).unwrap(),
        b"pre-existing unrelated bytes"
    );
    let members = std::fs::read_dir(&root.0)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        members,
        BTreeSet::from(["state.json".into(), "state.json.tmp".into()])
    );
}
pub fn minimal_database(path: &Path) -> Connection {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let db = Connection::open(path).unwrap();
    db.execute_batch("CREATE TABLE items(content_key TEXT PRIMARY KEY,file_path TEXT,format TEXT,phash INTEGER); CREATE TABLE publications(base TEXT,content_key TEXT,set_name TEXT,custom_emoji_id TEXT); CREATE TABLE seen_files(file_unique_id TEXT PRIMARY KEY,content_key TEXT);").unwrap();
    db
}
#[derive(Clone)]
pub struct Fingerprint {
    pub key: String,
    pub phash: u64,
}
impl MediaCodec for Fingerprint {
    fn with_scratch(&self, _scratch: PathBuf) -> Self {
        self.clone()
    }
    async fn call(&self, request: Value) -> Result<Value, String> {
        if request["operation"] != "fingerprint" {
            return Err(format!("unexpected media operation: {request}"));
        }
        Ok(json!({"content_key":self.key,"phash":self.phash}))
    }
}
pub const OLD: &str = "v:aaaaaaaaaaaaaaaa";
pub const NEW: &str = "v:bbbbbbbbbbbbbbbb";
pub struct PlanFixture {
    pub root: Scratch,
    pub data: PathBuf,
    pub path: PathBuf,
    pub before: Value,
    pub config: Config,
}
impl PlanFixture {
    pub fn new() -> Self {
        let root = Scratch::new();
        let data = root.0.join("data");
        std::fs::create_dir(&data).unwrap();
        let source = root.media(
            "data/007_video_aaaaaaaaaaaa.webm",
            b"\x1a\x45\xdf\xa3original",
        );
        let db = minimal_database(&data.join("catalog.db"));
        db.pragma_update(None, "journal_mode", "WAL").unwrap();
        db.execute(
            "INSERT INTO items VALUES(?,?,'video',0)",
            params![OLD, source.to_str().unwrap()],
        )
        .unwrap();
        db.execute(
            "INSERT INTO publications VALUES('base',?,'set1','CID')",
            [OLD],
        )
        .unwrap();
        db.execute("INSERT INTO seen_files VALUES('FUID',?)", [OLD])
            .unwrap();
        drop(db);
        let path = data.join("pack_plan.json");
        let before = json!({"version":1,"known":[OLD],"excluded":[OLD],"targets":[[OLD,2]],"moves":[{"key":OLD,"label":OLD,"from_pack":1,"to_pack":2}],"held":[{"key":OLD,"label":"v:free-text","from_pack":1}]});
        production::atomic::write_json(&path, &before).unwrap();
        let config = Config::load_with_environment(
            root.0.clone(),
            &BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]),
        )
        .unwrap();
        Self {
            root,
            data,
            path,
            before,
            config,
        }
    }
    pub fn apply(&self, key: &str, phash: u64) -> Result<Value, String> {
        production::migration_apply::apply(
            &self.config,
            &self.data,
            &Fingerprint {
                key: key.into(),
                phash,
            },
            &runtime(),
            None,
        )
    }
    pub fn manifests(&self) -> Vec<PathBuf> {
        std::fs::read_dir(&self.data)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.to_str().unwrap().ends_with(".rollback.json"))
            .collect()
    }
    pub fn row_keys(&self) -> Vec<String> {
        let db = Connection::open(self.data.join("catalog.db")).unwrap();
        let mut q = db
            .prepare("SELECT content_key FROM items ORDER BY content_key")
            .unwrap();
        q.query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }
    pub fn journal(&self) -> PathBuf {
        self.data.join("identity-migration.journal.json")
    }
}
pub fn plan_keys(plan: &Value) -> BTreeSet<String> {
    // Observe every structured reference; free-form labels deliberately do not participate.
    let mut out = BTreeSet::new();
    for field in ["known", "excluded"] {
        for key in plan[field].as_array().unwrap() {
            out.insert(key.as_str().unwrap().into());
        }
    }
    for field in ["moves", "held"] {
        for row in plan[field].as_array().unwrap() {
            out.insert(row["key"].as_str().unwrap().into());
        }
    }
    for pair in plan["targets"].as_array().unwrap() {
        out.insert(pair[0].as_str().unwrap().into());
    }
    out
}
