use super::*;
mod test_blank_guards {
    use super::*;
    // tests.test_make_emoji_pngs.TestBlankGuards.test_is_blank_transparent
    #[test]
    fn test_is_blank_transparent() {
        assert!(raster::is_blank(&RgbaImage::new(100, 100)));
    }
    // tests.test_make_emoji_pngs.TestBlankGuards.test_is_blank_opaque
    #[test]
    fn test_is_blank_opaque() {
        assert!(!raster::is_blank(&RgbaImage::from_pixel(
            100,
            100,
            Rgba([10, 120, 200, 255])
        )));
    }
    // tests.test_make_emoji_pngs.TestBlankGuards.test_convert_raster_blank_returns_false
    #[test]
    fn test_convert_raster_blank_returns_false() {
        let t = Scratch::new();
        let source = png(&t.join("blank.png"), [0, 0, 0, 0], (50, 50));
        let out = t.join("out.png");
        assert!(rt::codec_image::to_static_png(&source, &out).is_err());
        assert!(!out.exists());
    }
    // tests.test_make_emoji_pngs.TestBlankGuards.test_convert_raster_solid_returns_true_and_100
    #[test]
    fn test_convert_raster_solid_returns_true_and_100() {
        let t = Scratch::new();
        let source = png(&t.join("solid.png"), [200, 30, 30, 255], (40, 70));
        let out = t.join("out.png");
        rt::codec_image::to_static_png(&source, &out).unwrap();
        assert_eq!(raster::open(&out).unwrap().rgba.dimensions(), (100, 100));
    }
}
mod test_general_run {
    use super::*;
    // tests.test_make_emoji_pngs.TestGeneralRun.test_valid_output_is_reused
    #[test]
    fn test_valid_output_is_reused() {
        let c = Convert::new();
        let source = c.raster("a.png", RED, (40, 70));
        assert_eq!(c.run(), 0);
        let out = c.output.join("a.png");
        age(&out, 1700000000);
        age(&source, 1699999940);
        assert_eq!(c.run(), 0);
        assert_eq!(
            std::fs::metadata(out)
                .unwrap()
                .modified()
                .unwrap()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
            1700000000
        );
    }
    // tests.test_make_emoji_pngs.TestGeneralRun.test_newer_source_is_reconverted
    #[test]
    fn test_newer_source_is_reconverted() {
        let c = Convert::new();
        let source = c.raster("a.png", RED, (40, 70));
        assert_eq!(c.run(), 0);
        png(&source, BLUE, (40, 70));
        age(&c.output.join("a.png"), 1700000000);
        age(&source, 1700000060);
        assert_eq!(c.run(), 0);
        let center = c.center("a.png");
        assert!(center[2] > center[0]);
    }
    // tests.test_make_emoji_pngs.TestGeneralRun.test_corrupt_output_is_rebuilt
    #[test]
    fn test_corrupt_output_is_rebuilt() {
        let c = Convert::new();
        c.raster("a.png", RED, (40, 70));
        file(&c.output.join("a.png"), b"not a png at all");
        assert_eq!(c.run(), 0);
        let image = raster::open(&c.output.join("a.png")).unwrap().rgba;
        assert_eq!(image.dimensions(), (100, 100));
        assert!(!raster::is_blank(&image));
    }
    // tests.test_make_emoji_pngs.TestGeneralRun.test_wrong_size_output_is_rebuilt
    #[test]
    fn test_wrong_size_output_is_rebuilt() {
        let c = Convert::new();
        c.raster("a.png", RED, (40, 70));
        png(&c.output.join("a.png"), BLUE, (64, 64));
        assert_eq!(c.run(), 0);
        let image = raster::open(&c.output.join("a.png")).unwrap().rgba;
        assert_eq!(image.dimensions(), (100, 100));
        assert!(!raster::is_blank(&image));
    }
    // tests.test_make_emoji_pngs.TestGeneralRun.test_blank_output_is_rebuilt
    #[test]
    fn test_blank_output_is_rebuilt() {
        let c = Convert::new();
        c.raster("a.png", RED, (40, 70));
        png(&c.output.join("a.png"), [0, 0, 0, 0], (100, 100));
        assert_eq!(c.run(), 0);
        let image = raster::open(&c.output.join("a.png")).unwrap().rgba;
        assert_eq!(image.dimensions(), (100, 100));
        assert!(!raster::is_blank(&image));
    }
    // tests.test_make_emoji_pngs.TestGeneralRun.test_pick_sources_prefers_svg_over_rasters
    #[test]
    fn test_pick_sources_prefers_svg_over_rasters() {
        let c = Convert::new();
        c.raster("foo.png", RED, (40, 70));
        image::RgbImage::from_pixel(40, 70, image::Rgb([240, 20, 20]))
            .save(c.source.join("foo.jpg"))
            .unwrap();
        file(&c.source.join("foo.svg"), BLUE_SVG.as_bytes());
        assert_eq!(c.run(), 0);
        let calls = c.codec.calls.lock().unwrap();
        let sources = calls
            .iter()
            .filter(|v| v["operation"] == "convert")
            .map(|v| {
                std::path::Path::new(v["source"].as_str().unwrap())
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(sources, vec!["foo.svg"]);
    }
    // tests.test_make_emoji_pngs.TestGeneralRun.test_svg_wins_over_same_stem_png
    #[test]
    fn test_svg_wins_over_same_stem_png() {
        let c = Convert::new();
        c.raster("foo.png", RED, (40, 70));
        file(&c.source.join("foo.svg"), BLUE_SVG.as_bytes());
        assert_eq!(c.run(), 0);
        let count = std::fs::read_dir(&c.output)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|v| v.path().extension().is_some_and(|s| s == "png"))
            .count();
        assert_eq!(count, 1);
        let center = c.center("foo.png");
        assert!(center[2] > center[0]);
    }
    // tests.test_make_emoji_pngs.TestGeneralRun.test_all_failed_exits_nonzero
    #[test]
    fn test_all_failed_exits_nonzero() {
        let c = Convert::new();
        for name in ["a.png", "b.png"] {
            c.raster(name, [0, 0, 0, 0], (40, 70));
        }
        assert_eq!(c.run(), 4);
        assert!(
            std::fs::read_dir(&c.output)
                .unwrap()
                .filter_map(Result::ok)
                .all(|v| v.path().extension().is_none_or(|s| s != "png"))
        );
    }
    // tests.test_make_emoji_pngs.TestGeneralRun.test_some_failed_exits_partial
    #[test]
    fn test_some_failed_exits_partial() {
        let c = Convert::new();
        c.raster("good.png", RED, (40, 70));
        c.raster("bad.png", [0, 0, 0, 0], (40, 70));
        assert_eq!(c.run(), 3);
    }
    // tests.test_make_emoji_pngs.TestGeneralRun.test_marker_names_the_file_being_converted
    #[test]
    fn test_marker_names_the_file_being_converted() {
        let c = Convert::new();
        c.raster("a.png", RED, (40, 70));
        assert_eq!(c.run(), 0);
        assert_eq!(*c.codec.marks.lock().unwrap(), vec!["a"]);
        assert!(!c.output.join(".svg_cur").exists());
    }
    // tests.test_make_emoji_pngs.TestGeneralRun.test_interrupted_file_is_quarantined_and_reported
    #[test]
    fn test_interrupted_file_is_quarantined_and_reported() {
        if !is_child() {
            let t = Scratch::new();
            let output = child(
                "test_make_emoji_pngs::test_general_run::test_interrupted_file_is_quarantined_and_reported",
                &t.0,
                &[],
            );
            assert!(output.contains("REVIEW"));
            return;
        }
        let c = Convert::new();
        c.raster("a.png", RED, (40, 70));
        file(&c.output.join(".svg_cur"), b"a");
        c.run();
        assert!(!c.output.join("a.png").exists());
        assert!(
            std::fs::read_to_string(c.output.join(".svg_skip.txt"))
                .unwrap()
                .contains("a")
        );
    }
}
mod coin_defaults_point_at_the_coins_folder {
    use super::*;
    // tests.test_make_emoji_pngs.CoinDefaultsPointAtTheCoinsFolder.test_the_legacy_conversion_reads_and_writes_under_coins
    #[test]
    fn test_the_legacy_conversion_reads_and_writes_under_coins() {
        let t = Scratch::new();
        file(&t.join("coins/logos/svg/vector.svg"), BLUE_SVG.as_bytes());
        png(&t.join("coins/logos/png/raster.png"), RED, (40, 70));
        let config = config(&t.0, &[]);
        let codec = Codec::new(&t.0);
        let log = rt::logging::RunLog::open(&t.0, "legacy-defaults", vec![]);
        assert_eq!(
            rt::convert_cli::run_with_codec(&config, &[], &log, |_| codec.clone()).unwrap(),
            0
        );
        for name in ["vector", "raster"] {
            assert_eq!(
                raster::open(&t.join(&format!("coins/logos/emoji/{name}.png")))
                    .unwrap()
                    .rgba
                    .dimensions(),
                (100, 100)
            );
        }
        assert!(!t.join("logos").exists());
    }
    // tests.test_make_emoji_pngs.CoinDefaultsPointAtTheCoinsFolder.test_auto_keywords_load_for_the_default_coin_source
    #[test]
    fn test_auto_keywords_load_for_the_default_coin_source() {
        if !is_child() {
            let t = Scratch::new();
            let output = child(
                "test_make_emoji_pngs::coin_defaults_point_at_the_coins_folder::test_auto_keywords_load_for_the_default_coin_source",
                &t.0,
                &[],
            );
            assert!(output.contains("keywords=1"));
            return;
        }
        let t = Scratch::new();
        file(
            &t.join("coins/keywords.csv"),
            b"ticker,name,format,file,keywords\na,Alpha,png,logos/png/a.png,\"a, Alpha\"\n",
        );
        let default = t.join("coins/logos/emoji");
        let corpus = t.join("corpus");
        let other = t.join("other");
        for source in [&default, &corpus, &other] {
            png(&source.join("a.png"), RED, (100, 100));
        }
        let source = match "default" {
            "default" => &default,
            "configured" => &corpus,
            _ => &other,
        };
        let c = config(
            &t.0,
            &[
                ("FAKE_TOKEN", "x".into()),
                (
                    "COIN_EMOJI_DIR",
                    if "default" == "configured" {
                        corpus.to_string_lossy().into()
                    } else {
                        String::new()
                    },
                ),
            ],
        );
        let log = rt::logging::RunLog::open(&t.0, "keyword-defaults", vec!["x".into()]);
        assert_eq!(
            rt::ticker_build::run(
                &c,
                &[
                    "--base".into(),
                    "t".into(),
                    "--title".into(),
                    "T".into(),
                    "--user-id".into(),
                    "1".into(),
                    "--source-dir".into(),
                    source.to_string_lossy().into(),
                    "--token-env".into(),
                    "FAKE_TOKEN".into(),
                    "--dry-run".into()
                ],
                &log
            )
            .unwrap(),
            0
        );
    }
    // tests.test_make_emoji_pngs.CoinDefaultsPointAtTheCoinsFolder.test_auto_keywords_load_for_the_operators_coin_corpus
    #[test]
    fn test_auto_keywords_load_for_the_operators_coin_corpus() {
        if !is_child() {
            let t = Scratch::new();
            let output = child(
                "test_make_emoji_pngs::coin_defaults_point_at_the_coins_folder::test_auto_keywords_load_for_the_operators_coin_corpus",
                &t.0,
                &[],
            );
            assert!(output.contains("keywords=1"));
            return;
        }
        let t = Scratch::new();
        file(
            &t.join("coins/keywords.csv"),
            b"ticker,name,format,file,keywords\na,Alpha,png,logos/png/a.png,\"a, Alpha\"\n",
        );
        let default = t.join("coins/logos/emoji");
        let corpus = t.join("corpus");
        let other = t.join("other");
        for source in [&default, &corpus, &other] {
            png(&source.join("a.png"), RED, (100, 100));
        }
        let source = match "configured" {
            "default" => &default,
            "configured" => &corpus,
            _ => &other,
        };
        let c = config(
            &t.0,
            &[
                ("FAKE_TOKEN", "x".into()),
                (
                    "COIN_EMOJI_DIR",
                    if "configured" == "configured" {
                        corpus.to_string_lossy().into()
                    } else {
                        String::new()
                    },
                ),
            ],
        );
        let log = rt::logging::RunLog::open(&t.0, "keyword-defaults", vec!["x".into()]);
        assert_eq!(
            rt::ticker_build::run(
                &c,
                &[
                    "--base".into(),
                    "t".into(),
                    "--title".into(),
                    "T".into(),
                    "--user-id".into(),
                    "1".into(),
                    "--source-dir".into(),
                    source.to_string_lossy().into(),
                    "--token-env".into(),
                    "FAKE_TOKEN".into(),
                    "--dry-run".into()
                ],
                &log
            )
            .unwrap(),
            0
        );
    }
    // tests.test_make_emoji_pngs.CoinDefaultsPointAtTheCoinsFolder.test_a_general_source_gets_no_coin_keywords
    #[test]
    fn test_a_general_source_gets_no_coin_keywords() {
        if !is_child() {
            let t = Scratch::new();
            let output = child(
                "test_make_emoji_pngs::coin_defaults_point_at_the_coins_folder::test_a_general_source_gets_no_coin_keywords",
                &t.0,
                &[],
            );
            assert!(output.contains("keywords=0"));
            return;
        }
        let t = Scratch::new();
        file(
            &t.join("coins/keywords.csv"),
            b"ticker,name,format,file,keywords\na,Alpha,png,logos/png/a.png,\"a, Alpha\"\n",
        );
        let default = t.join("coins/logos/emoji");
        let corpus = t.join("corpus");
        let other = t.join("other");
        for source in [&default, &corpus, &other] {
            png(&source.join("a.png"), RED, (100, 100));
        }
        let source = match "other" {
            "default" => &default,
            "configured" => &corpus,
            _ => &other,
        };
        let c = config(
            &t.0,
            &[
                ("FAKE_TOKEN", "x".into()),
                (
                    "COIN_EMOJI_DIR",
                    if "other" == "configured" {
                        corpus.to_string_lossy().into()
                    } else {
                        String::new()
                    },
                ),
            ],
        );
        let log = rt::logging::RunLog::open(&t.0, "keyword-defaults", vec!["x".into()]);
        assert_eq!(
            rt::ticker_build::run(
                &c,
                &[
                    "--base".into(),
                    "t".into(),
                    "--title".into(),
                    "T".into(),
                    "--user-id".into(),
                    "1".into(),
                    "--source-dir".into(),
                    source.to_string_lossy().into(),
                    "--token-env".into(),
                    "FAKE_TOKEN".into(),
                    "--dry-run".into()
                ],
                &log
            )
            .unwrap(),
            0
        );
    }
}
