//! Shared finite fixtures invoke actual Rust policy, disk, image and transport effects.
#![allow(dead_code)]
pub mod server;
use numera_runtime as rt;
use serde_json::{Map, Value, json};
#[allow(unused_imports)]
// Distinct original-case unit modules use different fixture components.
pub use server::{Body, Reply, Request, Root, Server, TOKEN, fault, ok, reject, reply};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
    path::{Path, PathBuf},
    time::Duration,
};
pub const SET: &str = "cryptoemoji1_by_bot";
pub const NEXT_SET: &str = "cryptoemoji2_by_bot";
pub const OLD: &str = "cid-old";
pub const NEW: &str = "cid-new";
pub const HTML: &[u8] = b"<html><body>429 Too Many Requests</body></html>";
pub fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|s| (*s).into()).collect()
}
pub fn doc(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}
pub fn write(path: &Path, value: &Value) {
    rt::atomic::write_json(path, value).unwrap();
}
pub fn png(img: image::RgbaImage) -> Vec<u8> {
    let mut out = Cursor::new(vec![]);
    img.write_to(&mut out, image::ImageFormat::Png).unwrap();
    out.into_inner()
}
pub fn solid(size: u32, color: [u8; 4]) -> Vec<u8> {
    png(image::RgbaImage::from_pixel(size, size, image::Rgba(color)))
}
pub fn ramp(reverse: bool) -> Vec<u8> {
    png(image::RgbaImage::from_fn(100, 100, |x, _| {
        let v = if reverse { (99 - x) * 2 } else { x * 2 };
        image::Rgba([v as u8, v as u8, v as u8, 255])
    }))
}
pub fn block(color: [u8; 4]) -> Vec<u8> {
    png(image::RgbaImage::from_fn(100, 100, |x, y| {
        if (20..80).contains(&x) && (20..80).contains(&y) {
            image::Rgba(color)
        } else {
            image::Rgba([0, 0, 0, 0])
        }
    }))
}
pub fn noise(name: &str) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let data = match name {
        "some-coin" => include_bytes!("fixtures/some-coin.png").as_slice(),
        "some-coin-inverted" => include_bytes!("fixtures/some-coin-inverted.png").as_slice(),
        "solana" => include_bytes!("fixtures/solana.png").as_slice(),
        "solama-the-llama" => include_bytes!("fixtures/solama-the-llama.png").as_slice(),
        "official-btc" => include_bytes!("fixtures/official-btc.png").as_slice(),
        "stranger" => include_bytes!("fixtures/stranger.png").as_slice(),
        _ => panic!("unknown frozen original art"),
    };
    let provenance: Value = serde_json::from_str(include_str!("fixtures/provenance.json")).unwrap();
    assert_eq!(provenance["Pillow"], "12.3.0");
    assert_eq!(
        provenance["baseline_revision"],
        "45a96f9de70da8429b83c047e0626b6ed6694b20"
    );
    let row = provenance["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == name)
        .unwrap();
    let digest = Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    assert_eq!(row["sha256"], digest);
    data.into()
}
pub fn config(root: &Path, values: &[(&str, &str)]) -> rt::config::Config {
    let mut env = BTreeMap::from([
        ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
        ("COIN_PACK_BASE".into(), "cryptoemoji".into()),
        ("COIN_PACK_TITLE".into(), "T".into()),
    ]);
    env.extend(values.iter().map(|(k, v)| ((*k).into(), (*v).into())));
    rt::config::Config::load_with_environment(root.into(), &env).unwrap()
}
pub fn member(n: usize) -> Value {
    json!({"custom_emoji_id":format!("c{n}"),"file_id":format!("f{n}"),"file_unique_id":format!("u{n}")})
}
pub fn sticker(cid: &str) -> Value {
    json!({"custom_emoji_id":cid,"file_id":format!("fid-{cid}"),"file_unique_id":format!("uid-{cid}")})
}
pub fn listing(members: &[Value]) -> Reply {
    reply("getStickerSet", ok(json!({"stickers":members})))
}
pub fn list_ids(cids: &[&str]) -> Reply {
    listing(&cids.iter().map(|cid| sticker(cid)).collect::<Vec<_>>())
}
pub fn unavailable() -> Reply {
    reply("getStickerSet", reject("controlled unavailable"))
}
pub fn me() -> Reply {
    reply("getMe", ok(json!({"username":"bot"})))
}
pub fn download(script: &mut Vec<Reply>, file: &str, bytes: &[u8]) {
    script.push(reply(
        "getFile",
        ok(json!({"file_path":format!("{file}.png")})),
    ));
    let mut answer = reply(&format!("GET {file}.png"), Value::Null);
    answer.body = Body::Bytes(bytes.into());
    script.push(answer);
}
pub fn provider_reply(path: &str, status: u16, bytes: &[u8]) -> Reply {
    let mut r = reply(&format!("GET {path}"), Value::Null);
    r.status = status;
    r.body = Body::Bytes(bytes.into());
    r
}
pub fn json_reply(path: &str, status: u16, value: Value) -> Reply {
    provider_reply(path, status, value.to_string().as_bytes())
}
pub fn csv_rows(path: &Path) -> BTreeMap<String, BTreeMap<String, String>> {
    let mut r = csv::Reader::from_path(path).unwrap();
    let h = r.headers().unwrap().clone();
    r.records()
        .map(|v| {
            let v = v.unwrap();
            let row = h
                .iter()
                .zip(v.iter())
                .map(|(k, v)| (k.into(), v.into()))
                .collect::<BTreeMap<String, String>>();
            (row["ticker"].clone(), row)
        })
        .collect()
}

