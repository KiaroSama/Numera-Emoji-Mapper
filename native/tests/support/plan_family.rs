//! Original opaque-cell Family recipe and actual copied-command ownership, shared across scenarios.
use super::{bot_api::TOKEN, tooling_fixture::Root};
use _native::{catalog::Catalog, logging::RunLog};
use pyo3::prelude::*;
use pyo3::types::PyDict;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[path = "retained_cli.rs"]
mod retained_cli;

pub const UTC: &str = "2026-10-07 00:00:00 UTC";
pub fn name(index: usize) -> String {
    format!("cryptoemoji{index}_by_YourEmojiBot")
}
pub fn read(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
fn image(path: &Path, seed: u64) {
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        let random = py.import("random")?.getattr("Random")?.call1((seed,))?;
        let image = py
            .import("PIL.Image")?
            .call_method1("new", ("RGBA", (100, 100), (0, 0, 0, 0)))?;
        let result = (|| -> PyResult<()> {
            for gx in 0..10 {
                for gy in 0..10 {
                    if random.call_method0("random")?.extract::<f64>()? < 0.6 {
                        let channel = || random.call_method1("randrange", (256,))?.extract::<u8>();
                        let color = (channel()?, channel()?, channel()?, 255);
                        image.call_method1(
                            "paste",
                            (color, (gx * 10, gy * 10, gx * 10 + 10, gy * 10 + 10)),
                        )?;
                    }
                }
            }
            image.call_method1("save", (path.to_path_buf(), "PNG"))?;
            Ok(())
        })();
        let closed = image.call_method0("close").map(|_| ());
        result.and(closed)
    })
    .unwrap();
}

