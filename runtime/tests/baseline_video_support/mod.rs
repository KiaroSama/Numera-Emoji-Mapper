#[allow(dead_code)]
#[path = "../baseline_media_support/mod.rs"]
mod common;
pub use common::{
    Scratch, UTC, age, args, child, config, file, fixture, gzip, is_child, png, project, runtime,
    save,
};
#[path = "../baseline_units_support/production_link.rs"]
pub mod production_link;
use numera_runtime as rt;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
pub const RED: [u8; 4] = [220, 20, 20, 255];
pub const BLUE: [u8; 4] = [20, 20, 220, 255];

fn frozen(manifest: &str, input: &str) -> PathBuf {
    let metadata: Value = serde_json::from_slice(
        &std::fs::read(fixture(&format!("native/{manifest}.json"))).unwrap(),
    )
    .unwrap();
    let original = metadata["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["input"] == input)
        .expect("the independent original manifest must name this input");
    let path = project().join(input);
    assert_eq!(
        rt::native_raster::digest_hex(&std::fs::read(&path).unwrap()),
        original["input_sha256"],
        "original clip bytes changed"
    );
    path
}
pub fn run_ff(root: &Path, arguments: Vec<String>) {
    let environment = std::env::vars_os().collect();
    let tool = rt::codec_process::executable("ffmpeg").unwrap();
    let result = runtime().block_on(rt::codec_process::capture_with_budget(
        root,
        &tool,
        &arguments,
        1024 * 1024,
        None,
        &environment,
        rt::codec_process::Budget {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(30),
            warning: None,
        },
    ));
    assert!(
        result.is_ok(),
        "real bounded FFmpeg fixture failed: {}",
        result.err().unwrap_or_default()
    );
}
pub fn remux(root: &Path, input: &Path, name: &str) -> PathBuf {
    let out = root.join(name);
    let mut a = args(&["-v", "error", "-y", "-i"]);
    a.push(input.to_string_lossy().into());
    a.extend(args(&["-c", "copy"]));
    a.push(out.to_string_lossy().into());
    run_ff(root, a);
    assert_ne!(std::fs::read(input).unwrap(), std::fs::read(&out).unwrap());
    out
}
pub fn encode(root: &Path, name: &str, frames: &[Vec<u8>], pts: Option<&str>) -> PathBuf {
    let source = file(&root.join(format!("{name}.rgba")), &frames.concat());
    let out = root.join(format!("{name}.webm"));
    let mut a = args(&[
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
    ]);
    a.push(source.to_string_lossy().into());
    if let Some(pts) = pts {
        a.extend([
            "-vf".into(),
            format!("settb=1/1000,setpts={pts}"),
            "-fps_mode".into(),
            "passthrough".into(),
            "-enc_time_base".into(),
            "1/1000".into(),
            "-bsf:v".into(),
            "setts=duration=33".into(),
        ]);
    }
    a.extend(args(&[
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
    ]));
    a.push(out.to_string_lossy().into());
    run_ff(root, a);
    std::fs::remove_file(source).unwrap();
    out
}
pub fn color(value: [u8; 4]) -> Vec<u8> {
    value.repeat(100 * 100)
}
pub fn encode_sequence(
    root: &Path,
    name: &str,
    colors: &[[u8; 4]],
    size: u32,
    rate: u32,
    codec: &str,
) -> PathBuf {
    let frames = root.join(format!("{name}-frames"));
    std::fs::create_dir_all(&frames).unwrap();
    for (i, &color) in colors.iter().enumerate() {
        png(&frames.join(format!("{i:04}.png")), color, (size, size));
    }
    let out = root.join(format!("{name}.webm"));
    let mut a = args(&["-y", "-v", "error", "-framerate"]);
    a.extend([
        rate.to_string(),
        "-i".into(),
        frames.join("%04d.png").to_string_lossy().into(),
    ]);
    a.extend(args(&[
        "-c:v",
        codec,
        "-pix_fmt",
        "yuva420p",
        "-auto-alt-ref",
        "0",
    ]));
    a.push(out.to_string_lossy().into());
    run_ff(root, a);
    out
}
pub fn alpha(root: &Path, name: &str, opacity: u8, codec: &str) -> PathBuf {
    let source = png(
        &root.join(format!("{name}.png")),
        [200, 30, 30, opacity],
        (100, 100),
    );
    let out = root.join(format!("{name}.webm"));
    let mut a = args(&["-y", "-v", "error", "-loop", "1", "-t", "1", "-i"]);
    a.push(source.to_string_lossy().into());
    a.extend(args(&[
        "-c:v",
        codec,
        "-pix_fmt",
        "yuva420p",
        "-auto-alt-ref",
        "0",
        "-r",
        "10",
    ]));
    a.push(out.to_string_lossy().into());
    run_ff(root, a);
    out
}
pub fn decoder(root: &Path) -> rt::codec_video::VideoDecoder {
    rt::codec_video::VideoDecoder::new(root).expect("FFmpeg and FFprobe are mandatory")
}
pub fn fp(root: &Path, path: &Path) -> Value {
    runtime()
        .block_on(decoder(root).fingerprint_typed(path))
        .unwrap()
}
pub fn key(root: &Path, path: &Path) -> String {
    fp(root, path)["content_key"].as_str().unwrap().into()
}
pub fn same(root: &Path, a: &Path, b: &Path) -> Option<bool> {
    runtime()
        .block_on(decoder(root).same_image_typed(a, b))
        .unwrap()
}
pub struct Identity {
    pub scratch: Scratch,
    pub paths: BTreeMap<String, PathBuf>,
}
impl Identity {
    pub fn new(names: &[&str]) -> Self {
        let scratch = Scratch::new();
        let mut paths = BTreeMap::new();
        for &name in names {
            let p = match name {
                "opaque" => encode_sequence(
                    &scratch.0,
                    name,
                    &[[255, 0, 0, 255]; 6],
                    64,
                    30,
                    "libvpx-vp9",
                ),
                "faint" | "noext" => encode_sequence(
                    &scratch.0,
                    name,
                    &[[255, 0, 0, 129]; 6],
                    64,
                    30,
                    "libvpx-vp9",
                ),
                "clear" => {
                    encode_sequence(&scratch.0, name, &[[255, 0, 0, 0]; 6], 64, 30, "libvpx-vp9")
                }
                "vp8_opaque" => {
                    encode_sequence(&scratch.0, name, &[[255, 0, 0, 255]; 6], 64, 30, "libvpx")
                }
                "vp8_faint" => {
                    encode_sequence(&scratch.0, name, &[[255, 0, 0, 120]; 6], 64, 30, "libvpx")
                }
                "fast30" => frozen(
                    "video-fingerprint",
                    "tests/fixtures/native/video-fingerprint/fast30.webm",
                ),
                "fast15" => frozen(
                    "video-fingerprint",
                    "tests/fixtures/native/video-fingerprint/fast15.webm",
                ),
                "single" => frozen(
                    "video-fingerprint",
                    "tests/fixtures/native/video-fingerprint/short30.webm",
                ),
                _ => panic!("unknown identity input"),
            };
            let p = if name == "noext" {
                let out = scratch.join("downloaded_blob");
                std::fs::copy(p, &out).unwrap();
                out
            } else {
                p
            };
            paths.insert(name.into(), p);
        }
        Self { scratch, paths }
    }
    pub fn get(&self, name: &str) -> &Path {
        &self.paths[name]
    }
}
pub struct Timelines {
    pub scratch: Scratch,
    pub paths: BTreeMap<String, PathBuf>,
}
impl Timelines {
    pub fn new(names: &[&str]) -> Self {
        let scratch = Scratch::new();
        let mut paths = BTreeMap::new();
        for &name in names {
            let p = match name {
                "blue" | "green" | "short" | "spiked" => frozen(
                    "video-timeline",
                    &format!("tests/fixtures/native/video-timeline/{name}.webm"),
                ),
                "blue_again" => {
                    let mut colors = vec![RED; 10];
                    colors.extend([BLUE; 20]);
                    encode_sequence(&scratch.0, name, &colors, 100, 30, "libvpx-vp9")
                }
                "fade" => encode_sequence(
                    &scratch.0,
                    name,
                    &(0..30).map(|i| [20, 20, 220, i * 8]).collect::<Vec<_>>(),
                    100,
                    30,
                    "libvpx-vp9",
                ),
                "solid" => encode_sequence(&scratch.0, name, &[BLUE; 30], 100, 30, "libvpx-vp9"),
                _ => panic!("unknown timeline input"),
            };
            paths.insert(name.into(), p);
        }
        Self { scratch, paths }
    }
    pub fn get(&self, name: &str) -> &Path {
        &self.paths[name]
    }
}
pub struct Collision {
    pub scratch: Scratch,
    pub paths: BTreeMap<String, PathBuf>,
}
impl Collision {
    pub fn new() -> Self {
        let scratch = Scratch::new();
        let mut paths = BTreeMap::new();
        paths.insert(
            "red".into(),
            encode(&scratch.0, "red", &vec![color(RED); 30], None),
        );
        paths.insert(
            "short".into(),
            encode(&scratch.0, "short", &vec![color(RED); 29], None),
        );
        for (name, index) in [("first", 0), ("middle", 15), ("last", 29)] {
            let mut frames = vec![color(RED); 30];
            frames[index] = color(BLUE);
            paths.insert(name.into(), encode(&scratch.0, name, &frames, None));
        }
        paths.insert(
            "vfr".into(),
            encode(
                &scratch.0,
                "vfr",
                &[color(RED), color(BLUE), color(RED), color(RED)],
                Some(r"if(eq(N\,0)\,0\,if(eq(N\,1)\,490\,if(eq(N\,2)\,500\,967)))"),
            ),
        );
        let remux = remux(&scratch.0, &paths["middle"], "remux.webm");
        paths.insert("remux".into(), remux);
        let pixels = (0..100)
            .flat_map(|y| (0..100).flat_map(move |x| [x * 2, y * 2, x + y, 255]))
            .collect::<Vec<u8>>();
        paths.insert(
            "pattern".into(),
            encode(&scratch.0, "pattern", &vec![pixels; 6], None),
        );
        let out = scratch.join("encoded.webm");
        let mut a = args(&["-y", "-v", "error", "-c:v", "libvpx-vp9", "-i"]);
        a.push(paths["pattern"].to_string_lossy().into());
        a.extend(args(&[
            "-c:v",
            "libvpx-vp9",
            "-threads",
            "1",
            "-crf",
            "35",
            "-b:v",
            "0",
            "-pix_fmt",
            "yuva420p",
            "-auto-alt-ref",
            "0",
        ]));
        a.push(out.to_string_lossy().into());
        run_ff(&scratch.0, a);
        paths.insert("encoded".into(), out);
        Self { scratch, paths }
    }
    pub fn get(&self, name: &str) -> &Path {
        &self.paths[name]
    }
    pub fn copy(&self, root: &Path, name: &str) -> PathBuf {
        let out = root.join(format!("{name}.webm"));
        std::fs::copy(self.get(name), &out).unwrap();
        out
    }
}
#[derive(Clone)]
pub struct Codec {
    pub root: PathBuf,
}
impl Codec {
    pub fn new(root: &Path) -> Self {
        Self { root: root.into() }
    }
}
impl rt::media_adapter::MediaCodec for Codec {
    fn with_scratch(&self, _: PathBuf) -> Self {
        self.clone()
    }
    async fn call(&self, request: Value) -> Result<Value, String> {
        let root = self.root.clone();
        tokio::task::spawn_blocking(move || {
            rt::codec_worker::execute_typed_at(Some(&root), &request).map_err(|e| e.message)
        })
        .await
        .map_err(|_| "native codec fixture task failed".to_owned())?
    }
}
pub fn catalog(root: &Path) -> rt::catalog::Catalog {
    rt::catalog::Catalog::open(&root.join("catalog.db"), UTC, root).unwrap()
}
pub fn add(
    cat: &mut rt::catalog::Catalog,
    root: &Path,
    path: &Path,
    fuid: &str,
    threshold: i32,
) -> Result<(String, bool), String> {
    let info = fp(root, path);
    let item = json!({"content_key":info["content_key"],"fmt":"video","file_path":path,"phash":info["phash"],"file_unique_id":fuid});
    let d = decoder(root);
    runtime().block_on(rt::ingest::add_verified(
        cat,
        item,
        UTC,
        threshold,
        async |a: &Path, b: &Path, _: &str| d.same_image_typed(a, b).await.map_err(|e| e.message),
        async |path: &Path, key: &str| rt::codec_format::collision_key(path, key),
    ))
}
pub fn seen(cat: &mut rt::catalog::Catalog, fuid: &str) -> Value {
    cat.dispatch(json!({"operation":"seen","file_unique_id":fuid}))
        .unwrap()
}
pub fn bases(cat: &mut rt::catalog::Catalog) -> Value {
    cat.dispatch(json!({"operation":"bases"})).unwrap()
}
pub fn valid() -> Value {
    json!({"v":"5.5","fr":30,"ip":0,"op":60,"w":512,"h":512,"layers":[]})
}
pub fn tint(node: &Value) -> Value {
    let mut doc = rt::native_lottie::Document::from_value(node).unwrap();
    doc.tint([0, 0, 255]);
    doc.to_value().unwrap()
}
pub fn gradient() -> Value {
    json!({"g":{"p":2,"k":{"a":0,"k":[0.0,1.0,0.0,0.0,1.0,0.0,0.0,1.0,0.0,1.0,1.0,0.25]}}})
}
pub fn assert_pixel(got: [u8; 4], want: [u8; 4], resampled: bool) {
    assert_eq!(got[3], want[3]);
    for channel in 0..3 {
        assert!(
            got[channel].abs_diff(want[channel]) <= if resampled { 2 } else { 0 },
            "pixel channels changed: {got:?} != {want:?}"
        );
    }
}

pub struct Fault {
    pub root: PathBuf,
    pub mode: PathBuf,
    pub count: PathBuf,
    pub ffmpeg: PathBuf,
    pub pids: PathBuf,
}
impl Fault {
    pub fn set(&self, mode: &str) {
        std::fs::write(&self.mode, mode).unwrap();
        std::fs::write(&self.count, "0").unwrap();
    }
    pub fn reset_tool(&self) {
        self.set("healthy");
        age(&self.ffmpeg, 1700000600);
    }
    pub fn count(&self) -> u32 {
        std::fs::read_to_string(&self.count)
            .unwrap()
            .parse()
            .unwrap()
    }
}
pub fn fault_case(name: &str) -> Option<Fault> {
    if is_child() {
        return Some(Fault {
            root: std::env::var_os("NUMERA_VIDEO_FIXTURE_ROOT")
                .unwrap()
                .into(),
            mode: std::env::var_os("NUMERA_VIDEO_MODE").unwrap().into(),
            count: std::env::var_os("NUMERA_VIDEO_COUNT").unwrap().into(),
            ffmpeg: std::env::var_os("NUMERA_VIDEO_WRAPPER").unwrap().into(),
            pids: std::env::var_os("NUMERA_VIDEO_PIDS").unwrap().into(),
        });
    }
    let t = Scratch::new();
    let binary = t.join(if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    });
    let compiler = env!("NUMERA_BASELINE_RUSTC");
    let (status, _, errors) = common::bounded(
        &t.0,
        Command::new(compiler)
            .args(["--edition=2024"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/baseline_video_support/codec_fixture.rs"),
            )
            .arg("-o")
            .arg(&binary),
    );
    assert!(
        status.success(),
        "compile Rust media fault fixture: {}",
        String::from_utf8_lossy(&errors)
    );
    std::fs::copy(
        &binary,
        t.join(if cfg!(windows) {
            "ffprobe.exe"
        } else {
            "ffprobe"
        }),
    )
    .unwrap();
    let mode = file(&t.join("mode"), b"healthy");
    let count = file(&t.join("count"), b"0");
    let real = rt::codec_process::executable("ffmpeg").unwrap();
    let probe = rt::codec_process::executable("ffprobe").unwrap();
    let path = std::env::join_paths(
        std::iter::once(t.0.clone())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    child(
        name,
        &t.0,
        &[
            ("PATH", path.to_string_lossy().into()),
            ("NUMERA_VIDEO_REAL_FFMPEG", real.to_string_lossy().into()),
            ("NUMERA_VIDEO_REAL_FFPROBE", probe.to_string_lossy().into()),
            ("NUMERA_VIDEO_FIXTURE_ROOT", t.0.to_string_lossy().into()),
            ("NUMERA_VIDEO_MODE", mode.to_string_lossy().into()),
            ("NUMERA_VIDEO_COUNT", count.to_string_lossy().into()),
            ("NUMERA_VIDEO_WRAPPER", binary.to_string_lossy().into()),
            ("NUMERA_VIDEO_PIDS", t.join("pids").to_string_lossy().into()),
            ("EMOJI_FFMPEG_TIMEOUT", "1".into()),
        ],
    );
    None
}

pub struct Transport {
    pub base: String,
    pub calls: Arc<std::sync::Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
pub fn sticker(name: &str) -> Value {
    json!({"file_id":name,"file_unique_id":format!("fuid-{name}"),"custom_emoji_id":name,"is_video":true,"emoji":"x"})
}
impl Transport {
    pub fn new(clips: BTreeMap<String, PathBuf>) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let calls = Arc::new(std::sync::Mutex::new(Vec::new()));
        let called = calls.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        let thread = std::thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                let (mut stream, _) = match listener.accept() {
                    Ok(v) => v,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(e) => panic!("loopback fixture: {e}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = Vec::new();
                loop {
                    let mut byte = [0];
                    stream.read_exact(&mut byte).unwrap();
                    bytes.push(byte[0]);
                    assert!(bytes.len() < 65536);
                    if bytes.ends_with(b"\r\n\r\n") {
                        break;
                    }
                }
                let header = String::from_utf8_lossy(&bytes);
                let path = header
                    .lines()
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap()
                    .to_owned();
                let length = header
                    .lines()
                    .find_map(|line| {
                        line.split_once(':')
                            .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                            .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                assert!(length < 4 * 1024 * 1024);
                let mut body = vec![0; length];
                stream.read_exact(&mut body).unwrap();
                called.lock().unwrap().push(path.clone());
                let payload = if path.contains("/file/bot") {
                    let name = path.rsplit('/').next().unwrap().trim_end_matches(".webm");
                    std::fs::read(&clips[name]).unwrap()
                } else {
                    let result = if path.ends_with("/getStickerSet") {
                        json!({"name":"fixture","stickers":clips.keys().map(|name|sticker(name)).collect::<Vec<_>>()})
                    } else if path.ends_with("/getCustomEmojiStickers") {
                        json!(clips.keys().map(|name| sticker(name)).collect::<Vec<_>>())
                    } else if path.ends_with("/getFile") {
                        let form = String::from_utf8(body).unwrap();
                        let id = form
                            .split('&')
                            .filter_map(|p| p.split_once('='))
                            .find(|(key, _)| *key == "file_id")
                            .unwrap()
                            .1;
                        let id = percent_encoding::percent_decode_str(id)
                            .decode_utf8()
                            .unwrap();
                        json!({"file_path":format!("{id}.webm")})
                    } else if path.ends_with("/createNewStickerSet")
                        || path.ends_with("/addStickerToSet")
                    {
                        json!(true)
                    } else {
                        panic!("unexpected synthetic Bot API method")
                    };
                    serde_json::to_vec(&json!({"ok":true,"result":result})).unwrap()
                };
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
                    payload.len()
                )
                .unwrap();
                stream.write_all(&payload).unwrap();
            }
        });
        Self {
            base,
            calls,
            stop,
            thread: Some(thread),
        }
    }
    pub fn telegram(&self) -> rt::telegram::Telegram {
        rt::telegram::Telegram::new("synthetic".into(), self.base.clone()).unwrap()
    }
}
impl Drop for Transport {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap();
    }
}
pub fn resolve(
    cat: &mut rt::catalog::Catalog,
    root: &Path,
    server: &Transport,
    name: &str,
) -> Result<Option<String>, String> {
    let tg = server.telegram();
    let runtime = runtime();
    let media = Codec::new(root);
    let mut p = rt::publication::Publisher {
        tg: &tg,
        cat,
        media: &media,
        data: root.into(),
        runtime: &runtime,
        config: None,
    };
    p.resolve(&sticker(name))
}
pub fn reconcile(
    cat: &mut rt::catalog::Catalog,
    root: &Path,
    server: &Transport,
    state: &mut Value,
) -> Result<(), String> {
    let tg = server.telegram();
    let runtime = runtime();
    let media = Codec::new(root);
    let mut p = rt::publication::Publisher {
        tg: &tg,
        cat,
        media: &media,
        data: root.into(),
        runtime: &runtime,
        config: None,
    };
    p.reconcile(state, "audit")
}
