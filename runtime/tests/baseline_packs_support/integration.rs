use super::support::*;
use serde_json::{Value, json};
use std::path::Path;
pub fn card(key: &str, pack: Option<u64>, included: bool, logo: bool) -> Value {
    let mut v = json!({"key":key,"label":key,"included":included});
    if let Some(n) = pack {
        v["pack"] = json!(n);
    }
    if logo {
        v["isLogo"] = json!(true);
    }
    v
}
pub fn build(view: Value, targets: Value, cap: u64) -> Value {
    numera_runtime::intent::execute(
        json!({"operation":"build","view":view,"targets":targets,"per_set":cap,"written_utc":UTC}),
    )
    .unwrap()
}
pub fn merge(previous: Value, live: Option<Value>) -> Value {
    let mut q = json!({"operation":"merge","previous":previous,"view":[card("here",Some(1),true,false)],"targets":{"here":1},"scope":["here"],"per_set":200,"written_utc":UTC});
    if let Some(l) = live {
        q["live"] = l;
    }
    numera_runtime::intent::execute(q).unwrap()
}
pub const ARCHIVE_KEY: &str = "s:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
pub const ARCHIVED_NAME: &str = "002_static_aaaaaaaaaaaa.webp";
pub fn archive_rows() -> (Value, Vec<Value>) {
    let items = json!({ARCHIVE_KEY:{"path":"x.webp","fmt":"static","cid":"1","set":"set1","keywords":["lock"]}});
    let rows = numera_runtime::archive_rows::rows(
        &json!({"name":"set1"}),
        &[
            json!({"custom_emoji_id":"logo-cid","emoji":"✅"}),
            json!({"custom_emoji_id":"1","emoji":"🔒"}),
        ],
        &items,
        &json!({"1":ARCHIVE_KEY}),
    )
    .unwrap();
    (items, rows)
}
pub struct Archive {
    pub root: Root,
    pub config: numera_runtime::config::Config,
}
impl Archive {
    pub fn new(
        live: u64,
        cid: &str,
        folder: bool,
        meta: bool,
        ids: &[&str],
        archived: bool,
    ) -> Self {
        let root = Root::new();
        let dir = root.0.join("archive/Pack One");
        std::fs::create_dir_all(&dir).unwrap();
        let media = if archived {
            root.write(&format!("archive/Pack One/{ARCHIVED_NAME}"), b"x")
        } else {
            root.write("collection/media/x.webp", b"x")
        };
        let db = root.0.join("collection/catalog.db");
        std::fs::create_dir_all(db.parent().unwrap()).unwrap();
        let con = rusqlite::Connection::open(db).unwrap();
        con.execute_batch("CREATE TABLE items(content_key TEXT,file_path TEXT,format TEXT,keywords TEXT);CREATE TABLE publications(base TEXT,content_key TEXT,set_name TEXT,custom_emoji_id TEXT);").unwrap();
        con.execute(
            "INSERT INTO items VALUES(?,?,?,?)",
            rusqlite::params![ARCHIVE_KEY, media.to_str().unwrap(), "static", "[\"lock\"]"],
        )
        .unwrap();
        con.execute(
            "INSERT INTO publications VALUES(?,?,?,?)",
            rusqlite::params!["pk", ARCHIVE_KEY, "set1", cid],
        )
        .unwrap();
        root.write("collection/publish_pk.json",json!({"sets":[{"name":"set1","title":"Pack One","index":1,"live":live,"fmt":"mixed"}]}).to_string());
        if meta {
            root.write("archive/Pack One/001_logo.png", b"logo");
            root.write("archive/Pack One/_history.md", b"x");
            root.write("archive/Pack One/_manifest.md", b"x");
            root.write(
                "archive/Pack One/_history.json",
                json!({"emoji":ids.iter().map(|id|json!({"premium_id":id})).collect::<Vec<_>>()} )
                    .to_string(),
            );
        }
        if !folder {
            std::fs::remove_dir_all(dir).unwrap();
        }
        let config = root.config(&[
            ("COLLECTION_PACK_BASE", "pk"),
            ("EMOJI_ARCHIVE_DIR", "archive"),
        ]);
        Self { root, config }
    }
    pub fn check(&self) -> (bool, Vec<String>) {
        numera_runtime::archive_check::check(&self.config).unwrap()
    }
}
pub fn apply_moves(mut order: Vec<String>, moves: &[Value]) -> Vec<String> {
    for row in moves {
        let pos = order
            .iter()
            .position(|x| x == row[1].as_str().unwrap())
            .unwrap();
        let s = order.remove(pos);
        order.insert(row[0].as_u64().unwrap() as usize, s);
    }
    order
}
pub fn sticker(cid: &str, glyph: &str, animated: bool, video: bool) -> Value {
    json!({"custom_emoji_id":cid,"emoji":glyph,"is_animated":animated,"is_video":video})
}
pub fn roster(stickers: Vec<Value>, family: &str, prov: Value, prior: Value, logo: bool) -> Value {
    numera_runtime::roster::pack(
        &json!({"name":if family=="coins"{"c"}else{"s"},"index":1,"logo":logo}),
        &json!({"stickers":stickers}),
        family,
        &prov,
        &prior,
        UTC,
    )
    .unwrap()
}
pub fn roster_doc(cid: &str, prior: Value) -> Value {
    roster(
        vec![
            sticker("logo", "😀", false, false),
            sticker(cid, "😀", false, false),
        ],
        "general",
        json!({cid:{"content_key":"k","name":"lock","source_emoji_ids":["999"]}}),
        prior,
        true,
    )
}
pub fn media(root: &Path) -> numera_runtime::media_native_adapter::NativeMediaAdapter {
    numera_runtime::media_native_adapter::NativeMediaAdapter {
        executable: binary(),
        root: project(),
        scratch: root.join("codec"),
    }
}
pub fn page(doc: &Value) -> String {
    let r = Root::new();
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(numera_runtime::pack_gallery::render(
        doc,
        &media(&r.0),
        &std::collections::BTreeMap::new(),
        &r.0.join("thumbs"),
    ))
    .unwrap()
}
pub fn roster_blob(page: &str) -> Value {
    serde_json::from_str(
        page.split_once("id=\"roster\">")
            .unwrap()
            .1
            .split_once("</script>")
            .unwrap()
            .0,
    )
    .unwrap()
}
pub fn png(path: &Path, color: [u8; 4]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut im = image::RgbaImage::new(100, 100);
    for x in 20..80 {
        for y in 20..80 {
            im.put_pixel(x, y, image::Rgba(color));
        }
    }
    im.save(path).unwrap();
}
pub fn sync_catalog(r: &Root, pairs: &[(Option<&str>, &str)]) -> numera_runtime::catalog::Catalog {
    let mut c = numera_runtime::catalog::Catalog::open(&r.0.join("catalog.db"), UTC, &r.0).unwrap();
    for (index, (id, key)) in pairs.iter().enumerate() {
        let file = r.write(&format!("item{index}.png"), b"not read by reorder");
        c.insert(
            &json!({"content_key":key,"fmt":"static","file_path":file,"emojis":[],"keywords":[]}),
            UTC,
        )
        .unwrap();
        if let Some(id) = id {
            c.uploaded(&json!({"content_key":key,"custom_emoji_id":id,"base":"pk","set_name":"pk1_by_bot"}),UTC).unwrap();
        }
    }
    c.order(&pairs.iter().map(|(_, k)| k.to_string()).collect::<Vec<_>>())
        .unwrap();
    c
}
pub fn sync_fixture(ids: &[&str]) -> http::Bot {
    let members = std::sync::Arc::new(std::sync::Mutex::new(live(ids)));
    http::Bot::new(move |r| {
        assert!(r.files.is_empty());
        match r.method.as_str() {
            "getStickerSet" => http::ok(json!({"stickers":members.lock().unwrap().clone()})),
            "setStickerPositionInSet" => {
                let mut v = members.lock().unwrap();
                let i = v
                    .iter()
                    .position(|s| s["file_id"] == r.form["sticker"])
                    .unwrap();
                let item = v.remove(i);
                v.insert(r.form["position"].parse().unwrap(), item);
                http::ok(json!(true))
            }
            m => panic!("unexpected sync wire method {m}"),
        }
    })
}
