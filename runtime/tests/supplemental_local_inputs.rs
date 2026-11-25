//! Producer-to-consumer input discovery must not turn converter controls into media.
#[allow(dead_code)]
#[path = "baseline_catalog_support/mod.rs"]
mod baseline_catalog_support;
// Shared CLI fixtures re-export helpers used by other cohorts.
#[allow(dead_code, unused_imports)]
#[path = "baseline_cli_status_support/mod.rs"]
mod support;
#[path = "support/test_admission.rs"]
mod test_admission;

use numera_runtime::{locks::Lease, native_raster};
use serde_json::json;
use std::path::{Path, PathBuf};
use support::{Fixture, snapshot, write};

fn image(path: &Path, seed: u8) -> PathBuf {
    let mut pixels = image::RgbaImage::new(100, 100);
    for x in 10..40 + u32::from(seed) {
        for y in 15..55 {
            pixels[(x, y)] = image::Rgba([seed * 11, 80, 190, 255]);
        }
    }
    write(path, native_raster::encode_png(&pixels).unwrap());
    path.into()
}

fn input_path(path: &Path) -> &str {
    path.to_str().unwrap()
}

fn assert_catalog_sources(fixture: &Fixture, names: &[&str]) {
    // Opening a new catalog proves that the CLI committed the observed rows.
    let rows = fixture.catalog().all(None).unwrap();
    assert_eq!(rows.len(), names.len());
    for (row, name) in rows.iter().zip(names) {
        assert_eq!(row["fmt"], "static");
        assert_eq!(row["sources"], json!([format!("local:{name}")]));
        let pixels = image::open(row["file_path"].as_str().unwrap())
            .unwrap()
            .to_rgba8();
        assert_eq!(pixels.dimensions(), (100, 100));
        assert!(pixels.pixels().any(|pixel| pixel[3] == 0));
        assert!(pixels.pixels().any(|pixel| pixel[3] == 255));
    }
}

#[test]
fn converted_directory_imports_and_deduplicates_without_decoding_its_lease() {
    let fixture = Fixture::new();
    let source = fixture.root.join("source images");
    let output = fixture.root.join("converted images");
    image(&source.join("first.png"), 1);
    image(&source.join("second.png"), 2);
    fixture
        .run(
            &[
                "make-emoji-pngs",
                "--in",
                input_path(&source),
                "--out",
                input_path(&output),
            ],
            &[],
        )
        .code(0);
    assert!(output.join(".convert.lock").is_file());
    let before = snapshot(&output);
    let args = ["add-media", "--in", input_path(&output), "--as", "static"];
    let first = fixture.run(&args, &[]);
    first.code(0);
    assert!(first.out.contains("new=2 dedup=0 failed=0"));
    assert_catalog_sources(&fixture, &["first.png", "second.png"]);
    assert_eq!(snapshot(&output), before);
    let repeated = fixture.run(&args, &[]);
    repeated.code(0);
    assert!(repeated.out.contains("new=0 dedup=2 failed=0"));
    assert_catalog_sources(&fixture, &["first.png", "second.png"]);
    assert_eq!(snapshot(&output), before);
}

#[test]
fn converter_controls_remain_unchanged_and_a_held_lease_stays_owned() {
    let fixture = Fixture::new();
    let input = fixture.root.join("input");
    image(&input.join("art.png"), 3);
    let lock = input.join(".convert.lock");
    let lease = Lease::acquire(&lock, "synthetic converter owner").unwrap();
    write(&input.join(".svg_cur"), b"interrupted-art\n");
    write(&input.join(".svg_skip.txt"), b"quarantined-art\n");
    let before = snapshot(&input);
    let run = fixture.run(&["add-media", "--in", input_path(&input)], &[]);
    run.code(0);
    assert!(run.out.contains("new=1 dedup=0 failed=0"));
    assert_catalog_sources(&fixture, &["art.png"]);
    assert_eq!(snapshot(&input), before);
    assert!(
        Lease::acquire(&lock, "second synthetic owner").is_err(),
        "input discovery must not unlink, recreate, or release a held lease"
    );
    drop(lease);
    drop(Lease::acquire(&lock, "after original owner stopped").unwrap());
}

