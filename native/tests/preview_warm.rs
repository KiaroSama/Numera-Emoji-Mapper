//! Original real preview and warm-up contracts through the actual same-binary codec/cache boundary.
use _native::{codec_process, media_native_adapter::NativeMediaAdapter, panel_preview::Previews};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[path = "support/bounded_media.rs"]
mod bounded_media;
#[path = "support/tooling_fixture.rs"]
mod tooling_fixture;

fn metadata(bytes: &[u8]) -> (u32, u32, usize, u8) {
    Python::attach(|py| -> PyResult<_> {
        let stream = py
            .import("io")?
            .call_method1("BytesIO", (PyBytes::new(py, bytes),))?;
        let result = (|| -> PyResult<_> {
            let image = py.import("PIL.Image")?.call_method1("open", (&stream,))?;
            let result = (|| -> PyResult<_> {
                let size: (u32, u32) = image.getattr("size")?.extract()?;
                let frames: usize = image.getattr("n_frames")?.extract()?;
                let rgba = image.call_method1("convert", ("RGBA",))?;
                let alpha = (|| -> PyResult<u8> {
                    rgba.call_method0("getextrema")?
                        .get_item(3)?
                        .get_item(1)?
                        .extract()
                })();
                let closed = rgba.call_method0("close");
                let alpha = alpha?;
                closed?;
                Ok((size.0, size.1, frames, alpha))
            })();
            let closed = image.call_method0("close");
            let answer = result?;
            closed?;
            Ok(answer)
        })();
        let closed = stream.call_method0("close");
        let answer = result?;
        closed?;
        Ok(answer)
    })
    .unwrap()
}
fn cache(directory: &Path) -> BTreeMap<PathBuf, (Vec<u8>, std::time::SystemTime)> {
    std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            assert_eq!(path.extension().and_then(|v| v.to_str()), Some("webp"));
            let bytes = std::fs::read(&path).unwrap();
            let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
            (path, (bytes, modified))
        })
        .collect()
}
fn previews(
    project: &Path,
    data: &Path,
    deadline: Instant,
) -> Previews<bounded_media::BoundedCodec> {
    Previews::new(
        bounded_media::BoundedCodec {
            native: NativeMediaAdapter {
                executable: PathBuf::from(env!("CARGO_BIN_EXE_numera-emoji")),
                root: project.into(),
                scratch: data.join("envelopes"),
            },
            deadline,
        },
        data,
        1,
    )
}

