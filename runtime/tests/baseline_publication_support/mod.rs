use crate::baseline_catalog_support as catalog_support;
pub use catalog_support::{Scratch, UTC, read_json};
use numera_runtime as production;
pub(crate) mod server;
use image::{Rgba, RgbaImage};
use production::{
    catalog::Catalog,
    config::Config,
    media_adapter::MediaCodec,
    publication::Publisher,
    publish_collection::{self, Options},
    telegram::Telegram,
};
use serde_json::{Value, json};
pub use server::{Bot, Model, sticker};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
pub const SET: &str = "pks1_by_YourEmojiBot";
pub const SET2: &str = "pks2_by_YourEmojiBot";
pub fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}
pub fn png(path: &Path, color: [u8; 4]) -> PathBuf {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut image = RgbaImage::new(100, 100);
    for y in 20..80 {
        for x in 20..80 {
            image.put_pixel(x, y, Rgba(color));
        }
    }
    image.save(path).unwrap();
    path.into()
}
pub fn key(path: &Path) -> String {
    production::codec_image::fingerprint_typed(path).unwrap()["content_key"]
        .as_str()
        .unwrap()
        .into()
}
#[derive(Clone, Default)]
pub struct Images {
    pub forced_key: Option<String>,
    pub fail: Option<String>,
}
impl MediaCodec for Images {
    fn with_scratch(&self, _scratch: PathBuf) -> Self {
        self.clone()
    }
    async fn call(&self, request: Value) -> Result<Value, String> {
        if let Some(error) = &self.fail {
            return Err(error.clone());
        }
        let source = Path::new(request["source"].as_str().ok_or("codec source missing")?);
        match request["operation"].as_str().unwrap_or("") {
            "fingerprint" => {
                if let Some(key) = &self.forced_key {
                    Ok(json!({"content_key":key,"phash":null}))
                } else {
                    production::codec_image::fingerprint_typed(source).map_err(|e| e.message)
                }
            }
            "same_image" | "compare" => Ok(json!(production::codec_image::same_image(
                source,
                Path::new(request["other"].as_str().ok_or("other missing")?)
            ))),
            "publication_usable" => production::publication_media::usable(
                source.parent().unwrap(),
                source,
                request["format"].as_str().ok_or("format missing")?,
            )
            .await
            .map(Value::Bool),
            other => Err(format!("unexpected publication codec operation {other}")),
        }
    }
}
pub struct Fixture {
    pub root: Scratch,
    pub data: PathBuf,
    pub keys: Vec<String>,
}
impl Fixture {
    pub fn new() -> Self {
        let root = Scratch::new();
        let data = root.0.join("data");
        std::fs::create_dir(&data).unwrap();
        let mut f = Self {
            root,
            data,
            keys: Vec::new(),
        };
        let mut cat = f.catalog();
        for i in 0..2 {
            let path = png(
                &f.data.join(format!("media/static/item{i}.png")),
                [10, 60 * (i + 1), 200, 255],
            );
            let key = key(&path);
            cat.insert(&json!({"content_key":key,"fmt":"static","file_path":path,"emojis":["😀"],"keywords":[format!("item{i}")]}),UTC).unwrap();
            cat.record_seen(&format!("UP-item{i}"), &key).unwrap();
            f.keys.push(key);
        }
        drop(cat);
        f
    }
    pub fn empty() -> Self {
        let root = Scratch::new();
        let data = root.0.join("data");
        std::fs::create_dir(&data).unwrap();
        let f = Self {
            root,
            data,
            keys: Vec::new(),
        };
        drop(f.catalog());
        f
    }
    pub fn catalog(&self) -> Catalog {
        Catalog::open(&self.data.join("catalog.db"), UTC, &self.root.0).unwrap()
    }
    pub fn config(&self, extra: &[(&str, String)]) -> Config {
        let mut values = BTreeMap::from([
            ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
            ("BRAND_LOGO_BOTS".into(), "".into()),
            ("GENERAL_BOT_TOKEN".into(), "test-only-token".into()),
            ("PACK_OWNER_USER_ID".into(), "7".into()),
            ("PACK_LINKS_CHAT_ID".into(), "".into()),
        ]);
        for (k, v) in extra {
            values.insert(k.to_string(), v.clone());
        }
        Config::load_with_environment(self.root.0.clone(), &values).unwrap()
    }
    pub fn state_path(&self) -> PathBuf {
        self.data.join("publish_pk.json")
    }
    pub fn plan_path(&self) -> PathBuf {
        self.data.join("publish_plan_pk.json")
    }
    pub fn state_set(&self, keys: Option<&[String]>, live: u64, logo: bool) -> Value {
        json!({"fmt":"static","index":1,"name":SET,"title":"Pack 1","live":live,"logo":logo,"keys":keys.unwrap_or(&self.keys)})
    }
    pub fn state(&self) -> Value {
        json!({"base":"pk","sets":[],"sent":[],"sent_full":[],"skipped":[]})
    }
    pub fn write_state(&self, sets: Vec<Value>, top: Value) {
        let mut state = self.state();
        state["sets"] = json!(sets);
        for (k, v) in top.as_object().unwrap() {
            state[k] = v.clone();
        }
        production::atomic::write_json(&self.state_path(), &state).unwrap();
    }
    pub fn read_back(&self, keys: &[String]) {
        let mut cat = self.catalog();
        for (i, key) in keys.iter().enumerate() {
            cat.uploaded(&json!({"content_key":key,"custom_emoji_id":format!("c{i}"),"base":"pk","set_name":SET}),UTC).unwrap();
        }
    }
    pub fn add(&mut self, i: u8) -> String {
        let path = png(
            &self.data.join(format!("media/static/item{i}.png")),
            [10, 30 * (i + 1), 90, 255],
        );
        let key = key(&path);
        let mut cat = self.catalog();
        cat.insert(&json!({"content_key":key,"fmt":"static","file_path":path,"emojis":["😀"],"keywords":[format!("item{i}")]}),UTC).unwrap();
        cat.record_seen(&format!("UP-item{i}"), &key).unwrap();
        self.keys.push(key.clone());
        key
    }
    pub fn options(&self) -> Options {
        Options {
            base: "pk".into(),
            title: "Pack".into(),
            bot: "YourEmojiBot".into(),
            user_id: 7,
            fmt: "static".into(),
            per_set: 200,
            new_set: false,
            into_pack: None,
            repaint: false,
            default_emoji: "😀".into(),
            logo: None,
        }
    }
    pub fn publish(
        &self,
        bot: &Bot,
        media: &Images,
        state: &mut Value,
        options: &Options,
    ) -> Result<Value, String> {
        let mut cat = self.catalog();
        let runtime = runtime();
        let config = self.config(&[]);
        let tg = bot.telegram();
        let mut publisher = Publisher {
            tg: &tg,
            cat: &mut cat,
            media,
            data: self.data.clone(),
            runtime: &runtime,
            config: Some(&config),
        };
        publish_collection::publish(&mut publisher, options, &self.keys, state)
    }
    pub fn reconcile(&self, bot: &Bot, media: &Images, set: &mut Value) -> Result<(), String> {
        let mut cat = self.catalog();
        let runtime = runtime();
        let tg = bot.telegram();
        let mut publisher = Publisher {
            tg: &tg,
            cat: &mut cat,
            media,
            data: self.data.clone(),
            runtime: &runtime,
            config: None,
        };
        publisher.reconcile(set, "pk")
    }
    pub fn cid(&self, key: &str) -> Value {
        self.catalog()
            .dispatch(json!({"operation":"custom_id","base":"pk","content_key":key}))
            .unwrap()
    }
    pub fn published(&self, key: &str) -> bool {
        self.catalog()
            .dispatch(json!({"operation":"published","base":"pk","content_key":key}))
            .unwrap()
            == true
    }
    pub fn seen(&self, fuid: &str) -> Value {
        self.catalog()
            .dispatch(json!({"operation":"seen","file_unique_id":fuid}))
            .unwrap()
    }
}
pub fn telegram(base: &str) -> Telegram {
    Telegram::new("test-only-token".into(), base.into()).unwrap()
}

pub fn seed(f: &mut Fixture, n: u8, prefix: &str) {
    let mut cat = f.catalog();
    for i in 0..n {
        let path = png(
            &f.data.join(format!("media/static/item{i}.png")),
            [10, ((u16::from(i) + 1) * 40 % 255) as u8, 200, 255],
        );
        let key = key(&path);
        cat.insert(&json!({"content_key":key,"fmt":"static","file_path":path,"emojis":["😀"],"keywords":[format!("item{i}")]}),UTC).unwrap();
        cat.record_seen(&format!("{prefix}item{i}"), &key).unwrap();
        f.keys.push(key);
    }
}
