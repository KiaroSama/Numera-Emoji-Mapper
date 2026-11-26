//! Sibling catalogs exercise the installed producer and its real codec worker.
use crate::{installed, read_only_api, root::Root};
use numera_runtime::{catalog::Catalog, codec_format, logging::RunLog, tooling_process};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

#[path = "external_preview.rs"]
mod preview;
#[path = "external_privacy.rs"]
mod privacy;
#[path = "external_publication.rs"]
mod publication;

const STATIC: &str = "static-codec/public-logo.png";
const STATIC_KEY: &str = "s:1745338c58098cc08d6406ac9d629f6a";
const VIDEO: &str = "video-fingerprint/fast15.webm";
const EMOJI_ID: &str = "990000001";

fn frozen(name: &str) -> Vec<u8> {
    let hash = match name {
        STATIC => "6f85376de3893dfa1e78045ce264103b0055acec400dd8a35222405d5c4f6951",
        VIDEO => "91baf8d30ae00bc60584ed617bf355933885b8c7a4a493f573cc60fb560ba04e",
        "static-codec/opaque.png" => {
            "186ee151bdd1301c0e6c82e4e18550d248d4be36b8a0ae4c37ec6b91d44622f4"
        }
        _ => panic!("unrecognized fixture"),
    };
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/fixtures/native")
        .join(name);
    assert_eq!(codec_format::file_sha256(&path).unwrap(), hash);
    fs::read(path).unwrap()
}

struct Installation {
    outer: Root,
    project: PathBuf,
    binary: PathBuf,
    data: PathBuf,
    env: BTreeMap<String, String>,
}
impl Installation {
    fn new() -> Self {
        let outer = Root::new("external catalog café");
        let project = outer.0.join("installed app");
        let data = outer.0.join("installed app sibling داده");
        fs::create_dir(&project).unwrap();
        fs::create_dir(&data).unwrap();
        assert!(
            !data
                .canonicalize()
                .unwrap()
                .starts_with(project.canonicalize().unwrap())
        );
        let (binary, mut env) = installed::install(
            Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap(),
            &project,
        );
        env.insert("NUMERA_TEST_API_BASE".into(), "http://127.0.0.1:9".into());
        Self {
            outer,
            project,
            binary,
            data,
            env,
        }
    }
    fn run(&self, args: &[&str], data: &Path) -> tooling_process::Completed {
        let mut argv = vec![self.binary.as_os_str().to_owned()];
        argv.extend(args.iter().map(OsString::from));
        argv.extend(["--data-dir".into(), data.as_os_str().to_owned()]);
        let log = RunLog::open(
            &self.outer.0,
            "external-catalog",
            vec![read_only_api::TOKEN.into()],
        );
        runtime()
            .block_on(tooling_process::capture(
                &self.outer.0,
                &argv,
                &self.env,
                Duration::from_secs(45),
                Duration::from_secs(20),
                &log,
            ))
            .unwrap()
    }
    fn base(&self) -> PathBuf {
        self.project.join("logs/tmp/external-data/tmp")
    }
    fn catalog(&self) -> Catalog {
        Catalog::open(&self.data.join("catalog.db"), "fixture", &self.project).unwrap()
    }
    fn seed(&self, key: &str, format: &str, name: &str, bytes: &[u8], phash: u64) -> PathBuf {
        let path = self.data.join(name);
        fs::write(&path, bytes).unwrap();
        assert!(
            self.catalog()
                .insert(
                    &json!({"content_key":key,"fmt":format,"file_path":path,
            "emojis":["😀"],"keywords":[],"sources":["fixture"],"phash":phash}),
                    "fixture"
                )
                .unwrap()
        );
        path
    }
    fn api(&mut self, base: String) {
        self.env
            .insert("GENERAL_BOT_TOKEN".into(), read_only_api::TOKEN.into());
        self.env.insert("NUMERA_TEST_API_BASE".into(), base);
    }
}
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}
fn empty(path: &Path) {
    assert!(!numera_runtime::process_owner::cleanup_pending(path));
    assert!(
        fs::read_dir(path).unwrap().next().is_none(),
        "remaining evidence at {}",
        path.display()
    );
}
fn success(result: tooling_process::Completed) -> String {
    assert_eq!(result.code, 0, "{} {}", result.stdout, result.stderr);
    result.stdout
}
fn sticker() -> Value {
    json!({"file_id":"remote","file_unique_id":"unique","custom_emoji_id":EMOJI_ID,
        "emoji":"😀","is_animated":false,"is_video":false})
}