pub type PublishOutcome = (usize, usize, Map<String, Value>);
pub type PublishTranscript = (Provider, PublishOutcome, Vec<Request>);

pub struct Provider {
    pub root: Root,
    pub files: rt::coin_provider_recover::Files,
    pub ours: Vec<u8>,
    pub foreign: Vec<u8>,
    pub before: Vec<Value>,
}
impl Provider {
    pub fn new(count: usize) -> Self {
        let root = Root::new();
        let files = rt::coin_provider_recover::Files {
            root: root.0.clone(),
            state: root.0.join("state.json"),
            mapping: root.0.join("ticker_to_id.json"),
            published: root.0.join("emoji"),
            base: "cryptoemoji".into(),
        };
        std::fs::create_dir(&files.published).unwrap();
        let ours = ramp(false);
        let foreign = ramp(true);
        std::fs::write(files.published.join("aaa.png"), &ours).unwrap();
        write(
            &files.state,
            &json!({"sets":[{"index":1,"name":SET,"title":"T 1"}]}),
        );
        write(&files.mapping, &json!({"btc":"c-btc"}));
        let before = (1..=count).map(member).collect();
        Self {
            root,
            files,
            ours,
            foreign,
            before,
        }
    }
    pub fn stage(&self, tickers: &[&str], bytes: &[u8]) -> rt::coin_provider_stage::Stage {
        let stage = rt::coin_provider_stage::Stage::new(&self.files.published).unwrap();
        for ticker in tickers {
            std::fs::write(stage.source(ticker).unwrap(), bytes).unwrap();
        }
        stage
    }
    pub fn run(
        &self,
        server: &Server,
        stage: &mut rt::coin_provider_stage::Stage,
        tickers: &[&str],
    ) -> Result<PublishOutcome, String> {
        rt::coin_provider_effects::run(
            &self.files,
            &server.telegram(),
            stage,
            &ids(tickers),
            "T",
            1,
            &Map::new(),
        )
    }
    pub fn mutation(&self, creating: bool, before: usize, body: Body) -> Reply {
        let mut r = reply(
            if creating {
                "createNewStickerSet"
            } else {
                "addStickerToSet"
            },
            Value::Null,
        );
        r.body = body;
        r.observe = Some(self.files.state.clone());
        r.action = Some(Box::new(move |request| {
            let intent = &request.observation.as_ref().unwrap()["provider_in_flight"];
            assert_eq!(intent["expected_before"], before);
            assert_eq!(request.form["user_id"], "1");
        }));
        r
    }
    pub fn release(&self) {
        for path in [
            rt::locks::pack_family_path(&self.files.root, &self.files.base),
            self.files.root.join(".locks/canonical_map.lock"),
        ] {
            drop(rt::locks::Lease::acquire(&path, "baseline release proof").unwrap());
        }
        assert!(!rt::process_owner::cleanup_pending(&self.root.0));
    }
}