#[test]
fn controls_only_directory_refuses_no_input_before_creating_a_catalog() {
    let fixture = Fixture::new();
    let input = fixture.root.join("only controls");
    for name in [".convert.lock", ".svg_cur", ".svg_skip.txt"] {
        write(&input.join(name), b"retained converter control\n");
    }
    let before = snapshot(&input);
    let run = fixture.run(&["add-media", "--in", input_path(&input)], &[]);
    run.code(2);
    assert!(run.err.contains("no input files"), "{}", run.err);
    assert!(!fixture.data().exists());
    assert_eq!(snapshot(&input), before);
}

#[test]
fn explicitly_named_controls_are_attempted_alone_and_after_directory_discovery() {
    for name in [".convert.lock", ".svg_cur", ".svg_skip.txt"] {
        let fixture = Fixture::new();
        let input = fixture.root.join("input");
        let control = input.join(name);
        write(&control, b"explicit invalid media\n");
        let alone = fixture.run(&["add-media", input_path(&control)], &[]);
        alone.code(4);
        assert!(alone.out.contains("new=0 dedup=0 failed=1"));
        assert!(fixture.catalog().all(None).unwrap().is_empty());
        image(&input.join("art.png"), 4);
        let before = snapshot(&input);
        let mixed = fixture.run(
            &[
                "add-media",
                "--in",
                input_path(&input),
                input_path(&control),
            ],
            &[],
        );
        mixed.code(3);
        assert!(mixed.out.contains("new=1 dedup=0 failed=1"));
        assert_catalog_sources(&fixture, &["art.png"]);
        assert_eq!(snapshot(&input), before);
    }
}

#[test]
fn unusual_media_keeps_flat_sorted_discovery_and_explicit_append_order() {
    let fixture = Fixture::new();
    let input = fixture.root.join("input");
    let names = [
        ".convert.lock.png",
        ".hidden-art",
        ".svg_cur.png",
        ".svg_skip.txt.png",
        "a-without-extension",
        "b.unusual",
        "c.lock",
        "d.PNG",
    ];
    // Reverse creation order so a filesystem enumeration is not the order oracle.
    for (index, name) in names.iter().enumerate().rev() {
        image(&input.join(name), index as u8 + 1);
    }
    image(&input.join("nested/not-discovered.png"), 10);
    let first_explicit = image(&fixture.root.join("second-explicit.png"), 11);
    let last_explicit = image(&fixture.root.join("first-explicit.png"), 12);
    #[cfg(unix)]
    std::os::unix::fs::symlink(input.join("d.PNG"), input.join("z-file-link")).unwrap();
    let run = fixture.run(
        &[
            "add-media",
            "--in",
            input_path(&input),
            input_path(&first_explicit),
            input_path(&last_explicit),
            input_path(&first_explicit),
        ],
        &[],
    );
    run.code(0);
    #[cfg(unix)]
    assert!(run.out.contains("new=10 dedup=2 failed=0"));
    #[cfg(windows)]
    assert!(run.out.contains("new=10 dedup=1 failed=0"));
    let expected = [
        ".convert.lock.png",
        ".hidden-art",
        ".svg_cur.png",
        ".svg_skip.txt.png",
        "a-without-extension",
        "b.unusual",
        "c.lock",
        "d.PNG",
        "second-explicit.png",
        "first-explicit.png",
    ];
    assert_catalog_sources(&fixture, &expected);
}

#[test]
fn unrelated_invalid_files_still_fail_without_discarding_valid_progress() {
    let fixture = Fixture::new();
    let input = fixture.root.join("input");
    image(&input.join("art.png"), 5);
    for name in [".convert.lock", ".svg_cur", ".svg_skip.txt"] {
        write(&input.join(name), b"converter control\n");
    }
    for name in [
        ".convert.lock.backup",
        ".other-hidden",
        "notes.txt",
        "user.lock",
    ] {
        write(&input.join(name), b"not an image\n");
    }
    let before = snapshot(&input);
    let run = fixture.run(&["add-media", "--in", input_path(&input)], &[]);
    run.code(3);
    assert!(run.out.contains("new=1 dedup=0 failed=4"), "{}", run.out);
    assert_catalog_sources(&fixture, &["art.png"]);
    assert_eq!(snapshot(&input), before);
}

