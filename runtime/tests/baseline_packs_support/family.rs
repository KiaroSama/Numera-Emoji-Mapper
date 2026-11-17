use super::{Root, UTC, http, project, read};
use numera_runtime::{atomic, catalog::Catalog};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::{Arc, Mutex},
};
pub const BASE: &str = "cryptoemoji";
pub fn name(n: u64) -> String {
    format!("cryptoemoji{n}_by_YourEmojiBot")
}
#[derive(Default)]
pub struct Model {
    pub sets: BTreeMap<String, Vec<Value>>,
    pub bodies: BTreeMap<String, Vec<u8>>,
    pub writes: Vec<(String, String)>,
    pub unreadable: BTreeSet<String>,
    pub flood_on: BTreeSet<String>,
    pub n: u64,
}
impl Model {
    fn fresh(&mut self, bytes: Vec<u8>) -> Value {
        self.n += 1;
        let fid = format!("FID-new-{}", self.n);
        self.bodies.insert(fid.clone(), bytes);
        json!({"custom_emoji_id":format!("cid-new-{}",self.n),"file_id":fid,"file_unique_id":format!("FU-new-{}",self.n)})
    }
    fn answer(&mut self, q: &http::Request) -> http::Response {
        let op = match q.method.as_str() {
            "addStickerToSet" => Some("add"),
            "deleteStickerFromSet" => Some("delete"),
            _ => None,
        };
        if let Some(op) = op
            && self.flood_on.remove(op)
        {
            return http::payload(
                json!({"ok":false,"description":"Too Many Requests: retry after 900","parameters":{"retry_after":900}}),
            );
        }
        match q.method.as_str() {
            "getMe" => http::ok(json!({"username":"YourEmojiBot","id":123})),
            "getStickerSet" => {
                let name = &q.form["name"];
                if self.unreadable.contains(name) {
                    return http::payload(
                        json!({"ok":false,"description":"test live set unreadable"}),
                    );
                }
                match self.sets.get(name) {
                    Some(s) => http::ok(json!({"name":name,"stickers":s})),
                    None => http::payload(json!({"ok":false,"description":"STICKERSET_INVALID"})),
                }
            }
            "createNewStickerSet" | "addStickerToSet" => {
                let name = &q.form["name"];
                let bytes = q
                    .files
                    .values()
                    .next()
                    .expect("real multipart upload")
                    .clone();
                let sticker = self.fresh(bytes);
                if q.method == "createNewStickerSet" {
                    assert!(!self.sets.contains_key(name));
                    self.sets.insert(name.clone(), vec![sticker]);
                    self.writes.push(("create".into(), name.clone()));
                } else {
                    self.sets.get_mut(name).unwrap().push(sticker);
                    self.writes.push(("add".into(), name.clone()));
                }
                http::ok(json!(true))
            }
            "deleteStickerFromSet" => {
                let fid = &q.form["sticker"];
                let before = self.sets.values().map(Vec::len).sum::<usize>();
                for v in self.sets.values_mut() {
                    v.retain(|s| s["file_id"] != *fid);
                }
                assert_eq!(self.sets.values().map(Vec::len).sum::<usize>() + 1, before);
                self.writes.push(("delete".into(), fid.clone()));
                http::ok(json!(true))
            }
            "setStickerPositionInSet" => {
                let fid = &q.form["sticker"];
                let mut found = false;
                for v in self.sets.values_mut() {
                    if let Some(i) = v.iter().position(|s| s["file_id"] == *fid) {
                        let st = v.remove(i);
                        v.insert(q.form["position"].parse().unwrap(), st);
                        found = true;
                        break;
                    }
                }
                assert!(found);
                self.writes.push(("move".into(), fid.clone()));
                http::ok(json!(true))
            }
            "getFile" => {
                let fid = &q.form["file_id"];
                assert!(self.bodies.contains_key(fid));
                http::ok(json!({"file_path":fid}))
            }
            m if m.starts_with("GET ") => {
                http::Response(self.bodies[m.strip_prefix("GET ").unwrap()].clone())
            }
            "sendMessage" => http::ok(json!({"message_id":1})),
            m => panic!("unexpected plan wire method {m}: {:?}", q.form),
        }
    }
}
pub struct Family {
    pub root: Root,
    pub data: std::path::PathBuf,
    pub keys: Vec<String>,
    pub by_pack: BTreeMap<u64, Vec<String>>,
    pub extra: Vec<String>,
    pub model: Arc<Mutex<Model>>,
    pub bot: http::Bot,
}
impl Family {
    pub fn new(packs: &[(u64, usize)], extra: usize, fmt: &str) -> Self {
        let root = Root::new();
        let data = root.0.join("collection");
        std::fs::create_dir_all(&data).unwrap();
        let mut cat = Catalog::open(&data.join("catalog.db"), UTC, &root.0).unwrap();
        let mut keys = vec![];
        let mut by_pack = BTreeMap::new();
        let mut model = Model::default();
        let mut sets = vec![];
        let mut seed = 0;
        for &(n, count) in packs {
            let mut stickers = vec![
                json!({"custom_emoji_id":format!("logo-{n}"),"file_id":format!("FID-logo-{n}"),"file_unique_id":format!("FU-logo-{n}")}),
            ];
            let mut packkeys = vec![];
            for _ in 0..count {
                seed += 1;
                let (key, bytes) = insert(&root, &mut cat, seed);
                let i = stickers.len();
                let st = json!({"custom_emoji_id":format!("cid-{n}-{i}"),"file_id":format!("FID-{n}-{i}"),"file_unique_id":format!("FU-{n}-{i}")});
                cat.uploaded(&json!({"content_key":key,"custom_emoji_id":st["custom_emoji_id"],"base":BASE,"set_name":name(n)}),UTC).unwrap();
                cat.record_seen(st["file_unique_id"].as_str().unwrap(), &key)
                    .unwrap();
                model
                    .bodies
                    .insert(st["file_id"].as_str().unwrap().into(), bytes);
                stickers.push(st);
                keys.push(key.clone());
                packkeys.push(key);
            }
            sets.push(json!({"fmt":fmt,"index":n,"name":name(n),"title":format!("Test Pack {n}"),"live":stickers.len(),"logo":true,"keys":packkeys}));
            model.sets.insert(name(n), stickers);
            by_pack.insert(n, packkeys);
        }
        let mut extras = vec![];
        for _ in 0..extra {
            seed += 1;
            let (key, _) = insert(&root, &mut cat, seed);
            keys.push(key.clone());
            extras.push(key);
        }
        cat.order(&keys).unwrap();
        drop(cat);
        atomic::write_json(&data.join(format!("publish_{BASE}.json")),&json!({"base":BASE,"sets":sets,"sent":packs.iter().map(|(n,_)|name(*n)).collect::<Vec<_>>(),"sent_full":[],"skipped":[]})).unwrap();
        let model = Arc::new(Mutex::new(model));
        let shared = model.clone();
        let bot = http::Bot::new(move |q| shared.lock().unwrap().answer(q));
        Self {
            root,
            data,
            keys,
            by_pack,
            extra: extras,
            model,
            bot,
        }
    }
    pub fn current(&self) -> BTreeMap<String, u64> {
        self.by_pack
            .iter()
            .flat_map(|(n, keys)| keys.iter().map(move |k| (k.clone(), *n)))
            .collect()
    }
    pub fn plan(&self, targets: BTreeMap<String, u64>, excluded: &[String], cap: u64, over: Value) {
        let packs = targets
            .values()
            .copied()
            .chain(self.by_pack.keys().copied())
            .collect::<BTreeSet<_>>();
        let logos = packs
            .iter()
            .map(|n| (n.to_string(), json!(1)))
            .collect::<serde_json::Map<_, _>>();
        atomic::write_json(&self.data.join("pack_plan.json"),&json!({"version":1,"per_set":cap,"targets":targets.into_iter().collect::<Vec<_>>(),"excluded":excluded,"held":[],"moves":[],"counts":{},"over_capacity":over,"logo_slots":logos})).unwrap();
    }
    pub fn state(&self) -> Value {
        read(&self.data.join(format!("publish_{BASE}.json")))
    }
    pub fn journal(&self) -> Value {
        read(&self.data.join(format!("plan_apply_{BASE}.json")))
    }
    pub fn cat(&self) -> Catalog {
        Catalog::open(&self.data.join("catalog.db"), UTC, &self.root.0).unwrap()
    }
    pub fn steps(&self) -> Value {
        let (items, ids) =
            numera_runtime::catalog_read::plan_rows(&self.data.join("catalog.db"), BASE).unwrap();
        numera_runtime::plan_steps::compute(&json!({"plan":read(&self.data.join("pack_plan.json")),"state":self.state(),"items":items,"ids":ids})).unwrap()
    }
    pub fn invariant(&self) {
        let state = self.state();
        let mut seen = BTreeSet::new();
        let mut cat = self.cat();
        let m = self.model.lock().unwrap();
        for rec in state["sets"].as_array().unwrap() {
            let s = &m.sets[rec["name"].as_str().unwrap()];
            assert_eq!(rec["live"], s.len());
            let ks = rec["keys"].as_array().unwrap();
            assert_eq!(s.len(), ks.len() + 1);
            for (k, st) in ks.iter().zip(&s[1..]) {
                let k = k.as_str().unwrap();
                assert_eq!(
                    cat.dispatch(json!({"operation":"custom_id","base":BASE,"content_key":k}))
                        .unwrap(),
                    st["custom_emoji_id"]
                );
                assert!(seen.insert(k.to_owned()), "one key live twice");
            }
        }
    }
    pub fn config(&self, logo: bool) -> numera_runtime::config::Config {
        self.root.config(&[
            ("COLLECTION_PACK_BASE", BASE),
            ("PACK_OWNER_USER_ID", "111111111"),
            ("GENERAL_BOT_TOKEN", http::TOKEN),
            ("BRAND_LOGO_BOTS", if logo { "youremojibot" } else { "" }),
            ("BRAND_LOGO_PATH", "collection/brand-logo.png"),
        ])
    }
    pub fn args(&self, apply: bool) -> Vec<String> {
        let mut args = vec![
            "--base".into(),
            BASE.into(),
            "--data-dir".into(),
            self.data.to_str().unwrap().into(),
        ];
        if apply {
            args.push("--apply".into());
        }
        args
    }
    pub fn kinds(&self) -> Vec<String> {
        self.model
            .lock()
            .unwrap()
            .writes
            .iter()
            .map(|(k, _)| k.clone())
            .collect()
    }
    pub fn snapshot(&self) -> BTreeMap<std::path::PathBuf, Vec<u8>> {
        fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<std::path::PathBuf, Vec<u8>>) {
            for e in std::fs::read_dir(dir).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    walk(root, &p, out);
                } else {
                    let n = p.file_name().unwrap().to_str().unwrap();
                    if !n.ends_with("-wal") && !n.ends_with("-shm") {
                        out.insert(
                            p.strip_prefix(root).unwrap().to_owned(),
                            std::fs::read(p).unwrap(),
                        );
                    }
                }
            }
        }
        let mut m = BTreeMap::new();
        walk(&self.data, &self.data, &mut m);
        m
    }
}
fn insert(root: &Root, cat: &mut Catalog, seed: usize) -> (String, Vec<u8>) {
    let bytes = std::fs::read(project().join(format!(
        "runtime/tests/baseline_packs_support/fixtures/family-{seed}.png"
    )))
    .unwrap();
    let path = root.write(&format!("collection/media/static/item{seed}.png"), &bytes);
    let key = numera_runtime::codec_image::fingerprint_typed(&path).unwrap()["content_key"]
        .as_str()
        .unwrap()
        .to_owned();
    assert!(cat.insert(&json!({"content_key":key,"fmt":"static","file_path":path,"emojis":["😀"],"keywords":[format!("item{seed}")]}),UTC).unwrap());
    (key, bytes)
}
impl Family {
    pub fn cli_dry(&self) -> (u8, String) {
        self.root.write(
            "assets/panel.html",
            b"<!doctype html><title>Owned CLI fixture</title>",
        );
        let executable = self.root.0.join(if cfg!(windows) {
            "numera-emoji.exe"
        } else {
            "numera-emoji"
        });
        if !executable.exists() {
            std::fs::copy(super::binary(), &executable).unwrap();
        }
        let mut command = std::process::Command::new(&executable);
        command.env_clear();
        for key in [
            "PATH",
            "SystemRoot",
            "WINDIR",
            "TEMP",
            "TMP",
            "TMPDIR",
            "LANG",
        ] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        command
            .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
            .env("GENERAL_BOT_TOKEN", http::TOKEN)
            .env("NUMERA_TEST_API_BASE", &self.bot.base)
            .arg("plan-apply")
            .args(self.args(false))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        struct Child(std::process::Child);
        impl Drop for Child {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let mut child = Child(command.spawn().unwrap());
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
        let status = loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                break status;
            }
            assert!(std::time::Instant::now() < deadline, "dry CLI exceeded30s");
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        use std::io::Read;
        let mut stdout = String::new();
        let mut stderr = String::new();
        child
            .0
            .stdout
            .take()
            .unwrap()
            .read_to_string(&mut stdout)
            .unwrap();
        child
            .0
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut stderr)
            .unwrap();
        assert!(
            matches!(status.code(), Some(0 | 3)),
            "unexpected dry CLI status {status}: {stderr}"
        );
        (status.code().unwrap().try_into().unwrap(), stdout)
    }
}