pub fn publish_case(kind: &str) -> PublishTranscript {
    let count = if kind == "rollover" {
        200
    } else if kind == "duplicate" {
        1
    } else {
        2
    };
    let f = Provider::new(count);
    let mut stage = f.stage(&["aaa"], &f.ours);
    let mut live = f.before.clone();
    let creating = kind == "rollover";
    let own = count + 1;
    let own_member = member(own);
    if creating {
        live = vec![own_member.clone()]
    } else {
        live.push(own_member.clone());
    }
    if matches!(kind, "foreign-tail" | "duplicate") {
        live.push(member(own + 1));
    }
    let mut script = vec![me(), listing(&f.before)];
    if !creating {
        script.push(listing(&f.before));
    }
    let body = if kind == "rejected" {
        Body::Json(reject("addStickerToSet failed: STICKER_PNG_NOPNG"))
    } else if kind == "ambiguous" {
        Body::Disconnect
    } else {
        Body::Json(ok(json!(true)))
    };
    script.push(f.mutation(creating, if creating { 0 } else { count }, body));
    if kind == "ambiguous" {
        script.push(listing(&live));
        download(&mut script, &format!("f{own}"), &f.ours);
    }
    if kind != "rejected" {
        script.push(listing(&live));
        download(&mut script, &format!("f{own}"), &f.ours);
        if matches!(kind, "foreign-tail" | "duplicate") {
            download(
                &mut script,
                &format!("f{}", own + 1),
                if kind == "duplicate" {
                    &f.ours
                } else {
                    &f.foreign
                },
            );
        }
    }
    let server = Server::new(script);
    let outcome = f.run(&server, &mut stage, &["aaa"]).unwrap();
    let requests = server.finish();
    f.release();
    (f, outcome, requests)
}
pub fn assert_verified_normal(kind: &str) {
    let (f, result, r) = publish_case(kind);
    assert_eq!((result.0, result.1), (1, 0));
    let adds = r
        .iter()
        .filter(|r| r.method == "addStickerToSet")
        .collect::<Vec<_>>();
    assert_eq!(adds.len(), 1);
    assert_eq!(adds[0].form["name"], SET);
    assert_eq!(
        adds[0].observation.as_ref().unwrap()["provider_in_flight"]["key"],
        "aaa"
    );
    assert_eq!(
        adds[0].observation.as_ref().unwrap()["provider_in_flight"]["expected_before"],
        2
    );
    assert_eq!(result.2["aaa"], "c3");
    assert_eq!(doc(&f.files.mapping)["aaa"], "c3");
    assert_eq!(doc(&f.files.state)["provider_in_flight"], Value::Null);
}

pub fn begin_unresolved(f: &Provider, tickers: &[&str]) -> (Value, Vec<Request>) {
    let mut stage = f.stage(tickers, &f.ours);
    let script = vec![
        me(),
        listing(&f.before),
        listing(&f.before),
        f.mutation(false, f.before.len(), Body::Disconnect),
        unavailable(),
        unavailable(),
    ];
    let server = Server::new(script);
    let result = f.run(&server, &mut stage, tickers).unwrap();
    assert_eq!((result.0, result.1), (0, tickers.len()));
    assert!(!result.2.contains_key("aaa"));
    let intent = doc(&f.files.state)["provider_in_flight"].clone();
    assert_eq!(intent["key"], "aaa");
    assert_eq!(intent["operation"], "add");
    assert_eq!(intent["set_name"], SET);
    assert_eq!(intent["expected_before"], f.before.len());
    assert!(Path::new(intent["source_path"].as_str().unwrap()).is_file());
    let r = server.finish();
    assert_eq!(
        r.iter().filter(|r| r.method == "addStickerToSet").count(),
        1
    );
    f.release();
    (intent, r)
}
pub fn resume(
    f: &Provider,
    stage: &mut rt::coin_provider_stage::Stage,
    live: &[Value],
    bytes: &[u8],
) -> (PublishOutcome, Vec<Request>) {
    let mut script = vec![me(), listing(live)];
    download(&mut script, "f3", bytes);
    let server = Server::new(script);
    let result = f.run(&server, stage, &["aaa"]).unwrap();
    let r = server.finish();
    assert!(
        r.iter()
            .all(|r| r.method != "addStickerToSet" && r.method != "createNewStickerSet")
    );
    f.release();
    (result, r)
}