#[test]
fn local_import_preserves_inputs_durable_media_and_internal_scratch_layout() {
    let app = Installation::new();
    let inputs = app.outer.0.join("input café");
    fs::create_dir(&inputs).unwrap();
    let original = frozen(STATIC);
    fs::write(inputs.join("لوگو.png"), &original).unwrap();
    fs::write(inputs.join(".convert.lock"), b"owned control bytes").unwrap();
    let internal = app.project.join("internal data");
    for (data, scratch) in [(&app.data, app.base()), (&internal, internal.join("tmp"))] {
        for counts in ["new=1 dedup=0 failed=0", "new=0 dedup=1 failed=0"] {
            let out = success(app.run(&["add-media", "--in", inputs.to_str().unwrap()], data));
            assert!(out.contains(counts), "{out}");
            empty(&scratch);
            let rows = Catalog::open(&data.join("catalog.db"), "observe", &app.project)
                .unwrap()
                .all(None)
                .unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0]["sources"], json!(["local:لوگو.png"]));
            let path = Path::new(rows[0]["file_path"].as_str().unwrap())
                .canonicalize()
                .unwrap();
            assert!(path.starts_with(data.join("media/static").canonicalize().unwrap()));
            let pixels = image::open(path).unwrap().to_rgba8();
            assert_eq!((pixels.width(), pixels.height()), (100, 100));
            assert_eq!(
                numera_runtime::native_raster::digest_hex(pixels.as_raw()),
                "5ee0faa7c2dfa0bc29655b4c215b1de1ae132c521cd29d266d37ac3a9a401f86"
            );
        }
    }
    assert!(!app.data.join("tmp").exists());
    assert_eq!(
        fs::read(inputs.join(".convert.lock")).unwrap(),
        b"owned control bytes"
    );
    assert_eq!(fs::read(inputs.join("لوگو.png")).unwrap(), original);
}

#[test]
fn both_fetch_commands_record_the_downloaded_identity_in_the_sibling_catalog() {
    use read_only_api::{Fixture, Reply};
    for pack in [true, false] {
        let mut app = Installation::new();
        let bytes = frozen(STATIC);
        let mut replies = if pack {
            vec![Reply::api(
                "getStickerSet",
                "name",
                "fixture_pack",
                json!({"stickers":[sticker()]}),
            )]
        } else {
            vec![
                Reply::api("getMe", "", "", json!({"username":"fixturebot"})),
                Reply::api(
                    "getCustomEmojiStickers",
                    "custom_emoji_ids",
                    &json!([EMOJI_ID]).to_string(),
                    json!([sticker()]),
                ),
            ]
        };
        replies.extend([
            Reply::api(
                "getFile",
                "file_id",
                "remote",
                json!({"file_path":"logo.png"}),
            ),
            Reply::file("logo.png", &bytes),
        ]);
        let count = replies.len();
        let mut service = Fixture::new(replies);
        app.api(service.base.clone());
        let args = if pack {
            vec!["fetch-pack", "fixture_pack"]
        } else {
            vec!["fetch-emoji-ids", "--id", EMOJI_ID]
        };
        let out = success(app.run(&args, &app.data));
        assert!(out.contains("new=1 dedup=0 failed=0"), "{out}");
        assert_eq!(service.finish().unwrap().len(), count);
        let mut cat = app.catalog();
        assert_eq!(cat.all(None).unwrap().len(), 1);
        let row = cat.get(STATIC_KEY).unwrap().unwrap();
        assert_eq!(
            row["sources"],
            json!([if pack {
                "fixture_pack"
            } else {
                "bot-inventory"
            }])
        );
        if !pack {
            assert!(
                row["keywords"]
                    .as_array()
                    .unwrap()
                    .contains(&json!(format!("premium-id:{EMOJI_ID}")))
            );
        }
        assert_eq!(
            cat.dispatch(json!({"operation":"seen","file_unique_id":"unique"}))
                .unwrap(),
            STATIC_KEY
        );
        let path = Path::new(row["file_path"].as_str().unwrap())
            .canonicalize()
            .unwrap();
        assert!(path.starts_with(app.data.join("media/static").canonicalize().unwrap()));
        let bytes = fs::read(path).unwrap();
        assert_eq!(
            image::guess_format(&bytes).unwrap(),
            image::ImageFormat::WebP
        );
        let img = image::load_from_memory(&bytes).unwrap();
        assert_eq!((img.width(), img.height()), (512, 512));
        empty(&app.base().join("native-codec"));
        assert!(!app.data.join("tmp/native-codec").exists());
        empty(&app.data.join("tmp"));
    }
}

