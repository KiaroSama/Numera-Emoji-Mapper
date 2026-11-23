//! Owned fixtures for the95 original frozen rebuild, canonical-map and coin argument cases.
#![allow(dead_code)]
use crate::baseline_coin_provider_support as common;
// Integration and private-unit cohorts use distinct components of this one fixture module.
#[allow(unused_imports)]
pub use common::{Body, Reply, Root, Server, TOKEN, command, doc, ids, ok, reject, reply, write};
pub fn empty_provider() -> Root {
    let root = Root::new();
    write(&root.0.join("ticker_to_id.json"), &serde_json::json!({}));
    std::fs::write(root.0.join("currency-emoji-inventory.md"), "").unwrap();
    root
}

use numera_runtime as rt;
use serde_json::{Value, json};
use std::path::Path;
pub const ORIGINAL_TITLE: &str = "@YourBrand Crypto Emoji";
pub fn noise(key: &str) -> Vec<u8> {
    use sha2::{Digest, Sha256};
    let provenance: Value = serde_json::from_str(include_str!("fixtures/provenance.json")).unwrap();
    assert_eq!(
        provenance["baseline_revision"],
        "a39a049c02f8dcc90d18fe8771869f82dd655221"
    );
    assert_eq!(provenance["Pillow"], "12.3.0");
    let row = provenance["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == key)
        .unwrap();
    let data = std::fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/baseline_coin_state_support/fixtures")
            .join(row["file"].as_str().unwrap()),
    )
    .unwrap();
    let digest = Sha256::digest(&data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    assert_eq!(row["sha256"], digest);
    data
}
pub struct Rebuild {
    pub root: Root,
    pub files: rt::coin_rebuild_map_disk::Files,
}
impl Rebuild {
    pub fn new(reps: &[&str]) -> Self {
        let root = Root::new();
        let files = rt::coin_rebuild_map_disk::Files {
            root: root.0.clone(),
            directory: root.0.clone(),
            images: root.0.join("emoji"),
            base: "cryptoemoji".into(),
        };
        std::fs::create_dir(&files.images).unwrap();
        write(
            &files.directory.join("rebuild_dedup_plan.json"),
            &json!(
                reps.iter()
                    .map(|r| json!({"rep":r,"tickers":[r],"kw":r,"hash":r}))
                    .collect::<Vec<_>>()
            ),
        );
        for rep in reps {
            std::fs::write(files.images.join(format!("{rep}.png")), noise(rep)).unwrap();
        }
        std::fs::write(
            files.directory.join("currency-emoji-inventory.md"),
            "ticker: aaa\n",
        )
        .unwrap();
        let result = Self { root, files };
        result.state(json!({}));
        result
    }
    pub fn state(&self, overrides: Value) {
        let mut state = json!({"sets":[],"sent":[],"deleted_old":true,"final_sent":false,"order":[],"cursor":0,"in_flight":null});
        for (k, v) in overrides.as_object().unwrap() {
            state[k] = v.clone();
        }
        write(
            &self.files.directory.join("rebuild_dedup_state.json"),
            &state,
        );
    }
    pub fn saved(&self) -> Value {
        doc(&self.files.directory.join("rebuild_dedup_state.json"))
    }
    pub fn old(&self, names: &[&str]) {
        write(
            &self.files.directory.join("rebuild_state.json"),
            &json!({"sets":names.iter().map(|n|json!({"name":n})).collect::<Vec<_>>()}),
        );
    }
    pub fn run(&self, server: &Server) -> Result<(), String> {
        rt::coin_rebuild_effects::run(
            &common::config(
                &self.root.0,
                &[("COIN_PACK_TITLE", ORIGINAL_TITLE), ("BRAND_LOGO_BOTS", "")],
            ),
            &self.files,
            &rt::coin_rebuild_walk::Options {
                owner: 42,
                base: self.files.base.clone(),
                title: ORIGINAL_TITLE.into(),
                bot: "bot".into(),
            },
            &server.telegram(),
        )
    }
    pub fn rejects(&self, overrides: Value) -> String {
        let mut overrides = overrides;
        overrides
            .as_object_mut()
            .unwrap()
            .entry("deleted_old")
            .or_insert(json!(false));
        self.state(overrides);
        self.old(&["old1"]);
        let before = self.saved();
        let server = Server::new(vec![]);
        let error = self.run(&server).unwrap_err();
        assert!(
            server.finish().is_empty(),
            "invalid bookkeeping must stop before any wire effect"
        );
        assert_eq!(self.saved(), before);
        error
    }
}

pub fn members(name: &str, count: usize) -> Vec<Value> {
    (0..count).map(|i|json!({"custom_emoji_id":format!("{name}-{i}"),"file_id":format!("{name}#{i}"),"file_unique_id":format!("{name}#{i}")})).collect()
}
pub fn listing(name: &str, count: usize) -> Reply {
    let mut r = reply("getStickerSet", ok(json!({"stickers":members(name,count)})));
    let expected = name.to_owned();
    r.action = Some(Box::new(move |request| {
        assert_eq!(request.form["name"], expected)
    }));
    r
}
pub fn missing(name: &str) -> Reply {
    let mut r = reply("getStickerSet", reject("Bad Request: STICKERSET_INVALID"));
    let expected = name.to_owned();
    r.action = Some(Box::new(move |request| {
        assert_eq!(request.form["name"], expected)
    }));
    r
}
pub fn unknown(name: &str) -> Reply {
    let mut r = reply(
        "getStickerSet",
        reject("controlled live answer unavailable"),
    );
    let expected = name.to_owned();
    r.action = Some(Box::new(move |request| {
        assert_eq!(request.form["name"], expected)
    }));
    r
}
pub fn download(script: &mut Vec<Reply>, name: &str, index: usize, bytes: &[u8]) {
    let file_id = format!("{name}#{index}");
    let mut get = reply("getFile", ok(json!({"file_path":"original-image.png"})));
    get.action = Some(Box::new(move |request| {
        assert_eq!(request.form["file_id"], file_id)
    }));
    script.push(get);
    let mut body = reply("GET original-image.png", Value::Null);
    body.body = Body::Bytes(bytes.into());
    script.push(body);
}
pub fn notify() -> Reply {
    let mut r = reply("sendMessage", ok(json!({})));
    r.action = Some(Box::new(|req| {
        assert_eq!(req.form["chat_id"], "42");
        assert_eq!(req.form["disable_web_page_preview"], "true");
    }));
    r
}
impl Rebuild {
    pub fn mutate(&self, key: &str, name: &str, creating: bool, body: Body) -> Reply {
        let mut r = reply(
            if creating {
                "createNewStickerSet"
            } else {
                "addStickerToSet"
            },
            Value::Null,
        );
        r.body = body;
        r.observe = Some(self.files.directory.join("rebuild_dedup_state.json"));
        let key = key.to_owned();
        let name = name.to_owned();
        let family = rt::locks::pack_family_path(&self.root.0, &self.files.base);
        r.action = Some(Box::new(move |req| {
            assert!(
                rt::locks::Lease::acquire(&family, "actual competing publisher at mutation")
                    .is_err(),
                "real family lock must be held at mutation"
            );
            let marker = &req.observation.as_ref().unwrap()["in_flight"];
            assert_eq!(marker["key"], key);
            assert_eq!(marker["set_name"], name);
            assert_eq!(marker["operation"], if creating { "create" } else { "add" });
            assert_eq!(req.form["name"], name);
            assert_eq!(req.form["user_id"], "42");
            assert_eq!(req.files["file0"], noise(&key));
        }));
        r
    }
    pub fn released(&self) {
        drop(
            rt::locks::Lease::acquire(
                &rt::locks::pack_family_path(&self.root.0, &self.files.base),
                "release positive control",
            )
            .unwrap(),
        );
        assert!(!rt::process_owner::cleanup_pending(&self.root.0));
    }
}

pub fn me() -> Reply {
    common::me()
}
impl Rebuild {
    pub fn build_cli(&self, server: &Server) -> rt::tooling_process::Completed {
        command(
            &self.root,
            &server.base,
            &server.base,
            &ids(&[
                "coin-rebuild",
                "build",
                "--coin-dir",
                self.files.directory.to_str().unwrap(),
                "--emoji-dir",
                self.files.images.to_str().unwrap(),
            ]),
            &[
                ("TELEGRAM_BOT_TOKEN", TOKEN),
                ("PACK_OWNER_USER_ID", "42"),
                ("COIN_PACK_TITLE", ORIGINAL_TITLE),
                ("BRAND_LOGO_BOTS", ""),
            ],
        )
    }
}

pub fn mapping_fixture() -> Rebuild {
    let f = Rebuild::new(&["aaa", "bbb", "ccc"]);
    f.state(json!({"sets":[{"index":1,"name":"s1","title":"T 1"}],"order":["aaa","bbb","ccc"],"cursor":3}));
    f
}
pub fn mapping_script(keys: &[&str]) -> Vec<Reply> {
    let mut script = vec![listing("s1", keys.len())];
    for (i, key) in keys.iter().enumerate() {
        download(&mut script, "s1", i, &noise(key));
    }
    script
}
impl Rebuild {
    pub fn map(&self, server: &Server) -> Result<(), String> {
        rt::coin_rebuild_map_disk::run(&self.files, &server.telegram())
    }
    pub fn mapping(&self) -> Value {
        doc(&self.files.directory.join("ticker_to_id.json"))
    }
    pub fn write_mapping(&self, value: Value) {
        write(&self.files.directory.join("ticker_to_id.json"), &value)
    }
}

pub const CREATED: &str = "cryptoemoji1_by_bot";
pub fn pending_create(reps: &[&str]) -> Rebuild {
    let f = Rebuild::new(reps);
    f.state(json!({"sets":[],"order":[],"cursor":1,"in_flight":{"key":"aaa","operation":"create","set_name":CREATED,"set_index":1,"expected_before":0,"phase":"upload"}}));
    f
}
pub fn pending_add() -> Rebuild {
    let f = Rebuild::new(&["seed", "aaa"]);
    f.state(json!({"sets":[{"index":1,"name":"s1","title":"T 1"}],"order":["seed"],"cursor":2,"in_flight":{"key":"aaa","operation":"add","set_name":"s1","set_index":1,"expected_before":1,"phase":"upload"}}));
    f
}
pub fn no_mutations(requests: &[common::Request]) {
    assert!(requests.iter().all(|r| r.method != "createNewStickerSet"
        && r.method != "addStickerToSet"
        && r.method != "deleteStickerSet"));
}

pub const NAME_INVENTORY: &str = "## 🟡 — Foo Protocol\n   ticker: ccc\n   premium-id:\n\n## 🟢 — Bar Coin\n   ticker: eee\n   premium-id:\n";
pub fn alias_fixture() -> Root {
    let root = Root::new();
    write(
        &root.0.join("ticker_to_id.json"),
        &json!({"aaa":"111","bbb":"222","ddd":"333"}),
    );
    std::fs::write(root.0.join("keywords.csv"),"ticker,name,format,file,keywords\naaa,Foo Network,svg,logos/svg/aaa.svg,aaa\nbbb,Foo Token,svg,logos/svg/bbb.svg,bbb\nddd,Bar Labs,svg,logos/svg/ddd.svg,ddd\n").unwrap();
    std::fs::write(root.0.join("currency-emoji-inventory.md"), NAME_INVENTORY).unwrap();
    root
}
pub fn filled(root: &Root) -> String {
    std::fs::read_to_string(root.0.join("currency-emoji-inventory.filled.md"))
        .unwrap()
        .replace("\r\n", "\n")
}