pub struct CacheFixture {
    pub root: Root,
    pub members: Vec<Value>,
    pub blobs: BTreeMap<String, Vec<u8>>,
    pub fail: BTreeSet<String>,
    pub fetched: Vec<String>,
}
impl CacheFixture {
    pub fn new(cids: &[&str]) -> Self {
        Self {
            root: Root::new(),
            members: cids.iter().map(|s| sticker(s)).collect(),
            blobs: BTreeMap::from([
                ("a".into(), block([200, 20, 20, 255])),
                ("b".into(), block([20, 200, 20, 255])),
                ("c".into(), block([20, 20, 200, 255])),
            ]),
            fail: BTreeSet::new(),
            fetched: vec![],
        }
    }
    pub fn path(&self) -> PathBuf {
        self.root.0.join("cache.json")
    }
    pub fn run(&mut self, cache: &mut Value) -> Vec<String> {
        rt::coin_remap_cache::download(&[json!({"index":1,"name":"s1"})], cache, self).unwrap()
    }
}
impl rt::coin_remap_cache::CacheIo for CacheFixture {
    fn list(&mut self, name: &str) -> Result<Vec<Value>, String> {
        assert_eq!(name, "s1");
        Ok(self.members.clone())
    }
    fn signature(&mut self, sticker: &Value) -> Result<Vec<u8>, String> {
        let cid = sticker["custom_emoji_id"].as_str().unwrap();
        self.fetched.push(cid.into());
        if self.fail.contains(cid) {
            return Err("controlled unavailable image".into());
        }
        rt::codec_image::remap_signature(self.blobs.get(cid).unwrap())
    }
    fn save(&mut self, cache: &Value) -> Result<(), String> {
        rt::atomic::write_json(&self.path(), cache).map_err(|e| e.to_string())
    }
    fn wait(&mut self, _: Duration) {}
}
pub struct Remap {
    pub root: Root,
    pub files: rt::coin_remap::Files,
}
impl Remap {
    pub fn new() -> Self {
        let root = Root::new();
        let files = rt::coin_remap::Files {
            root: root.0.clone(),
            state: root.0.join("state.json"),
            cache: root.0.join("cache.json"),
            images: root.0.join("emoji"),
            output: root.0.join("ticker_to_id.json"),
            candidates: root.0.join("ticker_to_id.candidate.json"),
            base: "cryptoemoji".into(),
        };
        std::fs::create_dir(&files.images).unwrap();
        write(&files.state, &json!({"sets":[{"index":1,"name":"s1"}]}));
        Self { root, files }
    }
    pub fn local(&self, name: &str, bytes: &[u8]) {
        std::fs::write(self.files.images.join(format!("{name}.png")), bytes).unwrap();
    }
    pub fn run(&self, server: &Server, max: Option<f64>, apply: bool) -> Result<u8, String> {
        rt::coin_remap::run(
            &self.files,
            &server.telegram(),
            rt::coin_remap_policy::Options {
                apply,
                max_distance: max,
                min_margin: None,
                incomplete_sets: 0,
                failed_images: 0,
            },
            &self.root.log(),
        )
    }
}
pub fn remap_script(
    cids: &[&str],
    blobs: &BTreeMap<String, Vec<u8>>,
    failed: &[&str],
) -> Vec<Reply> {
    let mut script = vec![list_ids(cids)];
    for cid in cids {
        if failed.contains(cid) {
            script.push(reply("getFile", reject("controlled unavailable image")));
        } else {
            download(&mut script, &format!("fid-{cid}"), blobs.get(*cid).unwrap());
        }
    }
    script
}

