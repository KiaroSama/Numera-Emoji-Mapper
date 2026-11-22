use super::*;

mod MakeEmojiPngsContracts {
    use super::*;
    // tests.test_entry_point_contracts.MakeEmojiPngsContracts.test_negative_limit_is_rejected
    #[test]
    fn test_negative_limit_is_rejected() {
        let root = Root::new();
        root.install();
        root.raster("in/a.png", [240, 20, 20, 255]);
        let result = root.run(
            &[
                "make-emoji-pngs",
                "--in",
                root.0.join("in").to_str().unwrap(),
                "--out",
                root.0.join("out").to_str().unwrap(),
                "--limit",
                "-1",
            ],
            &[],
        );
        assert_eq!(result.code, 2);
        assert!(!root.0.join("out").exists());
    }
    // tests.test_entry_point_contracts.MakeEmojiPngsContracts.test_blank_svg_falls_back_to_the_healthy_raster
    #[test]
    fn test_blank_svg_falls_back_to_the_healthy_raster() {
        let root = Root::new();
        root.install();
        root.file("in/foo.svg", EMPTY_SVG.as_bytes());
        root.raster("in/foo.png", [240, 20, 20, 255]);
        let result = root.run(
            &[
                "make-emoji-pngs",
                "--in",
                root.0.join("in").to_str().unwrap(),
                "--out",
                root.0.join("out").to_str().unwrap(),
            ],
            &[],
        );
        assert_eq!(result.code, 0, "{}", result.stderr);
        let image =
            runtime::native_raster::decode(&std::fs::read(root.0.join("out/foo.png")).unwrap())
                .unwrap();
        assert_eq!(image.rgba.dimensions(), (100, 100));
        let pixel = image.rgba.get_pixel(50, 50);
        assert!(pixel[0] > pixel[2]);
        assert!(!runtime::native_raster::is_blank(&image.rgba));
    }
    // tests.test_entry_point_contracts.MakeEmojiPngsContracts.test_group_with_no_usable_source_still_fails
    #[test]
    fn test_group_with_no_usable_source_still_fails() {
        let root = Root::new();
        root.install();
        root.file("in/foo.svg", EMPTY_SVG.as_bytes());
        root.raster("in/foo.png", [0, 0, 0, 0]);
        let result = root.run(
            &[
                "make-emoji-pngs",
                "--in",
                root.0.join("in").to_str().unwrap(),
                "--out",
                root.0.join("out").to_str().unwrap(),
            ],
            &[],
        );
        assert_eq!(result.code, 4);
        assert!(!root.0.join("out/foo.png").exists());
    }
    // tests.test_entry_point_contracts.MakeEmojiPngsContracts.test_preferred_source_is_still_tried_first
    #[test]
    fn test_preferred_source_is_still_tried_first() {
        let root = Root::new();
        root.install();
        root.file("in/foo.svg",br#"<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64"><rect width="64" height="64" fill="lime"/></svg>"#);
        root.raster("in/foo.png", [240, 20, 20, 255]);
        let result = root.run(
            &[
                "make-emoji-pngs",
                "--in",
                root.0.join("in").to_str().unwrap(),
                "--out",
                root.0.join("out").to_str().unwrap(),
            ],
            &[],
        );
        assert_eq!(result.code, 0);
        let image =
            runtime::native_raster::decode(&std::fs::read(root.0.join("out/foo.png")).unwrap())
                .unwrap();
        let pixel = image.rgba.get_pixel(50, 50);
        assert!(
            pixel[1] > pixel[0],
            "actual SVG priority must beat the healthy red fallback"
        );
    }
}

mod MakeEmojiPngsLegacyFallback {
    use super::*;
    // tests.test_entry_point_contracts.MakeEmojiPngsLegacyFallback.test_blank_svg_with_healthy_png_fallback_exits_ok
    #[test]
    fn test_blank_svg_with_healthy_png_fallback_exits_ok() {
        let root = Root::new();
        root.install();
        root.file("coins/logos/svg/btc.svg", EMPTY_SVG.as_bytes());
        root.raster("coins/logos/png/btc.png", [240, 20, 20, 255]);
        let result = root.run(&["make-emoji-pngs"], &[]);
        assert_eq!(result.code, 0, "{}", result.stderr);
        assert!(result.stdout.contains("failed=0"));
        assert_eq!(
            runtime::native_raster::decode(
                &std::fs::read(root.0.join("coins/logos/emoji/btc.png")).unwrap()
            )
            .unwrap()
            .rgba
            .dimensions(),
            (100, 100)
        );
    }
    // tests.test_entry_point_contracts.MakeEmojiPngsLegacyFallback.test_stem_with_no_usable_source_anywhere_still_fails
    #[test]
    fn test_stem_with_no_usable_source_anywhere_still_fails() {
        let root = Root::new();
        root.install();
        root.file("coins/logos/svg/bad.svg", EMPTY_SVG.as_bytes());
        let result = root.run(&["make-emoji-pngs"], &[]);
        assert_eq!(result.code, 4);
        assert!(!root.0.join("coins/logos/emoji/bad.png").exists());
    }
    // tests.test_entry_point_contracts.MakeEmojiPngsLegacyFallback.test_blank_png_fallback_is_counted_once_not_twice
    #[test]
    fn test_blank_png_fallback_is_counted_once_not_twice() {
        let root = Root::new();
        root.install();
        root.file("coins/logos/svg/bad.svg", EMPTY_SVG.as_bytes());
        root.raster("coins/logos/png/bad.png", [0, 0, 0, 0]);
        root.raster("coins/logos/png/good.png", [240, 20, 20, 255]);
        let result = root.run(&["make-emoji-pngs"], &[]);
        assert_eq!(result.code, 3);
        assert!(result.stdout.contains("failed=1"));
    }
}
