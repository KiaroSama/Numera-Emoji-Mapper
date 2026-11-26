use super::*;
use numera_runtime::{panel_session, process_owner::OwnedProcess};
use std::{
    net::TcpListener,
    process::{Command, Stdio},
    time::{Instant, SystemTime},
};

#[test]
fn video_report_and_cold_panel_preview_use_the_external_catalog() {
    let app = Installation::new();
    let video = frozen(VIDEO);
    let source = app.seed(
        VIDEO_KEY,
        "video",
        "video é.webm",
        &video,
        6510615555426900570,
    );
    let report = success(app.run(&["identity-repair", "report"], &app.data));
    assert!(
        report.contains("VIDEO IDENTITY: 1 row(s) checked"),
        "{report}"
    );
    assert!(report.contains("already correct        : 1"), "{report}");
    assert!(report.contains("undecodable            : 0"), "{report}");
    empty(&app.base().join("identity-codec"));

    let session = panel_session::identity(&app.data.join("catalog.db"), false, &[]).unwrap();
    let port = TcpListener::bind(("127.0.0.1", 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    let url = format!("http://127.0.0.1:{port}");
    let mut cmd = Command::new(&app.binary);
    cmd.args([
        "panel",
        "--no-open",
        "--port",
        &port.to_string(),
        "--data-dir",
    ])
    .arg(&app.data)
    .current_dir(&app.project)
    .env_clear()
    .envs(&app.env)
    .stdin(Stdio::null())
    .stdout(Stdio::null())
    .stderr(Stdio::null());
    let owner = tooling_process::workspace(&app.outer.0).unwrap();
    let mut panel = OwnedProcess::spawn_session(&mut cmd, owner).unwrap();
    let client = reqwest::blocking::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    let end = Instant::now() + Duration::from_secs(25);
    loop {
        if let Ok(res) = client.get(format!("{url}/api/session")).send() {
            assert_eq!(res.status(), 200);
            assert_eq!(res.json::<Value>().unwrap(), session);
            let image = client.get(format!("{url}/img/{VIDEO_KEY}")).send().unwrap();
            if image.status() == 200 {
                assert_eq!(&image.bytes().unwrap()[..], &video);
                break;
            }
        }
        assert!(Instant::now() < end, "panel did not expose the owned video");
        std::thread::sleep(Duration::from_millis(50));
    }
    let digest = numera_runtime::native_raster::digest_hex(VIDEO_KEY.as_bytes());
    let cache = app
        .data
        .join("preview")
        .join(format!("{digest}@still-52.webp"));
    assert!(!cache.exists());
    let preview = || {
        let res = client
            .get(format!("{url}/preview/{VIDEO_KEY}?still=1&size=52"))
            .send()
            .unwrap();
        assert_eq!(res.status(), 200);
        assert_eq!(res.headers()["content-type"], "image/webp");
        res.bytes().unwrap()
    };
    let cold = preview();
    assert_eq!(fs::read(&cache).unwrap(), cold);
    assert_eq!(
        image::guess_format(&cold).unwrap(),
        image::ImageFormat::WebP
    );
    let pixels = image::load_from_memory(&cold).unwrap().to_rgba8();
    assert_eq!((pixels.width(), pixels.height()), (52, 52));
    assert!(pixels.pixels().any(|pixel| pixel[3] > 0));
    let old = SystemTime::UNIX_EPOCH + Duration::from_secs(946684800);
    fs::File::options()
        .write(true)
        .open(&cache)
        .unwrap()
        .set_modified(old)
        .unwrap();
    let observed = fs::metadata(&cache).unwrap().modified().unwrap();
    assert_eq!(preview(), cold);
    assert_eq!(fs::metadata(&cache).unwrap().modified().unwrap(), observed);
    // The panel also warms the two initial viewport tiers. Let their real workers settle.
    let end = Instant::now() + Duration::from_secs(25);
    for tier in [72, 104] {
        let warm = app
            .data
            .join("preview")
            .join(format!("{digest}@still-{tier}.webp"));
        while !warm.is_file() {
            assert!(Instant::now() < end, "panel startup warming did not settle");
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    panel.finish().unwrap();
    empty(&app.base().join("native-codec"));
    assert_eq!(fs::read(source).unwrap(), video);
    assert!(!app.data.join("tmp/native-codec").exists());
}