#[test]
fn original_real_previews_preserve_motion_alpha_cache_and_warmup_decisions() {
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let root = tooling_fixture::Root::new("original panel previews");
    let deadline = Instant::now() + Duration::from_secs(60);
    let png = root.0.join("source.png");
    let tgs = root.0.join("source.tgs");
    let broken = root.0.join("broken.tgs");
    std::fs::write(&broken, b"not a lottie at all").unwrap();
    let lottie = json!({"v":"5.7.4","w":512,"h":512,"fr":30,"ip":0,"op":30,
        "layers":[{"ty":1,"ind":1,"sw":80,"sh":80,"sc":"#ff0000","ip":0,"op":30,"st":0,"ks":{
        "o":{"a":0,"k":100},"r":{"a":0,"k":0},"a":{"a":0,"k":[0,0,0]},"s":{"a":0,"k":[100,100,100]},
        "p":{"a":1,"k":[{"t":0,"s":[80,200,0],"e":[400,200,0],"i":{"x":1,"y":1},"o":{"x":0,"y":0}},
        {"t":30,"s":[400,200,0]}]}}}]});
    Python::initialize();
    Python::attach(|py| -> PyResult<()> {
        let images = py.import("PIL.Image")?;
        assert_eq!(
            images.getattr("__version__")?.extract::<String>()?,
            "12.3.0"
        );
        for (path, color) in
            std::iter::once((png.clone(), (240, 20, 40, 128))).chain((0..10).map(|i| {
                (
                    root.0.join(format!("f{i:02}.png")),
                    (25 * i, 200 - 15 * i, 90, 128),
                )
            }))
        {
            let image = images.call_method1("new", ("RGBA", (100, 100), color))?;
            let saved = image.call_method1("save", (&path, "PNG"));
            let closed = image.call_method0("close");
            saved?;
            closed?;
        }
        let document = py
            .import("json")?
            .call_method1("loads", (lottie.to_string(),))?;
        let encoded = py
            .import("json")?
            .call_method1("dumps", (document,))?
            .call_method1("encode", ("utf-8",))?;
        let options = PyDict::new(py);
        options.set_item("mtime", 0)?;
        let bytes: Vec<u8> = py
            .import("gzip")?
            .call_method("compress", (encoded,), Some(&options))?
            .extract()?;
        std::fs::write(&tgs, bytes)?;
        Ok(())
    })
    .unwrap();
    let mut originals = std::fs::read_dir(&root.0)
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            (path.clone(), std::fs::read(path).unwrap())
        })
        .collect::<BTreeMap<_, _>>();
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async {
        let ffmpeg=codec_process::executable("ffmpeg").unwrap();
        let environment=std::env::vars_os().collect();
        for (name,input) in [
            ("source",vec!["-loop".into(),"1".into(),"-i".into(),png.to_string_lossy().into_owned(),"-t".into(),"0.1".into()]),
            ("clip",vec!["-framerate".into(),"10".into(),"-i".into(),root.0.join("f%02d.png").to_string_lossy().into_owned()]),
        ] {
            let output=root.0.join(format!("{name}.webm"));
            let mut args=vec!["-nostdin".into(),"-v".into(),"error".into(),"-y".into()];
            args.extend(input);
            args.extend(["-c:v".into(),"libvpx-vp9".into(),"-pix_fmt".into(),"yuva420p".into(),
                "-threads".into(),"1".into(),"-auto-alt-ref".into(),"0".into(),output.to_string_lossy().into_owned()]);
            // CPU libvpx is the original alpha-capable recipe, not a GPU performance comparison.
            codec_process::capture_with_budget(&root.0,&ffmpeg,&args,1024*1024,None,&environment,
                codec_process::Budget {wall:Duration::from_secs(5),idle:Duration::from_secs(3),warning:None}).await.unwrap();
            originals.insert(output.clone(),std::fs::read(output).unwrap());
        }
        let data=root.0.join("responses");
        std::fs::create_dir(&data).unwrap();
        let preview=previews(project,&data,deadline);
        let moving=preview.bytes("animated",&tgs,10,false,72).await.unwrap();
        let info=metadata(&moving);
        assert_eq!((info.0,info.1,info.2),(72,72,10));
        let before=cache(&data.join("preview"));
        assert_eq!(preview.bytes("animated",&tgs,10,false,72).await.unwrap(),moving);
        assert_eq!(cache(&data.join("preview")),before);
        let video=root.0.join("source.webm");
        for (key,source) in [("static",&png),("video",&video),("animated",&tgs)] {
            let bytes=preview.bytes(key,source,15,true,72).await.unwrap();
            let info=metadata(&bytes);
            assert_eq!((info.0,info.1,info.2),(72,72,1));
            if key=="video" {assert!(info.3<200,"video poster lost alpha");}
        }
        let clip=root.0.join("clip.webm");
        let animated=preview.bytes("v:clip",&clip,10,false,72).await.unwrap();
        let info=metadata(&animated);
        assert_eq!((info.0,info.1),(72,72));
        assert!(info.2>1 && info.3<200,"moving video preview lost motion or alpha");
        let before=cache(&data.join("preview"));
        assert_eq!(preview.bytes("v:clip",&clip,10,false,72).await.unwrap(),animated);
        assert_eq!(cache(&data.join("preview")),before);
        assert!(before.keys().any(|path|path.file_name().unwrap().to_str().unwrap().contains("@v10-72")));
        assert!(preview.bytes("a:../x",&tgs,15,false,104).await.is_err());
        assert!(preview.bytes("animated",&tgs,1000,false,10000).await.is_err());

        let bad_data=root.0.join("warm-bad");
        std::fs::create_dir(&bad_data).unwrap();
        let warm=previews(project,&bad_data,deadline);
        let view=json!({"view":[{"key":"l:logo","fmt":"static","isLogo":true,"included":true},
            {"key":"a:bad","fmt":"animated","included":true},{"key":"a:good","fmt":"animated","included":true}],
            "by_key":{"l:logo":tgs,"a:bad":broken,"a:good":tgs}});
        assert_eq!(warm.warm(&view,15).await,json!({"rendered":4,"cached":0,"failed":4}));
        assert_eq!(cache(&bad_data.join("preview")).len(),4);

        let held_data=root.0.join("warm-held");
        std::fs::create_dir(&held_data).unwrap();
        let warm=previews(project,&held_data,deadline);
        let (stop, stopped)=tokio::sync::oneshot::channel();
        stop.send(()).unwrap();
        let unstarted=json!({"view":[{"key":"a:stopped","fmt":"animated","included":true}],"by_key":{"a:stopped":tgs}});
        tokio::select! {
            biased;
            signal=stopped => {signal.unwrap();}
            _=warm.warm(&unstarted,15) => panic!("warm-up ran after pre-start cancellation"),
        }
        assert!(!held_data.join("preview").exists());
        assert!(!held_data.join("envelopes").exists());
        let view=json!({"view":[{"key":"s:shown","fmt":"static","included":true},
            {"key":"a:moving","fmt":"animated","included":true},{"key":"s:held","fmt":"static","included":false}],
            "by_key":{"s:shown":png,"a:moving":tgs,"s:held":png}});
        assert_eq!(warm.warm(&view,30).await,json!({"rendered":5,"cached":0,"failed":0}));
        let before=cache(&held_data.join("preview"));
        assert_eq!(before.len(),5);
        let shown=Sha256::digest(b"s:shown").iter().map(|byte|format!("{byte:02x}")).collect::<String>();
        assert!(before.keys().all(|path|!path.file_name().unwrap().to_str().unwrap().starts_with(&shown)));
        assert_eq!(metadata(&warm.bytes("s:held",&png,10,true,72).await.unwrap()).2,1);
        assert_eq!(warm.warm(&view,30).await,json!({"rendered":0,"cached":5,"failed":0}));
        assert_eq!(cache(&held_data.join("preview")),before);
        assert_eq!(warm.warm(&view,12).await,json!({"rendered":1,"cached":4,"failed":0}));
        let capped=cache(&held_data.join("preview"));
        let full=capped.iter().find(|(path,_)|path.file_name().unwrap().to_str().unwrap().ends_with("@12-104.webp")).unwrap();
        let dimensions=metadata(&full.1.0);
        assert_eq!((dimensions.0,dimensions.1,dimensions.2),(104,104,12));
        assert!(capped.keys().any(|path|path.file_name().unwrap().to_str().unwrap().ends_with("@10-72.webp")));
        let missing=root.0.join("source-held.png");
        std::fs::rename(&png,&missing).unwrap();
        assert_eq!(metadata(&warm.bytes("s:held",&png,10,true,72).await.unwrap()).2,1);
        assert_eq!(cache(&held_data.join("preview")),capped);
        std::fs::rename(&missing,&png).unwrap();
        for directory in [&data,&bad_data,&held_data] {
            assert!(std::fs::read_dir(directory.join("envelopes")).unwrap().next().is_none());
            assert!(!_native::process_owner::cleanup_pending(directory));
        }
    });
    drop(runtime);
    for (path, bytes) in originals {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    assert!(Instant::now() < deadline);
    assert!(!_native::process_owner::cleanup_pending(&root.0));
    root.clean().unwrap();
}