pub struct Family {
    pub data: PathBuf,
    pub state: Value,
    pub live: Vec<Vec<Value>>,
    pub keys: Vec<String>,
    pub rows: Vec<Value>,
    pub initial: Vec<(usize, usize)>,
    pub plan_bytes: Vec<u8>,
}
impl Family {
    pub fn create(
        root: &Path,
        label: &str,
        images: &[PathBuf],
        counts: [usize; 2],
        extra: usize,
    ) -> Self {
        let data = root.join(label);
        let total = counts.iter().sum::<usize>() + extra;
        let keys = images[..total]
            .iter()
            .map(|path| {
                _native::codec_static_identity::fingerprint(path).unwrap()["content_key"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect::<Vec<_>>();
        assert_eq!(keys.iter().collect::<BTreeSet<_>>().len(), total);
        let mut cat = Catalog::open(&data.join("catalog.db"), UTC, root).unwrap();
        for (index, key) in keys.iter().enumerate() {
            assert!(
                cat.insert(
                    &json!({"content_key":key,"fmt":"static","file_path":images[index],
                "emojis":["😀"],"keywords":[format!("item{}",index+1)]}),
                    UTC
                )
                .unwrap()
            );
        }
        cat.order(&keys).unwrap();
        let mut live = Vec::new();
        let mut sets = Vec::new();
        let mut initial = Vec::new();
        let mut index = 0;
        for (pack, count) in counts.into_iter().enumerate() {
            let mut members = vec![json!({"custom_emoji_id":format!("90000000{}",pack+1),
                "file_id":format!("logo-{}",pack+1),"file_unique_id":format!("logo-unique-{}",pack+1)})];
            let start = index;
            for _ in 0..count {
                let sticker = json!({"custom_emoji_id":format!("111000{index:03}"),
                    "file_id":format!("old-{index}"),"file_unique_id":format!("old-unique-{index}")});
                cat.uploaded(
                    &json!({"content_key":keys[index],"custom_emoji_id":sticker["custom_emoji_id"],
                    "base":"cryptoemoji","set_name":name(pack+1)}),
                    UTC,
                )
                .unwrap();
                cat.record_seen(sticker["file_unique_id"].as_str().unwrap(), &keys[index])
                    .unwrap();
                members.push(sticker);
                initial.push((index, pack + 1));
                index += 1;
            }
            sets.push(json!({"fmt":"mixed","index":pack+1,"name":name(pack+1),
                "title":format!("Test Pack {}",pack+1),"live":members.len(),"logo":true,"keys":&keys[start..index]}));
            live.push(members);
        }
        let state = json!({"base":"cryptoemoji","sets":sets,"sent":[name(1),name(2)],"sent_full":[],"skipped":[]});
        _native::atomic::write_json(&data.join("publish_cryptoemoji.json"), &state).unwrap();
        let rows = cat.all(None).unwrap();
        drop(cat);
        Self {
            data,
            state,
            live,
            keys,
            rows,
            initial,
            plan_bytes: Vec::new(),
        }
    }
    pub fn plan(&mut self, targets: Value) {
        let path = self.data.join("pack_plan.json");
        _native::atomic::write_json(
            &path,
            &json!({"version":1,"per_set":200,"targets":targets,
            "known":self.keys,"excluded":[],"held":[],"moves":[],"over_capacity":{},
            "logo_slots":{"1":1,"2":1,"3":1}}),
        )
        .unwrap();
        self.plan_bytes = std::fs::read(path).unwrap();
    }
    pub fn saved(&self) -> Value {
        read(&self.data.join("publish_cryptoemoji.json"))
    }
    pub fn journal(&self) -> Value {
        read(&self.data.join("plan_apply_cryptoemoji.json"))
    }
    pub fn check(&self, root: &Path, wanted: &[Vec<usize>], cids: &BTreeMap<usize, String>) {
        let saved = self.saved();
        assert_eq!(saved["sets"].as_array().unwrap().len(), wanted.len());
        let mut seen = BTreeSet::new();
        let mut cat = Catalog::open(&self.data.join("catalog.db"), UTC, root).unwrap();
        let published_sets = cat.dispatch(json!({"operation":"published_sets"})).unwrap();
        for (pack, indices) in wanted.iter().enumerate() {
            let record = &saved["sets"][pack];
            assert_eq!(record["index"], pack + 1);
            assert_eq!(record["name"], name(pack + 1));
            assert_eq!(record["logo"], true);
            assert_eq!(record["live"], indices.len() + 1);
            assert_eq!(
                record["keys"],
                json!(
                    indices
                        .iter()
                        .map(|index| &self.keys[*index])
                        .collect::<Vec<_>>()
                )
            );
            for index in indices {
                assert!(seen.insert(*index), "one art is recorded in two packs");
                assert_eq!(published_sets[&self.keys[*index]], name(pack + 1));
                assert_eq!(cat.dispatch(json!({"operation":"custom_id","base":"cryptoemoji","content_key":self.keys[*index]})).unwrap(), cids[index]);
            }
        }
        for (index, key) in self.keys.iter().enumerate() {
            let mut row = self.rows[index].clone();
            if let Some(cid) = cids.get(&index) {
                row["custom_emoji_id"] = json!(cid);
                row["uploaded"] = json!(true);
            }
            assert_eq!(cat.get(key).unwrap(), Some(row));
            assert_eq!(
                cat.dispatch(
                    json!({"operation":"published","base":"cryptoemoji","content_key":key})
                )
                .unwrap(),
                seen.contains(&index)
            );
        }
        drop(cat);
        assert!(saved.get("in_flight").is_none_or(Value::is_null));
        assert_eq!(
            std::fs::read(self.data.join("pack_plan.json")).unwrap(),
            self.plan_bytes
        );
    }
}

pub struct Fixture {
    pub root: Root,
    pub images: Vec<PathBuf>,
    pub bytes: Vec<Vec<u8>>,
    pub logo: PathBuf,
    pub logo_bytes: Vec<u8>,
    logo_source_bytes: Vec<u8>,
    executable: PathBuf,
    environment: BTreeMap<String, String>,
    runtime: tokio::runtime::Runtime,
    log: RunLog,
}
impl Fixture {
    pub fn new() -> Self {
        let root = Root::new("bounded original publication café");
        let project = _native::tooling_environment::physical_root(
            Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap(),
        )
        .unwrap();
        let (executable, mut environment) = retained_cli::install(&project, &root.0);
        let mut images = Vec::new();
        let mut bytes = Vec::new();
        for index in 0..17 {
            let path = root.0.join(format!("item{}.png", index + 1));
            image(&path, index + 1);
            bytes.push(std::fs::read(&path).unwrap());
            images.push(path);
        }
        let logo = root.0.join("brand-logo.png");
        image(&logo, 999);
        let logo_source_bytes = std::fs::read(&logo).unwrap();
        // The original seed covers the whole 100x100 canvas: PNG encoding, not a target conversion, supplies expected bytes.
        let encoded = root.0.join("expected-logo.png");
        Python::attach(|py| -> PyResult<()> {
            let image = py
                .import("PIL.Image")?
                .call_method1("open", (logo.clone(),))?;
            let result = (|| -> PyResult<()> {
                assert_eq!(image.getattr("size")?.extract::<(u32, u32)>()?, (100, 100));
                assert_eq!(
                    image
                        .call_method0("getbbox")?
                        .extract::<(u32, u32, u32, u32)>()?,
                    (0, 0, 100, 100)
                );
                let options = PyDict::new(py);
                options.set_item("optimize", true)?;
                image.call_method("save", (encoded.clone(), "PNG"), Some(&options))?;
                Ok(())
            })();
            let closed = image.call_method0("close").map(|_| ());
            result.and(closed)
        })
        .unwrap();
        let logo_bytes = std::fs::read(encoded).unwrap();
        environment.insert("GENERAL_BOT_TOKEN".into(), TOKEN.into());
        environment.insert("BRAND_LOGO_BOTS".into(), String::new());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let log = RunLog::open(&root.0, "bounded-plan-owner", vec![TOKEN.into()]);
        std::fs::write(root.0.join("empty-input"), b"").unwrap();
        Self {
            root,
            images,
            bytes,
            logo,
            logo_bytes,
            logo_source_bytes,
            executable,
            environment,
            runtime,
            log,
        }
    }
    pub fn command(
        &self,
        family: &Family,
        api: &str,
        logo: bool,
    ) -> _native::tooling_process::Completed {
        let mut environment = self.environment.clone();
        environment.insert("NUMERA_TEST_API_BASE".into(), api.into());
        if logo {
            environment.insert("BRAND_LOGO_BOTS".into(), "youremojibot".into());
            environment.insert("BRAND_LOGO_PATH".into(), self.logo.to_str().unwrap().into());
        }
        self.runtime
            .block_on(_native::tooling_process::capture_with_input(
                &self.root.0,
                &[
                    self.executable.as_os_str().to_owned(),
                    "plan-apply".into(),
                    "--base".into(),
                    "cryptoemoji".into(),
                    "--data-dir".into(),
                    family.data.as_os_str().to_owned(),
                    "--apply".into(),
                    "--user-id".into(),
                    "111111111".into(),
                ],
                &environment,
                &self.root.0.join("empty-input"),
                Duration::from_secs(25),
                Duration::from_secs(8),
                &self.log,
            ))
            .unwrap()
    }
    pub fn finish(self) {
        assert_eq!(std::fs::read(&self.logo).unwrap(), self.logo_source_bytes);
        for (path, bytes) in self.images.iter().zip(&self.bytes) {
            assert_eq!(std::fs::read(path).unwrap(), *bytes);
        }
        assert!(!_native::process_owner::cleanup_pending(&self.root.0));
        self.runtime.shutdown_timeout(Duration::from_secs(1));
        self.log.finish(0);
        drop(self.log);
        self.root.clean().unwrap();
    }
}