pub fn command(
    root: &Root,
    api: &str,
    provider: &str,
    args: &[String],
    values: &[(&str, &str)],
) -> rt::tooling_process::Completed {
    let name = if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    };
    let executable = root.0.join(name);
    if !executable.exists() {
        if std::fs::hard_link(binary(), &executable).is_err() {
            std::fs::copy(binary(), &executable).unwrap();
        }
        std::fs::create_dir_all(root.0.join("assets")).unwrap();
        std::fs::write(
            root.0.join("assets/panel.html"),
            b"owned command-root marker",
        )
        .unwrap();
    }
    let mut env = BTreeMap::new();
    for key in [
        "PATH",
        "SystemRoot",
        "SYSTEMROOT",
        "WINDIR",
        "LD_LIBRARY_PATH",
    ] {
        if let Ok(v) = std::env::var(key) {
            env.insert(key.into(), v);
        }
    }
    env.extend([
        ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
        ("NUMERA_TEST_API_BASE".into(), api.into()),
        ("NUMERA_TEST_PROVIDER_BASE".into(), provider.into()),
        ("COIN_PACK_BASE".into(), "cryptoemoji".into()),
        ("COIN_PACK_TITLE".into(), "T".into()),
    ]);
    env.extend(values.iter().map(|(k, v)| ((*k).into(), (*v).into())));
    let mut argv = vec![executable.into_os_string()];
    argv.extend(args.iter().map(std::ffi::OsString::from));
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let completed = runtime
        .block_on(rt::tooling_process::capture(
            &root.0,
            &argv,
            &env,
            Duration::from_secs(35),
            Duration::from_secs(15),
            &root.log(),
        ))
        .unwrap();
    for text in [&completed.stdout, &completed.stderr] {
        assert!(!text.contains(TOKEN));
    }
    assert!(!rt::process_owner::cleanup_pending(&root.0));
    completed
}
pub fn provider_top_up(root: &Path, path: &Path, ticker: &str, cid: &str) -> Result<(), String> {
    let _family = rt::locks::Lease::acquire(
        &rt::locks::pack_family_path(root, "cryptoemoji"),
        "provider foreign writer",
    )
    .map_err(|e| e.to_string())?;
    let _map = rt::locks::Lease::acquire(
        &root.join(".locks/canonical_map.lock"),
        "provider foreign map writer",
    )
    .map_err(|e| e.to_string())?;
    let mut mapping = doc(path);
    mapping[ticker] = json!(cid);
    rt::atomic::write_json(path, &mapping).map_err(|e| e.to_string())
}

pub fn binary() -> PathBuf {
    if let Some(path) = option_env!("CARGO_BIN_EXE_numera-emoji") {
        let path = PathBuf::from(path);
        assert!(path.is_file(), "Cargo standalone binary admission failed");
        return path;
    }
    let name = if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    };
    let current = std::env::current_exe().unwrap();
    if let Some(profile) = current.ancestors().find(|p| {
        matches!(
            p.file_name().and_then(|n| n.to_str()),
            Some("debug" | "release")
        )
    }) {
        let path = profile.join(name);
        if path.is_file() {
            return path;
        }
    }
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    for profile in ["debug", "release"] {
        let path = project.join("runtime/target").join(profile).join(name);
        if path.is_file() {
            return path;
        }
    }
    panic!("no real standalone Cargo executable admitted for coin baseline test");
}

