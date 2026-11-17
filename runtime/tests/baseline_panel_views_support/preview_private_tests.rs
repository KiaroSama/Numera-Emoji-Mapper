//! Original preview cases at the native cache/renderer seam, including private path validation.
use super::*;
use crate as runtime;
use crate::panel_http::baseline_private_tests::support;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
fn clock() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}
fn previews(d: &support::Data) -> Previews {
    Previews::new(
        runtime::media_native_adapter::NativeMediaAdapter {
            executable: support::binary(),
            root: support::project(),
            scratch: d.root.join("codec"),
        },
        &d.root,
        2,
    )
}
fn tgs(path: &std::path::Path) {
    std::fs::write(path, include_bytes!("preview-motion.tgs")).unwrap();
}
fn decode(bytes: &[u8]) -> runtime::native_raster::RasterAnimation {
    runtime::native_raster::decode_animation(bytes, runtime::native_raster::DecodeLimits::default())
        .unwrap()
}
fn times(path: &std::path::Path) -> BTreeMap<std::ffi::OsString, std::time::SystemTime> {
    std::fs::read_dir(path)
        .unwrap()
        .map(|e| {
            let e = e.unwrap();
            (e.file_name(), e.metadata().unwrap().modified().unwrap())
        })
        .collect()
}
// OriginalID: test_panel_preview.PreviewResponses.test_sized_motion_and_video_posters_are_real_cached_images
#[test]
fn preview_sized_motion_and_video_posters_are_real_cached_images() {
    let d = support::Data::new(0);
    let png = d.root.join("source.png");
    support::png(&png, [240, 20, 40, 128], 100);
    let animated = d.root.join("source.tgs");
    tgs(&animated);
    let video = d.root.join("source.webm");
    std::fs::write(&video, include_bytes!("preview-poster.webm")).unwrap();
    let p = d.panel(support::options());
    *p.view.lock().unwrap() =
        json!({"view":[],"by_key":{"animated":animated,"video":video,"static":png},"hidden":0});
    let s = support::Server::start(d, p);
    let get = |path: &str| {
        let r = s.get(&format!("/preview/{path}"));
        assert_eq!(r.status(), 200);
        r.bytes().unwrap().to_vec()
    };
    let moving = get("animated?fps=10&size=72");
    let decoded = decode(&moving);
    assert_eq!(decoded.frames[0].rgba.dimensions(), (72, 72));
    assert_eq!(decoded.frames.len(), 10);
    let before = times(&s.data.root.join("preview"));
    assert_eq!(get("animated?fps=10&size=72"), moving);
    assert_eq!(times(&s.data.root.join("preview")), before);
    for kind in ["static", "video", "animated"] {
        let image = decode(&get(&format!("{kind}?still=1&size=72")));
        assert_eq!(image.frames[0].rgba.dimensions(), (72, 72));
        assert_eq!(image.frames.len(), 1);
        if kind == "video" {
            assert!(
                image.frames[0].rgba.pixels().map(|p| p.0[3]).max().unwrap() < 200,
                "video decoder silently dropped alpha"
            );
        }
    }
    assert_eq!(s.get("/preview/animated?fps=1000&size=10000").status(), 400);
}
// OriginalID: test_panel_preview.PreviewResponses.test_a_video_preview_moves_and_keeps_its_alpha
#[test]
fn preview_a_video_preview_moves_and_keeps_its_alpha() {
    let d = support::Data::new(0);
    let video = d.root.join("clip.webm");
    std::fs::write(&video, include_bytes!("preview-clip.webm")).unwrap();
    let p = previews(&d);
    let rt = clock();
    let first = rt
        .block_on(p.bytes("v:clip", &video, 10, false, 72))
        .unwrap();
    let decoded = decode(&first);
    assert_eq!(decoded.frames[0].rgba.dimensions(), (72, 72));
    assert!(decoded.frames.len() > 1);
    assert!(
        decoded.frames[0]
            .rgba
            .pixels()
            .map(|p| p.0[3])
            .max()
            .unwrap()
            < 200,
        "video decoder silently dropped alpha"
    );
    assert_eq!(
        rt.block_on(p.bytes("v:clip", &video, 10, false, 72))
            .unwrap(),
        first
    );
    assert!(
        std::fs::read_dir(d.root.join("preview")).unwrap().any(|e| e
            .unwrap()
            .file_name()
            .to_string_lossy()
            .contains("@v10-72"))
    );
}
// OriginalID: test_panel_preview.TheWarmUpRendersBeforeTheScroll.test_it_renders_grid_order_and_a_bad_file_cannot_stop_it
#[test]
fn preview_it_renders_grid_order_and_a_bad_file_cannot_stop_it() {
    let d = support::Data::new(0);
    let good = d.root.join("good.tgs");
    tgs(&good);
    let bad = d.root.join("broken.tgs");
    std::fs::write(&bad, b"not a lottie at all").unwrap();
    let view = json!({"view":[{"key":"l:logo","fmt":"static","isLogo":true,"included":true},{"key":"a:bad","fmt":"animated","included":true},{"key":"a:good","fmt":"animated","included":true}],"by_key":{"l:logo":good,"a:bad":bad,"a:good":good}});
    let p = previews(&d);
    assert_eq!(
        clock().block_on(p.warm(&view, 15)),
        json!({"rendered":4,"cached":0,"failed":4})
    );
    assert_eq!(
        std::fs::read_dir(d.root.join("preview")).unwrap().count(),
        4
    );
}
// OriginalID: test_panel_preview.TheWarmUpRendersBeforeTheScroll.test_statics_are_skipped_except_the_holding_trays_still
#[test]
fn preview_statics_are_skipped_except_the_holding_trays_still() {
    let d = support::Data::new(0);
    let png = d.root.join("s.png");
    support::png(&png, [10, 200, 90, 255], 100);
    let t = d.root.join("a.tgs");
    tgs(&t);
    let view = json!({"view":[{"key":"s:shown","fmt":"static","included":true},{"key":"a:moving","fmt":"animated","included":true},{"key":"s:held","fmt":"static","included":false}],"by_key":{"s:shown":png,"a:moving":t,"s:held":png}});
    let p = previews(&d);
    let rt = clock();
    assert_eq!(
        rt.block_on(p.warm(&view, 30)),
        json!({"rendered":5,"cached":0,"failed":0})
    );
    assert!(
        !p.destination("s:shown", &png, 15, true, 104)
            .unwrap()
            .exists()
    );
    assert!(
        p.destination("s:held", &png, 10, true, 72)
            .unwrap()
            .exists()
    );
    assert_eq!(
        rt.block_on(p.warm(&view, 30)),
        json!({"rendered":0,"cached":5,"failed":0})
    );
}
struct Record(Arc<Mutex<Vec<Value>>>);
impl runtime::media_adapter::MediaCodec for Record {
    fn with_scratch(&self, _: std::path::PathBuf) -> Self {
        Self(self.0.clone())
    }
    async fn call(&self, request: Value) -> Result<Value, String> {
        self.0.lock().unwrap().push(request.clone());
        let rgba = image::RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 0]));
        let bytes = runtime::native_raster::encode_webp(&rgba, Default::default()).unwrap();
        std::fs::write(request["output"].as_str().unwrap(), bytes).unwrap();
        Ok(Value::Null)
    }
}
// OriginalID: test_panel_preview.TheWarmUpRendersBeforeTheScroll.test_the_tiers_follow_the_frame_rate_cap
#[test]
fn preview_the_tiers_follow_the_frame_rate_cap() {
    for cap in [30, 12] {
        let d = support::Data::new(0);
        let source = d.root.join("a.tgs");
        tgs(&source);
        let requests = Arc::new(Mutex::new(Vec::new()));
        let p = Previews::new(Record(requests.clone()), &d.root, 1);
        let view = json!({"view":[{"key":"a:moving","fmt":"animated","included":true}],"by_key":{"a:moving":source}});
        assert_eq!(clock().block_on(p.warm(&view, cap))["rendered"], 4);
        let got = requests
            .lock()
            .unwrap()
            .iter()
            .map(|x| (x["size"].as_u64().unwrap(), x["fps"].as_u64().unwrap()))
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            got,
            std::collections::BTreeSet::from([(104, cap.min(15)), (72, 10)])
        );
        // The page and warmer must advertise the same original tier contract.
        let mut options = support::options();
        options["preview_fps"] = json!(cap);
        let panel = d.panel(options);
        let server = support::Server::start(d, panel);
        let page = server.page();
        let raw = page
            .split("const PREVIEW_TIERS = ")
            .nth(1)
            .unwrap()
            .split(';')
            .next()
            .unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(raw).unwrap(),
            json!({"full":{"size":104,"fps":cap.min(15)},"compact":{"size":72,"fps":10}})
        );
    }
}
// OriginalID: test_panel_preview.TheWarmUpRendersBeforeTheScroll.test_a_key_with_a_path_separator_is_refused
#[test]
fn preview_a_key_with_a_path_separator_is_refused() {
    let d = support::Data::new(0);
    let p = previews(&d);
    assert!(
        p.destination("a:../x", std::path::Path::new("x.tgs"), 15, false, 104)
            .is_err()
    );
}
// OriginalID: test_panel_preview.TheWarmUpRendersBeforeTheScroll.test_a_set_stop_event_ends_it_without_finishing_the_catalog
#[test]
fn preview_a_set_stop_event_ends_it_without_finishing_the_catalog() {
    let d = support::Data::new(0);
    let source = d.root.join("a.tgs");
    tgs(&source);
    let mut paths = serde_json::Map::new();
    let cards = (0..50)
        .map(|i| {
            paths.insert(format!("a:k{i}"), json!(source));
            json!({"key":format!("a:k{i}"),"fmt":"animated"})
        })
        .collect::<Vec<_>>();
    let view = json!({"view":cards,"by_key":paths});
    let p = Arc::new(previews(&d));
    let rt = clock();
    // Rust warming is an async task owned by the server's JoinSet. A pre-set stop
    // corresponds to aborting that task before its first poll; no renderer may run.
    let task = rt.spawn(async move { p.warm(&view, 15).await });
    task.abort();
    assert!(rt.block_on(task).unwrap_err().is_cancelled());
    assert!(!d.root.join("preview").exists());
    assert!(!d.root.join("codec").exists());
}
