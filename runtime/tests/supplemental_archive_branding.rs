//! Archive branding follows explicit publication state while retaining legacy branded behavior.
use numera_runtime::{
    archive_check, archive_rows, archive_sync, atomic, catalog::Catalog, config::Config,
    logging::RunLog, telegram::Telegram,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[allow(dead_code)]
#[path = "support/read_only_api.rs"]
mod read_only_api;
#[path = "support/test_admission.rs"]
mod test_admission;
use read_only_api::{Fixture, Reply, TOKEN};

const SET: &str = "archives1_by_YourEmojiBot";

struct Archive {
    _root: tempfile::TempDir,
    config: Config,
    sources: BTreeMap<String, (PathBuf, Vec<u8>)>,
    live: Vec<Value>,
    logo: PathBuf,
}
impl Archive {
    fn new(logo_flag: Option<bool>, configure_logo: bool) -> Self {
        let root = tempfile::tempdir().unwrap();
        let data = root.path().join("collection");
        let media = data.join("media/static");
        std::fs::create_dir_all(&media).unwrap();
        let logo = root.path().join("configured-logo.png");
        image::RgbaImage::from_pixel(2, 2, image::Rgba([230, 40, 50, 255]))
            .save(&logo)
            .unwrap();
        let mut environment = BTreeMap::from([
            ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
            ("COLLECTION_PACK_BASE".into(), "archive".into()),
            ("EMOJI_ARCHIVE_DIR".into(), "archive".into()),
        ]);
        if configure_logo {
            environment.insert("BRAND_LOGO_PATH".into(), "configured-logo.png".into());
        }
        let config = Config::load_with_environment(root.path().into(), &environment).unwrap();
        let mut catalog =
            Catalog::open(&data.join("catalog.db"), "fixture UTC", root.path()).unwrap();
        let mut sources = BTreeMap::new();
        let mut live = Vec::new();
        if logo_flag != Some(false) {
            live.push(json!({"file_id":"logo-file","file_unique_id":"logo-fuid",
                "custom_emoji_id":"logo-cid","emoji":"😀"}));
        }
        for index in 0..(200 - live.len()) {
            let key = format!("s:{index:012x}{index:020x}");
            let source = media.join(format!("item-{index}.png"));
            image::RgbaImage::from_pixel(2, 2, image::Rgba([index as u8, 90, 180, 255]))
                .save(&source)
                .unwrap();
            catalog
                .insert(
                    &json!({"content_key":key,"fmt":"static","file_path":source,
                        "keywords":[format!("item-{index}")]}),
                    "fixture UTC",
                )
                .unwrap();
            let cid = format!("{}", 10000 + index);
            catalog
                .uploaded(
                    &json!({"content_key":key,"custom_emoji_id":cid,"base":"archive","set_name":SET}),
                    "fixture UTC",
                )
                .unwrap();
            sources.insert(key, (source.clone(), std::fs::read(source).unwrap()));
            live.push(json!({"file_id":format!("file-{index}"),
                "file_unique_id":format!("fuid-{index}"),"custom_emoji_id":cid,"emoji":"😀"}));
        }
        drop(catalog);
        let mut record = json!({"name":SET,"title":"Full pack","fmt":"static",
            "index":1,"live":200,"keys":sources.keys().collect::<Vec<_>>()});
        if let Some(flag) = logo_flag {
            record["logo"] = json!(flag);
        }
        atomic::write_json(
            &data.join("publish_archive.json"),
            &json!({"base":"archive","sets":[record]}),
        )
        .unwrap();
        Self {
            _root: root,
            config,
            sources,
            live,
            logo,
        }
    }

    fn sync(&self) -> Result<u8, String> {
        let mut server = Fixture::new(vec![Reply::api(
            "getStickerSet",
            "name",
            SET,
            json!({"name":SET,"stickers":self.live}),
        )]);
        let tg = Telegram::new(TOKEN.into(), server.base.clone()).unwrap();
        let log = RunLog::open(&self.config.root, "archive-branding", vec![]);
        let result = archive_sync::run(&self.config, &tg, &log);
        assert_eq!(server.finish().unwrap().len(), 1);
        result
    }
}

fn unbranded_sync(configure_logo: bool) {
    let archive = Archive::new(Some(false), configure_logo);
    let logo_before = std::fs::read(&archive.logo).unwrap();
    assert_eq!(archive.sync().unwrap(), 0);
    let folder = archive.config.root.join("archive").join("Full pack");
    assert!(
        !folder.join(archive_rows::LOGO_NAME).exists(),
        "unbranded pack acquired a foreign logo"
    );
    assert_eq!(std::fs::read(&archive.logo).unwrap(), logo_before);
    let catalog = Catalog::open(
        &archive.config.root.join("collection/catalog.db"),
        "reopen UTC",
        &archive.config.root,
    )
    .unwrap();
    let history: Value =
        serde_json::from_slice(&std::fs::read(folder.join("_history.json")).unwrap()).unwrap();
    assert_eq!(history["total"], 200);
    let rows = history["emoji"].as_array().unwrap();
    assert_eq!(rows.len(), 200);
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row["position"], index + 1);
        let key = row["content_key"].as_str().unwrap();
        let (source, bytes) = &archive.sources[key];
        let destination = folder.join(row["file"].as_str().unwrap());
        assert_eq!(std::fs::read(&destination).unwrap(), *bytes);
        assert!(!source.exists());
        assert_eq!(
            catalog.get(key).unwrap().unwrap()["file_path"],
            destination.to_str().unwrap()
        );
        assert!(!row["premium_id"].as_str().unwrap().is_empty());
    }
    drop(catalog);
    assert_eq!(
        archive_check::check(&archive.config).unwrap(),
        (false, vec![])
    );
    assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 203);
}

#[test]
fn full_unbranded_pack_archives_without_logo_configuration() {
    unbranded_sync(false);
}

#[test]
fn full_unbranded_pack_does_not_copy_an_unrelated_configured_logo() {
    unbranded_sync(true);
}

#[test]
fn explicit_unbranded_first_slot_is_not_reported_as_a_logo() {
    let live = [json!({"custom_emoji_id":"foreign-cid","emoji":"😀"})];
    for flag in [Some(false), Some(true), None] {
        let mut record = json!({"name":SET,"title":"Full pack"});
        if let Some(flag) = flag {
            record["logo"] = json!(flag);
        }
        let rows = archive_rows::rows(&record, &live, &json!({}), &json!({})).unwrap();
        if flag == Some(false) {
            assert!(rows[0]["file"].is_null());
            assert_eq!(rows[0]["premium_id"], "foreign-cid");
            assert_eq!(rows[0]["note"], "not in the catalog (unidentified)");
        } else {
            assert_eq!(rows[0]["file"], archive_rows::LOGO_NAME);
            assert!(rows[0]["premium_id"].is_null());
        }
    }
}

#[test]
fn branded_and_legacy_archives_still_require_the_configured_logo() {
    for flag in [Some(true), None] {
        let archive = Archive::new(flag, false);
        assert!(archive.sync().unwrap_err().contains("BRAND_LOGO_PATH"));
        for (source, bytes) in archive.sources.values() {
            assert_eq!(std::fs::read(source).unwrap(), *bytes);
        }
    }
}