pub struct Verify {
    pub root: Root,
    pub files: rt::coin_replace_disk::Files,
    pub old: Vec<u8>,
}
impl Verify {
    pub fn new() -> Self {
        let root = Root::new();
        let coins = root.0.join("coins");
        std::fs::create_dir(&coins).unwrap();
        let files = rt::coin_replace_disk::Files {
            root: root.0.clone(),
            map: root.0.join("ticker_to_id.json"),
            state: root.0.join("state.json"),
            images: root.0.join("emoji"),
            intent: coins.join("verify_logos_intent.json"),
            base: "cryptoemoji".into(),
        };
        std::fs::create_dir(&files.images).unwrap();
        std::fs::create_dir(root.0.join("png")).unwrap();
        let old = solid(48, [10, 200, 40, 255]);
        std::fs::write(files.images.join("btc.png"), &old).unwrap();
        std::fs::write(root.0.join("png/btc.png"), &old).unwrap();
        write(&files.map, &json!({"btc":OLD,"wbtc":OLD,"eth":"cid-eth"}));
        write(&files.state, &json!({"sets":[{"name":SET,"index":1}]}));
        Self { root, files, old }
    }
    pub fn before(&self) -> Vec<Value> {
        ["a", OLD, "c"].into_iter().map(sticker).collect()
    }
    pub fn after(&self) -> Vec<Value> {
        ["a", NEW, "c"].into_iter().map(sticker).collect()
    }
    pub fn unchanged(&self) {
        assert_eq!(
            std::fs::read(self.files.images.join("btc.png")).unwrap(),
            self.old
        );
        assert_eq!(
            std::fs::read(self.root.0.join("png/btc.png")).unwrap(),
            self.old
        );
    }
    pub fn mutation(&self, body: Body) -> Reply {
        let mut r = reply("replaceStickerInSet", Value::Null);
        r.body = body;
        r.observe = Some(self.files.intent.clone());
        r.action = Some(Box::new(|r| {
            let intent = r.observation.as_ref().unwrap();
            assert_eq!(intent["operation"], "replace");
            assert_eq!(intent["key"], "btc");
            assert_eq!(intent["old_cid"], OLD);
            assert_eq!(intent["set_index"], 1);
            assert_eq!(intent["before"], json!(["a", OLD, "c"]));
            assert_eq!(r.form["user_id"], "42");
            assert_eq!(r.form["old_sticker"], format!("fid-{OLD}"));
            assert!(r.files["file0"].starts_with(b"\x89PNG"));
        }));
        r
    }
    pub fn run(&self, server: &Server) -> Result<bool, String> {
        rt::coin_replace_effects::replace(
            &self.files,
            &server.telegram(),
            "btc",
            "Bitcoin",
            &noise("official-btc"),
            42,
        )
    }
    pub fn pending(&self) -> Vec<u8> {
        let server = Server::new(vec![
            listing(&self.before()),
            self.mutation(Body::Json(ok(json!(true)))),
            unavailable(),
        ]);
        assert!(!self.run(&server).unwrap());
        let r = server.finish();
        self.unchanged();
        assert!(self.files.intent.is_file());
        r.into_iter()
            .find(|r| r.method == "replaceStickerInSet")
            .unwrap()
            .files
            .remove("file0")
            .unwrap()
    }
    pub fn recovered(&self, bytes: &[u8]) -> Vec<Request> {
        let mut script = vec![listing(&self.after())];
        download(&mut script, &format!("fid-{NEW}"), bytes);
        let server = Server::new(script);
        assert!(rt::coin_replace_disk::run(&self.files, &server.telegram()).unwrap());
        let r = server.finish();
        assert!(!self.files.intent.exists());
        assert_eq!(
            doc(&self.files.map),
            json!({"btc":NEW,"wbtc":NEW,"eth":"cid-eth"})
        );
        assert_eq!(
            std::fs::read(self.files.images.join("btc.png")).unwrap(),
            bytes
        );
        r
    }
    pub fn args(&self, only: &str) -> Vec<String> {
        vec![
            "coin-verify-logos".into(),
            "--emoji-dir".into(),
            self.files.images.to_string_lossy().into_owned(),
            "--map".into(),
            self.files.map.to_string_lossy().into_owned(),
            "--state".into(),
            self.files.state.to_string_lossy().into_owned(),
            "--fix".into(),
            "--only".into(),
            only.into(),
        ]
    }
}
pub fn uploaded_download(script: &mut Vec<Reply>, fid: &str) {
    script.push(reply(
        "getFile",
        ok(json!({"file_path":format!("live/{fid}")})),
    ));
    let mut r = reply(&format!("GET live/{fid}"), Value::Null);
    r.body = Body::Uploaded("file0".into());
    script.push(r);
}