#[test]
fn missing_or_invalid_roots_are_errors_before_external_scratch_selection() {
    let app = Installation::new();
    let config =
        numera_runtime::config::Config::load_with_environment(app.project.clone(), &app.env)
            .unwrap();
    let missing = app.outer.0.join("absent catalog");
    let file = app.outer.0.join("ordinary file");
    fs::write(&file, b"catalog control").unwrap();
    for data in [&missing, &file] {
        assert!(
            config
                .codec_directory(data, "tmp/native-codec", false)
                .is_err()
        );
    }
    assert!(!app.project.join("logs/tmp/external-data").exists());
    config.codec_directory(&app.data, "tmp", true).unwrap();
    let wrong_type = app.base().join("native-codec");
    fs::write(&wrong_type, b"scratch control").unwrap();
    assert!(
        config
            .codec_directory(&app.data, "tmp/native-codec", false)
            .is_err()
    );
    assert_eq!(fs::read(&wrong_type).unwrap(), b"scratch control");
    assert_eq!(fs::read(&file).unwrap(), b"catalog control");
}

#[test]
fn adapter_still_refuses_a_direct_external_envelope() {
    let app = Installation::new();
    let destination = app.data.join("explicit envelope");
    let adapter = numera_runtime::media_native_adapter::NativeMediaAdapter {
        executable: app.binary.clone(),
        root: app.project.clone(),
        scratch: destination.clone(),
    };
    let message = runtime()
        .block_on(
            adapter.call(json!({"operation":"fingerprint","source":"unused","format":"static"})),
        )
        .unwrap_err();
    assert!(
        message.contains("codec envelope must stay inside project"),
        "{message}"
    );
    assert!(!destination.exists());
}

#[test]
fn linked_data_and_selected_temp_ancestors_are_refused_before_import() {
    #[cfg(windows)]
    use junction::create as link_directory;
    #[cfg(unix)]
    use std::os::unix::fs::symlink as link_directory;
    let app = Installation::new();
    let source = app.outer.0.join("unchanged.png");
    let bytes = frozen(STATIC);
    fs::write(&source, &bytes).unwrap();
    let alias = app.outer.0.join("linked catalog");
    link_directory(&app.data, &alias).unwrap();
    let args = ["add-media", source.to_str().unwrap()];
    let sep = std::path::MAIN_SEPARATOR;
    for spelling in [
        format!("{}{sep}", alias.display()),
        format!("{}{sep}.", alias.display()),
    ] {
        let result = app.run(&args, Path::new(&spelling));
        assert_ne!(result.code, 0);
        assert!(fs::read_dir(&app.data).unwrap().next().is_none());
    }
    let result = app.run(&args, &alias.join("new catalog"));
    assert_ne!(result.code, 0);
    assert!(!app.data.join("new catalog").exists());
    link_directory(&app.data, app.project.join("logs/tmp/external-data")).unwrap();
    let result = app.run(&args, &app.data);
    assert_ne!(result.code, 0);
    assert!(fs::read_dir(&app.data).unwrap().next().is_none());
    assert_eq!(fs::read(&source).unwrap(), bytes);
}
