use super::*;
use crate::{
    baseline_catalog_support::{Scratch, read_json},
    baseline_cli_http::{Reply, Server},
    media_adapter::MediaCodec,
    media_native_adapter::NativeMediaAdapter,
};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}
fn write(path: &Path, bytes: impl AsRef<[u8]>) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}
fn document(path: &Path, value: &Value) {
    write(path, serde_json::to_vec(value).unwrap());
}
fn clip(root: &Path) -> PathBuf {
    let raw = root.join("clip.rgba");
    write(&raw, [220_u8, 20, 20, 255].repeat(100 * 100 * 3));
    let out = root.join("clip.webm");
    let mut args = [
        "-y",
        "-v",
        "error",
        "-f",
        "rawvideo",
        "-pixel_format",
        "rgba",
        "-video_size",
        "100x100",
        "-framerate",
        "30",
        "-i",
    ]
    .map(str::to_owned)
    .to_vec();
    args.push(raw.display().to_string());
    args.extend(
        [
            "-c:v",
            "libvpx-vp9",
            "-threads",
            "1",
            "-cpu-used",
            "8",
            "-lossless",
            "1",
            "-pix_fmt",
            "yuva420p",
            "-auto-alt-ref",
            "0",
        ]
        .map(str::to_owned),
    );
    args.push(out.display().to_string());
    runtime()
        .block_on(crate::codec_process::capture_with_budget(
            root,
            &crate::codec_process::executable("ffmpeg").unwrap(),
            &args,
            1024 * 1024,
            None,
            &std::env::vars_os().collect(),
            crate::codec_process::Budget {
                wall: Duration::from_secs(30),
                idle: Duration::from_secs(30),
                warning: None,
            },
        ))
        .unwrap();
    out
}
struct Tap {
    inner: NativeMediaAdapter,
    data: PathBuf,
    receipt: Arc<Mutex<Value>>,
}
impl MediaCodec for Tap {
    fn with_scratch(&self, scratch: PathBuf) -> Self {
        Self {
            inner: self.inner.with_scratch(scratch),
            data: self.data.clone(),
            receipt: self.receipt.clone(),
        }
    }
    async fn call(&self, request: Value) -> Result<Value, String> {
        let result = self.inner.call(request.clone()).await?;
        if request["operation"] == "prepare" {
            let source = Path::new(request["source"].as_str().unwrap());
            let payload = std::fs::read(source).unwrap();
            let key = result["content_key"].as_str().unwrap();
            let collision = self
                .inner
                .call(json!({"operation":"collision","source":source,"format":"video","key":key}))
                .await?;
            let base = self
                .data
                .join("media/video")
                .join(format!("{}.webm", key.replace(':', "_")));
            let alternate = base.with_file_name(format!(
                "{}.webm",
                collision.as_str().unwrap().replace(':', "_")
            ));
            let occupied = vec![
                (base, b"existing unrelated bytes".to_vec()),
                (alternate, payload[..payload.len() / 2].to_vec()),
            ];
            for (path, bytes) in &occupied {
                write(path, bytes);
            }
            let incoming = self.data.parent().unwrap().join("normalized-incoming.webm");
            write(&incoming, &payload);
            *self.receipt.lock().unwrap() =
                json!({"incoming":incoming,"occupied":occupied,"content_key":key});
        }
        Ok(result)
    }
}
// Technical self-exec entry; not an original baseline case.
#[test]
fn retention_worker() {
    let Some(path) = std::env::var_os("NUMERA_CLI_RETENTION_SPEC") else {
        return;
    };
    let spec = read_json(Path::new(&path));
    let root = PathBuf::from(spec["root"].as_str().unwrap());
    let data = PathBuf::from(spec["data"].as_str().unwrap());
    let config = Config::load_with_environment(
        root.clone(),
        &BTreeMap::from([
            ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
            ("TEST_AUTH".into(), "fixture".into()),
        ]),
    )
    .unwrap();
    let log = RunLog::open(&root, "retention", vec!["fixture".into()]);
    let receipt = Arc::new(Mutex::new(Value::Null));
    let factory = |config: &Config, scratch: PathBuf| {
        Ok(Tap {
            inner: NativeMediaAdapter::for_project(&config.root, scratch)?,
            data: data.clone(),
            receipt: receipt.clone(),
        })
    };
    let mut args = if spec["route"] == "pack" {
        vec!["fixture".to_owned()]
    } else {
        vec!["--id".into(), "12345".into()]
    };
    args.extend([
        "--data-dir".into(),
        data.display().to_string(),
        "--repaintable".into(),
        "keep".into(),
        "--token-env".into(),
        "TEST_AUTH".into(),
    ]);
    let code = if spec["route"] == "pack" {
        super::run_with_codec_factory(&config, &args, &log, factory)
    } else {
        crate::fetch_ids_cli::run_with_codec_factory(&config, &args, &log, factory)
    }
    .unwrap();
    assert_eq!(code, 4);
    let mut receipt = receipt.lock().unwrap().clone();
    receipt["code"] = json!(code);
    document(Path::new(spec["receipt"].as_str().unwrap()), &receipt);
}
// tests.test_ingest_retention.RefusedDownloadRetention.test_both_cli_cleanups_preserve_complete_input_beside_occupied_targets
#[test]
fn both_actual_fetch_cli_cleanups_preserve_refused_complete_video() {
    let root = Scratch::new();
    let source = clip(&root.0);
    let payload = std::fs::read(source).unwrap();
    let bot = Server::new(move |r| {
        if r.verb == "GET" {
            Reply::Bytes(200, payload.clone())
        } else {
            match r.method() {
                "getMe" => Reply::ok(json!({"username":"fixture_bot"})),
                "getStickerSet" => Reply::ok(
                    json!({"stickers":[{"file_id":"12345","file_unique_id":"fuid-12345","emoji":"😀","is_video":true}]}),
                ),
                "getCustomEmojiStickers" => Reply::ok(
                    json!([{"custom_emoji_id":"12345","file_id":"12345","file_unique_id":"fuid-12345","emoji":"😀","is_video":true}]),
                ),
                "getFile" => Reply::ok(json!({"file_path":"clip.webm"})),
                _ => panic!("unexpected {}", r.method()),
            }
        }
    });
    for route in ["pack", "ids"] {
        let folder = root.0.join(route);
        std::fs::create_dir(&folder).unwrap();
        let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        write(
            &folder.join("assets/panel.html"),
            std::fs::read(project.join("assets/panel.html")).unwrap(),
        );
        let name = format!("numera-emoji{}", std::env::consts::EXE_SUFFIX);
        let binary = option_env!("CARGO_BIN_EXE_numera-emoji")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("target/debug")
                    .join(&name)
            });
        std::fs::copy(binary, folder.join(&name)).unwrap();
        let data = folder.join("data");
        let receipt = folder.join("receipt.json");
        let spec = folder.join("spec.json");
        document(
            &spec,
            &json!({"root":folder,"data":data,"route":route,"receipt":receipt}),
        );
        let stdout = folder.join("stdout");
        let stderr = folder.join("stderr");
        let mut cmd = Command::new(std::env::current_exe().unwrap());
        cmd.args([
            "--exact",
            "fetch_cli::baseline_cli_retention_tests::retention_worker",
            "--nocapture",
        ])
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(&stdout).unwrap())
        .stderr(std::fs::File::create(&stderr).unwrap())
        .env("NUMERA_CLI_RETENTION_SPEC", &spec)
        .env("NUMERA_TEST_API_BASE", &bot.base);
        std::fs::create_dir(folder.join("owner")).unwrap();
        let mut child =
            crate::process_owner::OwnedProcess::spawn_session(&mut cmd, folder.join("owner"))
                .unwrap();
        let status = runtime()
            .block_on(child.wait(Duration::from_secs(120), Duration::from_secs(110)))
            .unwrap();
        child.finish().unwrap();
        let out = std::fs::read_to_string(&stdout).unwrap();
        let err = std::fs::read_to_string(&stderr).unwrap();
        assert!(status.success(), "{route}: {out}; {err}");
        assert!(out.contains("failed=1"));
        let receipt = read_json(&receipt);
        assert_eq!(receipt["code"], 4);
        let incoming = std::fs::read(receipt["incoming"].as_str().unwrap()).unwrap();
        let occupied: Vec<(PathBuf, Vec<u8>)> =
            serde_json::from_value(receipt["occupied"].clone()).unwrap();
        for (path, sentinel) in &occupied {
            assert_eq!(std::fs::read(path).unwrap(), *sentinel);
        }
        fn files(root: &Path, out: &mut Vec<PathBuf>) {
            if !root.exists() {
                return;
            }
            for entry in std::fs::read_dir(root).unwrap() {
                let p = entry.unwrap().path();
                if p.is_dir() {
                    files(&p, out);
                } else {
                    out.push(p);
                }
            }
        }
        let mut tmp = Vec::new();
        files(&data.join("tmp"), &mut tmp);
        assert!(!tmp.iter().any(|p| p.extension().is_some_and(|e| e == "dl")));
        let mut paths = Vec::new();
        files(&data.join("media"), &mut paths);
        let retained = paths
            .iter()
            .filter(|p| {
                p.extension().is_some_and(|e| e == "webm")
                    && !occupied.iter().any(|(path, _)| path == *p)
                    && std::fs::read(p).unwrap() == incoming
            })
            .collect::<Vec<_>>();
        assert_eq!(retained.len(), 1);
        let record = read_json(&retained[0].with_extension("json"));
        assert_eq!(
            record["sha256"],
            crate::native_raster::digest_hex(&incoming)
        );
        assert_eq!(record["size"], incoming.len());
        assert_eq!(record["provenance"]["file_unique_id"], "fuid-12345");
        let retry = data.join("tmp/retry/retry.dl");
        write(&retry, &incoming);
        let adapter =
            NativeMediaAdapter::for_project(&folder, data.join("tmp/retry-codec")).unwrap();
        let error = runtime()
            .block_on(crate::media_store::store(
                &adapter,
                &retry,
                &occupied[0].0,
                "video",
                record["content_key"].as_str().unwrap(),
                &record["provenance"],
            ))
            .unwrap_err();
        assert!(error.contains("retained at"));
        let mut later = Vec::new();
        files(retained[0].parent().unwrap(), &mut later);
        assert_eq!(
            later
                .into_iter()
                .filter(|p| p.extension().is_some_and(|e| e == "webm"))
                .collect::<Vec<_>>(),
            retained.into_iter().cloned().collect::<Vec<_>>()
        );
        let mut cat = Catalog::open(&data.join("catalog.db"), "fixture UTC", &folder).unwrap();
        assert!(cat.all(None).unwrap().is_empty());
        assert!(
            cat.dispatch(json!({"operation":"seen","file_unique_id":"fuid-12345"}))
                .unwrap()
                .is_null()
        );
    }
}
