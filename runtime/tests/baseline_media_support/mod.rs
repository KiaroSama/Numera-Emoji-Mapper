use image::{Rgba, RgbaImage};
use numera_runtime as rt;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};
static NEXT: AtomicU64 = AtomicU64::new(0);
pub const UTC: &str = "2026-01-01T00:00:00Z";
pub const BLUE_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><rect width="64" height="64" fill="#0000ff"/></svg>"##;
pub const RED: [u8; 4] = [240, 20, 20, 255];
pub const BLUE: [u8; 4] = [20, 20, 240, 255];
pub fn project() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .into()
}
pub fn fixture(name: &str) -> PathBuf {
    project().join("tests/fixtures").join(name)
}
pub struct Scratch(pub PathBuf);
impl Scratch {
    pub fn new() -> Self {
        let parent =
            rt::tooling_environment::confined_directory(&project(), "logs/tmp", true).unwrap();
        loop {
            let p = parent.join(format!(
                "baseline-media-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&p) {
                Ok(()) => return Self(p.canonicalize().unwrap()),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("create owned media fixture: {e}"),
            }
        }
    }
    pub fn join(&self, p: &str) -> PathBuf {
        self.0.join(p)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            std::fs::remove_dir_all(&self.0).expect("clean owned media fixture");
        }
    }
}
pub fn file(path: &Path, bytes: &[u8]) -> PathBuf {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
    path.into()
}
pub fn png(path: &Path, color: [u8; 4], size: (u32, u32)) -> PathBuf {
    save(path, &RgbaImage::from_pixel(size.0, size.1, Rgba(color)))
}
pub fn save(path: &Path, rgba: &RgbaImage) -> PathBuf {
    file(path, &rt::native_raster::encode_png(rgba).unwrap())
}
pub fn webp(path: &Path, rgba: &RgbaImage, lossless: bool, quality: f32) -> PathBuf {
    file(
        path,
        &rt::native_raster::encode_webp(
            rgba,
            rt::native_raster::WebpOptions {
                lossless,
                quality,
                method: 4,
            },
        )
        .unwrap(),
    )
}
pub fn shape(index: u32) -> RgbaImage {
    let mut image = RgbaImage::new(100, 100);
    for x in index..index + 30 {
        for y in index..index + 30 {
            image[(x, y)] = Rgba([(10 * index % 256) as u8, 60, 200, 255]);
        }
    }
    image
}
pub fn gzip(raw: &[u8], level: u32, mtime: u32) -> Vec<u8> {
    let mut encoder = flate2::GzBuilder::new()
        .mtime(mtime)
        .write(Vec::new(), flate2::Compression::new(level));
    encoder.write_all(raw).unwrap();
    encoder.finish().unwrap()
}
pub fn tgs(path: &Path, doc: &Value) -> PathBuf {
    file(path, &gzip(&serde_json::to_vec(doc).unwrap(), 6, 123))
}
pub fn lottie() -> Value {
    serde_json::from_slice(&std::fs::read(fixture("lottie/red_circle_512.json")).unwrap()).unwrap()
}
pub fn empty_lottie() -> Value {
    json!({"v":"5.5","fr":60,"ip":0,"op":60,"w":512,"h":512,"layers":[]})
}
pub fn tinted_lottie() -> Value {
    json!({"v":"5.5","fr":30,"ip":0,"op":30,"w":512,"h":512,"layers":[{"ty":4,"ks":{},"shapes":[{"ty":"gr","it":[{"ty":"fl","c":{"a":0,"k":[1.0,0.0,0.0,1.0]}},{"ty":"st","c":{"a":0,"k":[0.0,1.0,0.0,1.0]}},{"ty":"gf","g":{"p":2,"k":{"a":0,"k":[0.0,0.0,0.0,1.0,1.0,1.0,1.0,0.0]}}}]}]}]})
}
pub fn masked(mode: &str, precomp: bool) -> Value {
    let mut doc = lottie();
    let layer =
        json!({"ty":4,"nm":"masked","ip":0,"op":10,"masksProperties":[{"mode":mode,"nm":"m"}]});
    if precomp {
        doc["assets"] = json!([{"id":"comp_0","layers":[layer]}]);
        doc["layers"] = json!([{"ty":0,"nm":"precomp","refId":"comp_0","ip":0,"op":10}]);
    } else {
        doc["layers"].as_array_mut().unwrap().push(layer);
    }
    doc
}
pub fn fingerprint(path: &Path) -> Value {
    rt::codec_image::fingerprint_typed(path).unwrap()
}
pub fn key(path: &Path) -> String {
    rt::native_raster::content_key(&rt::codec_image::decoded(path).unwrap().rgba).unwrap()
}
pub fn phash(path: &Path) -> u64 {
    rt::native_raster::source_dhash(&rt::codec_image::decoded(path).unwrap()).unwrap()
}
pub fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}
pub fn decoder(root: &Path) -> rt::codec_video::VideoDecoder {
    rt::codec_video::VideoDecoder::new(root).expect("actual FFmpeg and FFprobe are mandatory")
}
pub fn bounded(root: &Path, command: &mut Command) -> (std::process::ExitStatus, Vec<u8>, Vec<u8>) {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let output = root.join(format!("child-{id}.out"));
    let errors = root.join(format!("child-{id}.err"));
    let owner = root.join(format!("child-{id}.owner"));
    std::fs::create_dir_all(root).unwrap();
    std::fs::create_dir(&owner).unwrap();
    command
        .stdin(Stdio::null())
        .stdout(std::fs::File::create(&output).unwrap())
        .stderr(std::fs::File::create(&errors).unwrap());
    let mut process = rt::process_owner::OwnedProcess::spawn_descendants(command, owner).unwrap();
    let status = runtime()
        .block_on(process.wait(Duration::from_secs(120), Duration::from_secs(120)))
        .unwrap();
    process.finish().unwrap();
    let read = |p| {
        let file = std::fs::File::open(p).unwrap();
        assert!(file.metadata().unwrap().len() <= 32 * 1024 * 1024);
        let mut bytes = Vec::new();
        file.take(32 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .unwrap();
        bytes
    };
    (status, read(output), read(errors))
}
pub fn ffmpeg(root: &Path, args: &[String]) -> Vec<u8> {
    let tool = rt::codec_process::executable("ffmpeg").expect("FFmpeg required");
    let (status, stdout, stderr) = bounded(root, Command::new(tool).args(args));
    assert!(
        status.success(),
        "actual bounded ffmpeg failed: {}",
        String::from_utf8_lossy(&stderr)
    );
    stdout
}
pub fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|v| (*v).into()).collect()
}
pub fn gif(path: &Path) -> PathBuf {
    use image::codecs::gif::{GifEncoder, Repeat};
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut encoder = GifEncoder::new(std::fs::File::create(path).unwrap());
    encoder.set_repeat(Repeat::Infinite).unwrap();
    for i in 0..6 {
        encoder
            .encode_frame(image::Frame::from_parts(
                RgbaImage::from_pixel(64, 64, Rgba([10 * i, 50, 200, 255])),
                0,
                0,
                image::Delay::from_numer_denom_ms(80, 1),
            ))
            .unwrap();
    }
    path.into()
}
pub fn video(root: &Path, source: &Path, output: &Path) {
    let d = decoder(root);
    runtime()
        .block_on(rt::codec_video_encode::convert_typed(
            &d, source, output, 262144,
        ))
        .unwrap();
}
pub fn lavfi(
    root: &Path,
    name: &str,
    size: &str,
    rate: u32,
    duration: f64,
    extra: &[&str],
) -> PathBuf {
    let out = root.join(name);
    let mut a = args(&["-v", "error", "-y", "-f", "lavfi", "-i"]);
    a.push(format!(
        "testsrc2=size={size}:rate={rate}:duration={duration}"
    ));
    a.extend(args(extra));
    a.push(out.to_string_lossy().into());
    ffmpeg(root, &a);
    out
}
pub const VP9: &[&str] = &[
    "-c:v",
    "libvpx-vp9",
    "-pix_fmt",
    "yuva420p",
    "-crf",
    "50",
    "-b:v",
    "0",
    "-an",
];
pub fn one_frame(root: &Path, name: &str) -> PathBuf {
    lavfi(
        root,
        name,
        "100x100",
        30,
        1.0,
        &[
            "-frames:v",
            "1",
            "-c:v",
            "libvpx-vp9",
            "-pix_fmt",
            "yuva420p",
            "-crf",
            "50",
            "-b:v",
            "0",
            "-an",
        ],
    )
}
pub fn clear_pixels(root: &Path, path: &Path) -> usize {
    let mut a = args(&["-v", "error", "-c:v", "libvpx-vp9", "-i"]);
    a.push(path.to_string_lossy().into());
    a.extend(args(&[
        "-frames:v",
        "1",
        "-vf",
        "format=rgba",
        "-f",
        "rawvideo",
        "-pix_fmt",
        "rgba",
        "-",
    ]));
    ffmpeg(root, &a)
        .iter()
        .skip(3)
        .step_by(4)
        .filter(|&&a| a == 0)
        .count()
}
pub fn config(root: &Path, values: &[(&str, String)]) -> rt::config::Config {
    let mut env = BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]);
    env.extend(values.iter().map(|(k, v)| ((*k).into(), v.clone())));
    rt::config::Config::load_with_environment(root.into(), &env).unwrap()
}
#[derive(Clone)]
pub struct Codec {
    pub root: PathBuf,
    pub calls: Arc<Mutex<Vec<Value>>>,
    pub marker: Option<PathBuf>,
    pub marks: Arc<Mutex<Vec<String>>>,
}
impl Codec {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.into(),
            calls: Default::default(),
            marker: None,
            marks: Default::default(),
        }
    }
}
impl rt::media_adapter::MediaCodec for Codec {
    fn with_scratch(&self, _: PathBuf) -> Self {
        self.clone()
    }
    async fn call(&self, request: Value) -> Result<Value, String> {
        self.calls.lock().unwrap().push(request.clone());
        if request["operation"] == "convert"
            && let Some(marker) = &self.marker
        {
            self.marks.lock().unwrap().push(
                std::fs::read_to_string(marker)
                    .expect("conversion must publish heartbeat before decoding"),
            );
        }
        rt::codec_worker::execute_typed_at(Some(&self.root), &request)
            .map_err(|error| error.message)
    }
}
pub struct Convert {
    pub scratch: Scratch,
    pub source: PathBuf,
    pub output: PathBuf,
    pub codec: Codec,
}
impl Convert {
    pub fn new() -> Self {
        let scratch = Scratch::new();
        let source = scratch.join("in");
        std::fs::create_dir(&source).unwrap();
        let output = scratch.join("out");
        let mut codec = Codec::new(&scratch.0);
        codec.marker = Some(output.join(".svg_cur"));
        Self {
            scratch,
            source,
            output,
            codec,
        }
    }
    pub fn raster(&self, name: &str, color: [u8; 4], size: (u32, u32)) -> PathBuf {
        png(&self.source.join(name), color, size)
    }
    pub fn run(&self) -> u8 {
        convert(&self.scratch.0, &self.source, &self.output, &self.codec)
    }
    pub fn center(&self, name: &str) -> [u8; 4] {
        rt::native_raster::open(&self.output.join(name))
            .unwrap()
            .rgba[(50, 50)]
            .0
    }
}
pub fn convert(root: &Path, source: &Path, output: &Path, codec: &Codec) -> u8 {
    let c = config(root, &[]);
    let log = rt::logging::RunLog::open(root, "baseline-media", vec![]);
    rt::convert_cli::run_with_codec(
        &c,
        &[
            "--in".into(),
            source.to_string_lossy().into(),
            "--out".into(),
            output.to_string_lossy().into(),
        ],
        &log,
        |_| codec.clone(),
    )
    .unwrap()
}
pub fn age(path: &Path, seconds: u64) {
    let time = std::time::UNIX_EPOCH + Duration::from_secs(seconds);
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .unwrap()
        .set_times(
            std::fs::FileTimes::new()
                .set_accessed(time)
                .set_modified(time),
        )
        .unwrap();
}
pub fn child(test: &str, root: &Path, extra: &[(&str, String)]) -> String {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", test, "--nocapture", "--test-threads=1"])
        .env("NUMERA_BASELINE_MEDIA_CHILD", "1");
    for (k, v) in extra {
        command.env(k, v);
    }
    let (status, stdout, stderr) = bounded(root, &mut command);
    assert!(
        status.success(),
        "isolated real test child failed:\n{}\n{}",
        String::from_utf8_lossy(&stdout),
        String::from_utf8_lossy(&stderr)
    );
    String::from_utf8(stdout).unwrap()
}
pub fn is_child() -> bool {
    std::env::var("NUMERA_BASELINE_MEDIA_CHILD").as_deref() == Ok("1")
}
pub fn observe(test: &str) -> bool {
    if is_child() {
        return true;
    }
    let t = Scratch::new();
    let compiler = option_env!("NUMERA_BASELINE_RUSTC").unwrap_or("rustc");
    let out = t.join(if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    });
    let (status, _, errors) = bounded(
        &t.0,
        Command::new(compiler)
            .args(["--edition=2024"])
            .arg(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/baseline_media_support/ffmpeg_observer.rs"),
            )
            .arg("-o")
            .arg(&out),
    );
    assert!(
        status.success(),
        "compile isolated Rust forwarder: {}",
        String::from_utf8_lossy(&errors)
    );
    let real = rt::codec_process::executable("ffmpeg").unwrap();
    let path = std::env::join_paths(
        std::iter::once(t.0.clone())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    child(
        test,
        &t.0,
        &[
            ("PATH", path.to_string_lossy().into()),
            ("NUMERA_MEDIA_REAL_FFMPEG", real.to_string_lossy().into()),
            (
                "NUMERA_MEDIA_FFMPEG_LOG",
                t.join("calls.txt").to_string_lossy().into(),
            ),
        ],
    );
    false
}
pub fn clear_calls() {
    std::fs::write(std::env::var_os("NUMERA_MEDIA_FFMPEG_LOG").unwrap(), b"").unwrap();
}
pub fn calls() -> Vec<Vec<String>> {
    std::fs::read_to_string(std::env::var_os("NUMERA_MEDIA_FFMPEG_LOG").unwrap())
        .unwrap()
        .lines()
        .map(|line| line.split('\u{1f}').map(str::to_owned).collect())
        .collect()
}
pub fn art(seed: u32) -> PathBuf {
    let manifest: Value = serde_json::from_slice(
        &std::fs::read(fixture("native/reencode-art/provenance.json")).unwrap(),
    )
    .unwrap();
    let item = manifest["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["seed"] == seed)
        .unwrap();
    let path = fixture(&format!(
        "native/reencode-art/{}",
        item["path"].as_str().unwrap()
    ));
    let bytes = std::fs::read(&path).unwrap();
    assert_eq!(rt::native_raster::digest_hex(&bytes), item["png_sha256"]);
    let pixels = rt::native_raster::decode(&bytes).unwrap();
    assert_eq!(
        rt::native_raster::digest_hex(pixels.rgba.as_raw()),
        item["rgba_sha256"]
    );
    path
}
pub fn lossy(root: &Path, source: &Path, name: &str) -> PathBuf {
    webp(
        &root.join(name),
        &rt::native_raster::open(source).unwrap().rgba,
        false,
        90.0,
    )
}

pub struct TelegramFixture {
    pub base: String,
    pub downloaded: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl TelegramFixture {
    pub fn new(count: usize, marked: &[usize]) -> Self {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let downloaded = Arc::new(Mutex::new(Vec::new()));
        let download = downloaded.clone();
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        let stickers=(0..count).map(|i|json!({"custom_emoji_id":(101+i).to_string(),"file_id":i.to_string(),"file_unique_id":format!("FU-{i}"),"emoji":"😀","needs_repainting":marked.contains(&i)})).collect::<Vec<_>>();
        let thread = std::thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                let (mut stream, _) = match listener.accept() {
                    Ok(pair) => pair,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    Err(error) => panic!("loopback fixture accept: {error}"),
                };
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut request = Vec::new();
                let header_end = loop {
                    let mut byte = [0];
                    stream.read_exact(&mut byte).unwrap();
                    request.push(byte[0]);
                    assert!(request.len() < 65536);
                    if request.ends_with(b"\r\n\r\n") {
                        break request.len();
                    }
                };
                let header = String::from_utf8_lossy(&request);
                let path = header
                    .lines()
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap()
                    .to_string();
                let length = header
                    .lines()
                    .find_map(|line| {
                        line.split_once(':')
                            .filter(|(key, _)| key.eq_ignore_ascii_case("content-length"))
                            .map(|(_, v)| v.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                assert!(length < 65536);
                request.resize(header_end + length, 0);
                stream.read_exact(&mut request[header_end..]).unwrap();
                let body = String::from_utf8_lossy(&request[header_end..]);
                let form = body
                    .split('&')
                    .filter_map(|p| p.split_once('='))
                    .map(|(k, v)| {
                        (
                            k,
                            percent_encoding::percent_decode_str(v)
                                .decode_utf8()
                                .unwrap()
                                .into_owned(),
                        )
                    })
                    .collect::<BTreeMap<_, _>>();
                let bytes = if path.contains("/file/bot") {
                    let id = path
                        .rsplit('/')
                        .next()
                        .unwrap()
                        .trim_end_matches(".png")
                        .to_string();
                    download.lock().unwrap().push(id.clone());
                    rt::native_raster::encode_png(&shape(id.parse().unwrap())).unwrap()
                } else {
                    let result = if path.ends_with("/getStickerSet") {
                        json!({"title":"pack","stickers":stickers})
                    } else if path.ends_with("/getCustomEmojiStickers") {
                        json!(stickers)
                    } else if path.ends_with("/getFile") {
                        json!({"file_path":format!("{}.png",form["file_id"])})
                    } else {
                        panic!("unrecognized synthetic API operation")
                    };
                    serde_json::to_vec(&json!({"ok":true,"result":result})).unwrap()
                };
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    bytes.len()
                )
                .unwrap();
                stream.write_all(&bytes).unwrap();
            }
        });
        Self {
            base,
            downloaded,
            stop,
            thread: Some(thread),
        }
    }
}
impl Drop for TelegramFixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.thread.take().unwrap().join().unwrap();
    }
}
pub fn fetch(
    root: &Path,
    count: usize,
    marked: &[usize],
    mode: &str,
    tint: Option<&str>,
    ids: bool,
) -> (Value, Vec<Value>, Vec<String>) {
    let server = TelegramFixture::new(count, marked);
    let tg = rt::telegram::Telegram::new("synthetic".into(), server.base.clone()).unwrap();
    let mut catalog = rt::catalog::Catalog::open(&root.join("catalog.db"), UTC, root).unwrap();
    let codec = Codec::new(root);
    let mut collector = rt::collector::Collector {
        cat: &mut catalog,
        media: &codec,
        data: root.into(),
        threshold: -1,
    };
    let runtime = runtime();
    let log = rt::logging::RunLog::open(root, "baseline-repaint", vec![]);
    let counts = if ids {
        rt::fetch_ids_cli::fetch_ids(
            &mut collector,
            &tg,
            &runtime,
            &(0..count)
                .map(|i| (101 + i).to_string())
                .collect::<Vec<_>>(),
            mode,
            tint,
            &log,
        )
        .unwrap()
    } else {
        collector
            .fetch_pack(&tg, &runtime, "pack", 0, mode)
            .unwrap()
    };
    let rows = collector.cat.all(None).unwrap();
    let downloaded = server.downloaded.lock().unwrap().clone();
    (counts, rows, downloaded)
}

/// Observe the real collector's private sticker classification at its first
/// codec request, then stop. The original classification test did not decode.
#[derive(Clone)]
pub struct FormatWitness(pub Arc<Mutex<Vec<String>>>);
impl rt::media_adapter::MediaCodec for FormatWitness {
    fn with_scratch(&self, _: PathBuf) -> Self {
        self.clone()
    }
    async fn call(&self, request: Value) -> Result<Value, String> {
        assert_eq!(request["operation"], "prepare");
        let source = Path::new(request["source"].as_str().unwrap());
        assert!(source.is_file());
        self.0
            .lock()
            .unwrap()
            .push(source.extension().unwrap().to_str().unwrap().to_owned());
        Err("controlled classification witness refusal".into())
    }
}
