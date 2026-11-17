use crate::{identity::frozen_art, support::*};
use numera_runtime as native;
use std::{
    path::Path,
    time::{Duration, SystemTime},
};
fn convert(f: &Fixture, args: &[&str]) -> Run {
    let mut command = vec!["make-emoji-pngs"];
    command.extend_from_slice(args);
    f.run(&command, &[])
}
fn clean(f: &Fixture, out: &Path) {
    let ipc = f.root.join("logs/tmp/native-convert");
    if ipc.exists() {
        assert_eq!(
            std::fs::read_dir(ipc).unwrap().count(),
            0,
            "native conversion staging must be reaped"
        );
    }
    assert!(!out.join(".native-codec").exists());
}
fn assert_png(path: &Path) {
    let bytes = std::fs::read(path).unwrap();
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    let image = image::load_from_memory(&bytes).unwrap();
    assert_eq!(image.color(), image::ColorType::Rgba8);
    assert_eq!((image.width(), image.height()), (100, 100));
    assert_eq!(
        image.to_rgba8(),
        native::codec_image::decoded(&frozen_art("numera-emoji-mapper-logo.png"))
            .unwrap()
            .rgba
    );
}
// tests.test_native_convert_cli.NativeConvertCLI.test_limit_and_failed_replacement_preserve_existing_output
#[test]
fn conversion_limit_and_failed_replacement_preserve_existing_outputs() {
    let f = Fixture::new();
    let source = f.root.join("input/art.png");
    let input = source.parent().unwrap();
    let out = f.root.join("output");
    let original = std::fs::read(project().join("assets/numera-emoji-mapper-logo.png")).unwrap();
    write(&source, &original);
    write(&input.join("other.png"), &original);
    let run = |args: &[&str]| {
        let mut options = vec![
            "--in",
            input.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ];
        options.extend_from_slice(args);
        convert(&f, &options)
    };
    run(&["--limit", "-1"]).code(2);
    assert!(!out.exists());
    run(&["--limit", "1"]).code(0);
    assert_eq!(
        std::fs::read_dir(&out)
            .unwrap()
            .filter_map(|e| {
                let p = e.unwrap().path();
                (p.extension().is_some_and(|s| s == "png"))
                    .then(|| p.file_name().unwrap().to_string_lossy().into_owned())
            })
            .collect::<Vec<_>>(),
        ["art.png"]
    );
    assert_eq!(std::fs::read(&source).unwrap(), original);
    let prior = std::fs::read(out.join("art.png")).unwrap();
    write(&out.join("other.png/keep.bin"), b"previous durable output");
    run(&[]).code(2);
    assert_eq!(
        std::fs::read(out.join("other.png/keep.bin")).unwrap(),
        b"previous durable output"
    );
    assert_eq!(std::fs::read(out.join("art.png")).unwrap(), prior);
    assert_eq!(std::fs::read(&source).unwrap(), original);
    assert_eq!(std::fs::read(input.join("other.png")).unwrap(), original);
    clean(&f, &out);
}
// tests.test_native_convert_cli.NativeConvertCLI.test_failed_same_file_replacement_keeps_previous_png_and_source
#[test]
#[cfg_attr(
    not(windows),
    ignore = "Windows denies replacement of an opened file without delete sharing"
)]
fn windows_open_target_refuses_same_file_replacement_and_keeps_both_inputs() {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let f = Fixture::new();
        let source = f.root.join("source/art.png");
        write(
            &source,
            std::fs::read(project().join("assets/numera-emoji-mapper-logo.png")).unwrap(),
        );
        let target = f.root.join("output/art.png");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        let expected = crate::identity::art(&f, "numera-emoji-mapper-logo.png", "expected.png");
        image::open(expected)
            .unwrap()
            .to_rgb8()
            .save(&target)
            .unwrap();
        let before = (
            std::fs::read(&target).unwrap(),
            std::fs::read(&source).unwrap(),
        );
        let held = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .open(&target)
            .unwrap();
        let r = convert(
            &f,
            &[
                "--in",
                source.parent().unwrap().to_str().unwrap(),
                "--out",
                target.parent().unwrap().to_str().unwrap(),
            ],
        );
        r.code(2);
        assert_eq!(
            (
                std::fs::read(&target).unwrap(),
                std::fs::read(&source).unwrap()
            ),
            before
        );
        clean(&f, target.parent().unwrap());
        drop(held);
    }
}
// tests.test_native_convert_cli.NativeConvertCLI.test_competing_output_owner_refuses_before_touching_markers_or_png
#[test]
fn competing_conversion_owner_refuses_before_png_or_marker_writes() {
    let f = Fixture::new();
    let source = f.root.join("source/art.png");
    write(
        &source,
        std::fs::read(project().join("assets/numera-emoji-mapper-logo.png")).unwrap(),
    );
    let out = f.root.join("output");
    write(&out.join("art.png"), b"previous output awaiting repair");
    write(&out.join(".svg_cur"), b"previous attempt");
    let before = snapshot(&f.root);
    let held = native::locks::Lease::acquire(&out.join(".convert.lock"), UTC).unwrap();
    let r = convert(
        &f,
        &[
            "--in",
            source.parent().unwrap().to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ],
    );
    r.code(2);
    assert!(r.err.contains("conversion output is already owned"));
    for name in ["output/art.png", "output/.svg_cur", "source/art.png"] {
        assert_eq!(
            std::fs::read(f.root.join(name)).unwrap(),
            before[Path::new(name)]
        );
    }
    assert!(!out.join(".svg_skip.txt").exists());
    assert!(!out.join(".native-codec").exists());
    drop(held);
}
// tests.test_native_convert_cli.NativeConvertCLI.test_rgb_output_is_rebuilt_as_exact_rgba_then_reused
#[test]
fn rgb_output_rebuilt_as_exact_rgba_then_reused_and_legacy_fallbacks_work() {
    let f = Fixture::new();
    let source = f.root.join("input/art.png");
    write(
        &source,
        std::fs::read(project().join("assets/numera-emoji-mapper-logo.png")).unwrap(),
    );
    let output = f.root.join("output");
    std::fs::create_dir(&output).unwrap();
    image::open(frozen_art("numera-emoji-mapper-logo.png"))
        .unwrap()
        .to_rgb8()
        .save(output.join("art.png"))
        .unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .open(&source)
        .unwrap()
        .set_times(
            std::fs::FileTimes::new()
                .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1700000000)),
        )
        .unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .open(output.join("art.png"))
        .unwrap()
        .set_times(
            std::fs::FileTimes::new()
                .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(1700000060)),
        )
        .unwrap();
    let args = [
        "--in",
        source.parent().unwrap().to_str().unwrap(),
        "--out",
        output.to_str().unwrap(),
    ];
    let r = convert(&f, &args);
    r.code(0);
    assert_eq!(
        r.out,
        format!(
            "DONE: made 1 this run (svg=0, raster=1, failed=0); total emoji PNGs in {}: 1.\n",
            output.display()
        )
    );
    let target = output.join("art.png");
    assert_png(&target);
    let converted = std::fs::read(&target).unwrap();
    let stdout = f.root.join("owned-output");
    let stderr = f.root.join("owned-errors");
    f.run(
        &[
            "native-convert-owner",
            "--in",
            source.parent().unwrap().to_str().unwrap(),
            "--out",
            output.to_str().unwrap(),
            "--stdout",
            stdout.to_str().unwrap(),
            "--stderr",
            stderr.to_str().unwrap(),
        ],
        &[],
    )
    .code(0);
    assert!(std::fs::read_to_string(stdout).unwrap().contains("DONE:"));
    assert_png(&target);
    assert_eq!(std::fs::read(&target).unwrap(), converted);
    let before = (
        std::fs::read(&target).unwrap(),
        target.metadata().unwrap().modified().unwrap(),
    );
    convert(&f, &args).code(0);
    assert_eq!(
        (
            std::fs::read(&target).unwrap(),
            target.metadata().unwrap().modified().unwrap()
        ),
        before
    );
    assert!(!output.join(".svg_cur").exists());
    clean(&f, &output);
    let logos = f.root.join("coins/logos");
    write(&logos.join("svg/art.svg"), b"<svg broken");
    write(&logos.join("png/art.png"), std::fs::read(&source).unwrap());
    write(&logos.join(".svg_cur"), b"quarantined");
    write(&logos.join("svg/quarantined.svg"), b"<svg broken");
    write(
        &logos.join("png/quarantined.png"),
        std::fs::read(&source).unwrap(),
    );
    convert(&f, &["--limit", "0"]).code(0);
    for stem in ["art", "quarantined"] {
        assert_png(&logos.join(format!("emoji/{stem}.png")));
    }
    let bytes = ["art", "quarantined"].map(|stem| {
        (
            stem,
            std::fs::read(logos.join(format!("emoji/{stem}.png"))).unwrap(),
        )
    });
    assert!(!logos.join(".svg_cur").exists());
    assert!(
        std::fs::read_to_string(logos.join(".svg_skip.txt"))
            .unwrap()
            .contains("quarantined")
    );
    assert!(!logos.join("emoji/.svg_skip.txt").exists());
    std::fs::remove_dir_all(logos.join("svg")).unwrap();
    convert(&f, &["--limit", "0"]).code(0);
    for (stem, expected) in bytes {
        assert_eq!(
            std::fs::read(logos.join(format!("emoji/{stem}.png"))).unwrap(),
            expected,
            "fallback repeat must reuse the generated PNG"
        );
    }
    std::fs::remove_dir_all(&logos).unwrap();
    let r = convert(&f, &["--limit", "0"]);
    r.code(0);
    assert!(r.out.contains("made 0 this run"));
    assert!(logos.join("emoji").is_dir());
    #[cfg(windows)]
    {
        write(&logos.join("svg/vector.svg"),br#"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100"><rect width="100" height="100" fill="red"/></svg>"#);
        write(&logos.join("png/art.png"), std::fs::read(&source).unwrap());
        let r = f.run(&["coin-convert"], &[]);
        r.code(0);
        assert!(r.out.contains("SVG conversion complete"));
        assert_png(&logos.join("emoji/art.png"));
        assert!(logos.join("emoji/vector.png").is_file());
        assert!(!logos.join("emoji/.svg_cur").exists());
        clean(&f, &logos.join("emoji"));
    }
}