fn apng(path: &Path) -> PathBuf {
    use base64::Engine as _;

    // Independent Pillow 13.0.0 fixture: two 100x100 RGBA frames, first
    // (232, 30, 70, 255), then (15, 90, 230, 255), with 100/200 ms delays,
    // source blending, no disposal and an infinite loop. FFprobe independently
    // identified APNG and decoded both frames. The 691-byte payload has SHA-256
    // 8507f1d0aabb10f569c0fa26d74a463221a511b2700253e03e70944c7877f028.
    // Pixel expectations below come from those input colors, not this codec.
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(concat!(
            "iVBORw0KGgoAAAANSUhEUgAAAGQAAABkCAYAAABw4pVUAAAACGFjVEwAAAACAAAAAPONk3AAAAAaZmNUTAAAAAAAAABkAAAA",
            "ZAAAAAAAAAAAAAEACgAAfsDcPgAAAQVJREFUeJzt0UERwCAAwLAxIcjBvwxcsPcU0EeioHcde67zkPHeDuDPkBhDYgyJMSTG",
            "kBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTG",
            "kBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTG",
            "kBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhDYj4LeQMT",
            "QfWUfAAAABpmY1RMAAAAAQAAAGQAAABkAAAAAAAAAAAAAQAFAADu73HXAAABCWZkQVQAAAACeJzt0UENwCAAwMAxA/jDf7DB",
            "3lNAH3cKmnTMtc9Dxns7gD9DYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhD",
            "YgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhD",
            "YgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhDYgyJMSTGkBhD",
            "YgyJMSTGkBhDYgyJMSTGkBhDYgyJ+QDf9QMWTvzUqwAAAABJRU5ErkJggg==",
        ))
        .unwrap();
    write(path, bytes);
    path.into()
}

#[test]
fn apng_extension_cases_convert_the_default_frame_and_repeat_idempotently() {
    let fixture = Fixture::new();
    let input = fixture.root.join("animated png sources");
    let output = fixture.root.join("static png output");
    apng(&input.join("lower.apng"));
    apng(&input.join("UPPER.APNG"));
    let sources = snapshot(&input);
    let args = [
        "make-emoji-pngs",
        "--in",
        input_path(&input),
        "--out",
        input_path(&output),
    ];
    let first = fixture.run(&args, &[]);
    first.code(0);
    assert!(
        first
            .out
            .contains("made 2 this run (svg=0, raster=2, failed=0)"),
        "{}",
        first.out
    );
    let expected = image::RgbaImage::from_pixel(100, 100, image::Rgba([232, 30, 70, 255]));
    let mut converted = Vec::new();
    for name in ["lower.png", "upper.png"] {
        let path = output.join(name);
        assert_eq!(image::open(&path).unwrap().to_rgba8(), expected);
        converted.push((path.clone(), std::fs::read(&path).unwrap()));
    }
    let repeated = fixture.run(&args, &[]);
    repeated.code(0);
    assert!(
        repeated
            .out
            .contains("made 0 this run (svg=0, raster=0, failed=0)"),
        "{}",
        repeated.out
    );
    for (path, bytes) in converted {
        assert_eq!(std::fs::read(path).unwrap(), bytes);
    }
    assert_eq!(snapshot(&input), sources);
}

#[test]
fn apng_is_a_fallback_after_existing_png_priority() {
    let fixture = Fixture::new();
    let input = fixture.root.join("same stem sources");
    let output = fixture.root.join("converted");
    apng(&input.join("preferred.apng"));
    // The existing helper supplies a solid 40x40 square for seed 10. Its
    // independent static expectation is a full canvas in the same opaque color.
    image(&input.join("preferred.png"), 10);
    apng(&input.join("fallback.apng"));
    write(&input.join("fallback.png"), b"invalid preferred PNG\n");
    let sources = snapshot(&input);
    let run = fixture.run(
        &[
            "make-emoji-pngs",
            "--in",
            input_path(&input),
            "--out",
            input_path(&output),
        ],
        &[],
    );
    run.code(0);
    assert!(
        run.out
            .contains("made 2 this run (svg=0, raster=2, failed=0)"),
        "{}",
        run.out
    );
    for (name, color) in [
        ("preferred.png", [110, 80, 190, 255]),
        ("fallback.png", [232, 30, 70, 255]),
    ] {
        let expected = image::RgbaImage::from_pixel(100, 100, image::Rgba(color));
        assert_eq!(image::open(output.join(name)).unwrap().to_rgba8(), expected);
    }
    assert_eq!(snapshot(&input), sources);
}
