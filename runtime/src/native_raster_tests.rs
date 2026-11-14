use super::*;

#[test]
fn historical_pillow_jpeg_pixels_keys_and_hashes_survive_all_chroma_modes() {
    // Encoded JPEG inputs and expected hashes were generated independently by
    // historical Pillow 12.3.0; these are intentionally embedded immutable inputs.
    let fixture:serde_json::Value = serde_json::from_str(r##"[
  {
    "name": "RGB-sub0-progressiveFalse",
    "encoded": "ffd8ffe000104a46494600010100000100010000ffdb0043000503040404030504040405050506070c08070707070f0b0b090c110f1212110f111113161c1713141a1511111821181a1d1d1f1f1f13172224221e241c1e1f1effdb0043010505050706070e08080e1e1411141e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1effc00011080007000903011100021101031101ffc4001500010100000000000000000000000000000607ffc4001f1000010206030000000000000000000000030001060713172231515291ffc4001501010100000000000000000000000000000506ffc4001f1100020005050000000000000000000000000201030512510615162131ffda000c03010002110311003f000f014b01571e03df2ca3e755dedf4129ba99ad87655ad88ba0fd645eeef91ae4cd93ffd9",
    "mode": "RGB",
    "pixels": "54c05258440cc380205617e2981166819592b2d40f3e5e0574314f80d109a510",
    "key": "s:bdc2a01556806ec5d6f28a89a3a191d3",
    "phash": 0
  },
  {
    "name": "RGB-sub1-progressiveFalse",
    "encoded": "ffd8ffe000104a46494600010100000100010000ffdb0043000503040404030504040405050506070c08070707070f0b0b090c110f1212110f111113161c1713141a1511111821181a1d1d1f1f1f13172224221e241c1e1f1effdb0043010505050706070e08080e1e1411141e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1effc00011080007000903012100021101031101ffc4001500010100000000000000000000000000000607ffc4001f1000010206030000000000000000000000030001060713172231515291ffc4001501010100000000000000000000000000000406ffc4001c1100000603000000000000000000000000000102040521061213ffda000c03010002110311003f000f014b01571e03df2caad6c45d07eb285752ebd8ac05864cae2563ffd9",
    "mode": "RGB",
    "pixels": "c789080f64fb1724d0b2200baba90100b71e465219330ba56f3a217970ce773d",
    "key": "s:32771ac036d760a8c477c543bfe7738f",
    "phash": 0
  },
  {
    "name": "RGB-sub2-progressiveFalse",
    "encoded": "ffd8ffe000104a46494600010100000100010000ffdb0043000503040404030504040405050506070c08070707070f0b0b090c110f1212110f111113161c1713141a1511111821181a1d1d1f1f1f13172224221e241c1e1f1effdb0043010505050706070e08080e1e1411141e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1effc00011080007000903012200021101031101ffc400160001010100000000000000000000000000000607ffc4001f1000020102070000000000000000000000000301060713172231515291ffc4001501010100000000000000000000000000000607ffc4001a110002030101000000000000000000000004050001221213ffda000c03010002110311003f0087a0ad82b1d7a17bf306ad962ae8bf60027053123aad432bde9be35b9fffd9",
    "mode": "RGB",
    "pixels": "b4f01eb7d867d82dd36eb1af300c153a9a24fccf361f82af54a90302a4dd14ea",
    "key": "s:ade2a9bb5e450be4f748693fca11ac44",
    "phash": 0
  },
  {
    "name": "RGB-sub2-progressiveTrue",
    "encoded": "ffd8ffe000104a46494600010100000100010000ffdb0043000503040404030504040405050506070c08070707070f0b0b090c110f1212110f111113161c1713141a1511111821181a1d1d1f1f1f13172224221e241c1e1f1effdb0043010505050706070e08080e1e1411141e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1e1effc20011080007000903012200021101031101ffc400160001010100000000000000000000000000000506ffc4001501010100000000000000000000000000000506ffda000c0301000210031000000187ab26ccffc400191000010500000000000000000000000000010004051115ffda000801010001050261182f302fffc4001811000203000000000000000000000000000004031231ffda0008010301013f015de9a9a7ffc4001811000203000000000000000000000000000003041131ffda0008010201013f016c865e9fffc40017100100030000000000000000000000000000013141ffda0008010100063f028a63ffc400161001010100000000000000000000000000003161ffda0008010100013f218a323fffda000c030100020003000000100bffc4001511010100000000000000000000000000000011ffda0008010301013f108cff00ffc4001511010100000000000000000000000000000011ffda0008010201013f10887fffc40018100002030000000000000000000000000000015161f1ffda0008010100013f10bf528d647fffd9",
    "mode": "RGB",
    "pixels": "b4f01eb7d867d82dd36eb1af300c153a9a24fccf361f82af54a90302a4dd14ea",
    "key": "s:ade2a9bb5e450be4f748693fca11ac44",
    "phash": 0
  },
  {
    "name": "CMYK-sub0-progressiveFalse",
    "encoded": "ffd8ffee000e41646f626500640000000000ffdb0043000503040404030504040405050506070c08070707070f0b0b090c110f1212110f111113161c1713141a1511111821181a1d1d1f1f1f13172224221e241c1e1f1effc000140800070009044311004d11005911004b1100ffc40017000101010100000000000000000000000007050608ffc4002710000003050705000000000000000000000001030407234191020614172122510515315253ffda000e0443004d0059004b00003f005c7ceda71f5398a77adfd34e0edc55bc7063a2ef5f5b4b7e874171e7bd05a2ef527230098d3e4c1a67d34fd56a18c677b4b83a021cd05bdd4a18ffd9",
    "mode": "CMYK",
    "pixels": "0eb9ebc56b9abf6e410279b856ab2a13f5662444348e02c1b56017bca4843a3a",
    "key": "s:36a04aadaa1b00fa32fa3e8699f0bc1c",
    "phash": 18446744073709551615
  },
  {
    "name": "L-sub0-progressiveFalse",
    "encoded": "ffd8ffe000104a46494600010100000100010000ffdb0043000503040404030504040405050506070c08070707070f0b0b090c110f1212110f111113161c1713141a1511111821181a1d1d1f1f1f13172224221e241c1e1f1effc0000b080007000901011100ffc40014000100000000000000000000000000000007ffc4001c1000020007000000000000000000000000000104071421234151ffda0008010100003f002293104b059687aa25c47fffd9",
    "mode": "L",
    "pixels": "547c8492dc00b698dda788c4da16179a237bac8bd1fd87b6b7ca0cffef224f14",
    "key": "s:5f5c97d3d12382223d4a3be39544c6ef",
    "phash": 0
  }
]"##).unwrap();
    for case in fixture.as_array().unwrap() {
        let encoded = case["encoded"]
            .as_str()
            .unwrap()
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect::<Vec<_>>();
        let raster = decode(&encoded).unwrap();
        assert_eq!(
            raster.mode,
            case["mode"].as_str().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(
            digest_hex(raster.rgba.as_raw()),
            case["pixels"].as_str().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(
            content_key(&raster.rgba).unwrap(),
            case["key"].as_str().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(
            source_dhash(&raster).unwrap(),
            case["phash"].as_u64().unwrap(),
            "{}",
            case["name"]
        );
    }
    // A recognized but invalid JPEG must report an error without unwinding out
    // of the media boundary or terminating the process.
    assert!(decode(b"\xff\xd8\xff\xc0\x00\x02\xff\xd9").is_err());
}

fn patterned() -> RgbaImage {
    RgbaImage::from_fn(37, 23, |x, y| {
        Rgba([
            ((x * 17 + y * 3) % 256) as u8,
            ((x * x + y * 29) % 256) as u8,
            ((y * y + x * 7) % 256) as u8,
            ((x * 13 + y * 19) % 256) as u8,
        ])
    })
}

#[test]
fn optimized_animation_matches_independent_pillow_frames_and_final_flush() {
    // Independent Pillow 12.3.0/libwebp 1.6.0 references, including duplicate
    // coalescing and a final duration that differs from all preceding durations.
    let image = patterned();
    let tinted = tint(&image, [19, 107, 203]);
    let animation = RasterAnimation {
        frames: vec![
            RasterFrame {
                rgba: image.clone(),
                duration_ms: 40,
            },
            RasterFrame {
                rgba: image.clone(),
                duration_ms: 80,
            },
            RasterFrame {
                rgba: tinted,
                duration_ms: 120,
            },
            RasterFrame {
                rgba: image.clone(),
                duration_ms: 160,
            },
            RasterFrame {
                rgba: image,
                duration_ms: 200,
            },
        ],
        loop_count: 3,
    };
    for (lossless, hashes) in [
        (
            false,
            [
                "5623309cae97d621975ea0653f2582fb2069afb6a223bb7933c5210cc7cdf6e0",
                "44c5507f8ae51eef5364bfe84c23bde0343a8f0c98ab3ad8776f3b5c598ddfdc",
            ],
        ),
        (
            true,
            [
                "73d50299b538ff513e69dd0af884c5b1e5ecc2d496c18220a1b21e4563bbfbd0",
                "a1c6af7391a539ea6f0ba206939f2a9fed6ee04533f4b6eeed8d67fcf3fdcea7",
            ],
        ),
    ] {
        let bytes = encode_pillow_animated_webp(
            &animation,
            WebpOptions {
                lossless,
                quality: 60.0,
                method: 4,
            },
        )
        .unwrap();
        let decoded = decode_animation(&bytes, DecodeLimits::default()).unwrap();
        assert_eq!(decoded.loop_count, 3);
        assert_eq!(decoded.frames.len(), 3);
        for (index, (duration, hash)) in [(120, hashes[0]), (120, hashes[1]), (360, hashes[0])]
            .into_iter()
            .enumerate()
        {
            assert_eq!(decoded.frames[index].duration_ms, duration);
            assert_eq!(digest_hex(decoded.frames[index].rgba.as_raw()), hash);
        }
    }
}

#[test]
fn frozen_pre_migration_static_keys_and_source_mode_hashes_are_unchanged() {
    // These committed fixtures predate this engine; never regenerate expectations
    // by calling this implementation.
    let fixture: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/static-codec.json"
    ))
    .unwrap();
    let inputs: [&[u8]; 4] = [
        include_bytes!("../../tests/fixtures/native/static-codec/public-logo.png"),
        include_bytes!("../../tests/fixtures/native/static-codec/opaque.png"),
        include_bytes!("../../tests/fixtures/native/static-codec/transparent-rgb.png"),
        include_bytes!("../../tests/fixtures/native/static-codec/alpha-edge.png"),
    ];
    for (case, input) in fixture["cases"].as_array().unwrap().iter().zip(inputs) {
        assert_eq!(digest_hex(input), case["input_sha256"].as_str().unwrap());
        let decoded = decode(input).unwrap();
        let expected = &case["expected"];
        assert_eq!(decoded.mode, expected["mode"].as_str().unwrap());
        assert_eq!(
            decoded.rgba.width() as u64,
            expected["size"][0].as_u64().unwrap()
        );
        assert_eq!(
            decoded.rgba.height() as u64,
            expected["size"][1].as_u64().unwrap()
        );
        assert_eq!(
            is_blank(&decoded.rgba),
            expected["blank"].as_bool().unwrap()
        );
        assert_eq!(
            content_key(&decoded.rgba).unwrap(),
            expected["content_key"].as_str().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(
            source_dhash(&decoded).unwrap(),
            expected["phash"].as_u64().unwrap(),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn independent_pillow_12_3_lanczos_rounding_alpha_dhash_and_remap_reference() {
    // Reference generated once with the historical Pillow 12.3.0 oracle, using
    // exactly patterned()'s integer pixel formula, before running the new engine.
    let references = [
        (
            (64, 64),
            "ccc7f93fe0e961053d6adcf77b8f1125b6efff0d33c0b68e11439e77ead93b7f",
        ),
        (
            (100, 62),
            "38b55b7485a4fc7cadb2642d8ca3f07e3ff7fa04de5bf9e92dd22bc54cef897b",
        ),
        (
            (9, 8),
            "2b54d4ff330d9b1177ea65229c3fa76af0d156da62c8a0cb992e3a9ec44ff7bb",
        ),
        (
            (1, 1),
            "d7be0df24d20dd90dc4057125f5d2e64c97f2ddf4f43dbfc0cb241c886782202",
        ),
        (
            (11, 41),
            "6acb7361224a70e88b50c9f73d3c73769116b213327567e7353297c6e7139036",
        ),
        (
            (37, 23),
            "a40ffcce0fc861abafdaac2f72d4809fa1f2ba1200d34b70d3b4bd6eee3c15f1",
        ),
    ];
    let rgba = patterned();
    for ((width, height), expected) in references {
        assert_eq!(
            digest_hex(resize_lanczos(&rgba, width, height).unwrap().as_raw()),
            expected,
            "{width}x{height}"
        );
    }
    assert_eq!(dhash(&rgba).unwrap(), 1_743_284_235_711_034_214);
    assert_eq!(
        digest_hex(&remap_signature(&rgba).unwrap()),
        "9cfc4f2868fb9fc0229dcf936303cef1d47651eedaa54e746a020ca2d815ece3"
    );
    assert_eq!(
        digest_hex(fit_canvas(&rgba, 100).unwrap().as_raw()),
        "b9267a54681860a5983572c16fa645a45677f924d0b635837afde26c3629f059"
    );
}

#[test]
fn independent_pillow_tall_raster_pass_order_and_bicubic_thumbnail_reference() {
    let tall = RgbaImage::from_fn(3, 401, |x, y| {
        Rgba([
            ((x * 17 + y * 3) % 256) as u8,
            ((x * x + y * 29) % 256) as u8,
            ((y * y + x * 7) % 256) as u8,
            ((x * 13 + y * 19) % 256) as u8,
        ])
    });
    assert_eq!(
        digest_hex(resize_lanczos(&tall, 64, 64).unwrap().as_raw()),
        "e6621bdcccdfe75e68a5ababcdd66afd466104ee54b0e6d5c0aa82f284bd6765"
    );
    assert_eq!(
        digest_hex(resize_lanczos(&tall, 19, 9).unwrap().as_raw()),
        "f032f0991011ddbb4b19697e9bd0d26f28b7efce76c60313ab11a94b97f4f31a"
    );
    let thumb = thumbnail(&tall, 64, ResampleFilter::Bicubic).unwrap();
    assert_eq!(thumb.dimensions(), (1, 64));
    assert_eq!(
        digest_hex(thumb.as_raw()),
        "dc63adc5be875ac3bde049fa376c98f493da419f64c8ef16f42774ad7f99592c"
    );
}

#[test]
fn pillow_16_bit_png_modes_clamp_integer_luminance_and_truncate_color_samples() {
    // Pixel expectations independently observed from Pillow 12.3.0 PNG decode.
    let samples = [0u16, 1, 254, 255, 256, 257, 32767, 32768, 65534, 65535];
    for (channels, color, mode, expected) in [
        (
            1,
            image::ExtendedColorType::L16,
            "I;16",
            vec![
                [0, 0, 0, 255],
                [1, 1, 1, 255],
                [254, 254, 254, 255],
                [255, 255, 255, 255],
                [255, 255, 255, 255],
                [255, 255, 255, 255],
                [255, 255, 255, 255],
                [255, 255, 255, 255],
                [255, 255, 255, 255],
                [255, 255, 255, 255],
            ],
        ),
        (
            3,
            image::ExtendedColorType::Rgb16,
            "RGB",
            vec![
                [0, 0, 0, 255],
                [0, 0, 0, 255],
                [0, 0, 1, 255],
                [0, 1, 1, 255],
                [1, 1, 127, 255],
                [1, 127, 128, 255],
                [127, 128, 255, 255],
                [128, 255, 255, 255],
                [255, 255, 0, 255],
                [255, 0, 0, 255],
            ],
        ),
        (
            2,
            image::ExtendedColorType::La16,
            "RGBA",
            vec![
                [0, 0, 0, 0],
                [0, 0, 0, 0],
                [0, 0, 0, 0],
                [0, 0, 0, 1],
                [1, 1, 1, 1],
                [1, 1, 1, 127],
                [127, 127, 127, 128],
                [128, 128, 128, 255],
                [255, 255, 255, 255],
                [255, 255, 255, 0],
            ],
        ),
        (
            4,
            image::ExtendedColorType::Rgba16,
            "RGBA",
            vec![
                [0, 0, 0, 0],
                [0, 0, 0, 1],
                [0, 0, 1, 1],
                [0, 1, 1, 127],
                [1, 1, 127, 128],
                [1, 127, 128, 255],
                [127, 128, 255, 255],
                [128, 255, 255, 0],
                [255, 255, 0, 0],
                [255, 0, 0, 0],
            ],
        ),
    ] {
        let raw: Vec<u8> = (0..10)
            .flat_map(|x| {
                (0..channels).flat_map(move |c| samples[(x + c) % samples.len()].to_ne_bytes())
            })
            .collect();
        let mut encoded = Vec::new();
        image::codecs::png::PngEncoder::new(&mut encoded)
            .write_image(&raw, 10, 1, color)
            .unwrap();
        let decoded = decode(&encoded).unwrap();
        assert_eq!(decoded.mode, mode);
        assert_eq!(
            decoded.rgba.pixels().map(|p| p.0).collect::<Vec<_>>(),
            expected
        );
    }
}

#[test]
fn native_gif_decode_preserves_frame_colors_duration_and_loop_metadata() {
    let red = RgbaImage::from_pixel(4, 3, Rgba([255, 0, 0, 255]));
    let blue = RgbaImage::from_pixel(4, 3, Rgba([0, 0, 255, 255]));
    let mut encoded = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut encoded);
        encoder
            .set_repeat(image::codecs::gif::Repeat::Finite(3))
            .unwrap();
        encoder
            .encode_frames([
                image::Frame::from_parts(
                    red.clone(),
                    0,
                    0,
                    image::Delay::from_numer_denom_ms(100, 1),
                ),
                image::Frame::from_parts(
                    blue.clone(),
                    0,
                    0,
                    image::Delay::from_numer_denom_ms(200, 1),
                ),
            ])
            .unwrap();
    }
    let decoded = decode(&encoded).unwrap();
    assert_eq!(decoded.mode, "P");
    assert!(decoded.animated);
    assert_eq!(decoded.rgba, red);
    let animation = decode_animation(&encoded, DecodeLimits::default()).unwrap();
    assert_eq!(animation.loop_count, 3);
    assert_eq!(animation.frames.len(), 2);
    assert_eq!(animation.frames[0].rgba, red);
    assert_eq!(animation.frames[0].duration_ms, 100);
    assert_eq!(animation.frames[1].rgba, blue);
    assert_eq!(animation.frames[1].duration_ms, 200);
}

#[test]
fn blank_threshold_counts_nine_pixels_above_ten() {
    let mut rgba = RgbaImage::from_pixel(10, 10, Rgba([200, 20, 30, 10]));
    assert!(is_blank(&rgba));
    for x in 0..8 {
        rgba[(x, 0)][3] = 11;
    }
    assert!(is_blank(&rgba));
    rgba[(8, 0)][3] = 11;
    assert!(!is_blank(&rgba));
}

#[test]
fn deterministic_png_and_lossless_webp_preserve_every_channel_including_hidden_rgb() {
    let mut rgba = patterned();
    rgba[(0, 0)] = Rgba([213, 9, 41, 0]);
    let png = encode_png(&rgba).unwrap();
    assert_eq!(png, encode_png(&rgba).unwrap());
    assert_eq!(decode(&png).unwrap().rgba, rgba);
    let webp = encode_webp(&rgba, WebpOptions::default()).unwrap();
    assert_eq!(webp, encode_webp(&rgba, WebpOptions::default()).unwrap());
    assert_eq!(decode(&webp).unwrap().rgba, rgba);
}

#[test]
fn native_webp_animation_preserves_full_frames_hidden_rgb_timing_and_loop_count() {
    let mut first = patterned();
    first[(0, 0)] = Rgba([201, 23, 17, 0]);
    let second = tint(&first, [19, 107, 203]);
    let animation = RasterAnimation {
        frames: vec![
            RasterFrame {
                rgba: first.clone(),
                duration_ms: 40,
            },
            RasterFrame {
                rgba: second.clone(),
                duration_ms: 80,
            },
            RasterFrame {
                rgba: first.clone(),
                duration_ms: 120,
            },
        ],
        loop_count: 3,
    };
    let encoded = encode_animated_webp(&animation, WebpOptions::default()).unwrap();
    let decoded = decode_animation(&encoded, DecodeLimits::default()).unwrap();
    assert_eq!(decoded.loop_count, 3);
    assert_eq!(decoded.frames.len(), 3);
    for (actual, expected) in decoded.frames.iter().zip(&animation.frames) {
        assert_eq!(actual.duration_ms, expected.duration_ms);
        assert_eq!(actual.rgba, expected.rgba);
    }
    let first_decoded = decode(&encoded).unwrap();
    assert!(first_decoded.animated);
    assert_eq!(first_decoded.rgba, first);
}

#[test]
fn dimension_input_and_animation_bounds_reject_oversized_work() {
    let encoded = encode_png(&patterned()).unwrap();
    assert!(
        decode_with_limits(
            &encoded,
            DecodeLimits {
                max_dimension: 36,
                ..DecodeLimits::default()
            }
        )
        .is_err()
    );
    assert!(
        decode_with_limits(
            &encoded,
            DecodeLimits {
                max_pixels: 100,
                ..DecodeLimits::default()
            }
        )
        .is_err()
    );
    assert!(
        decode_with_limits(
            &encoded,
            DecodeLimits {
                max_encoded_bytes: 12,
                ..DecodeLimits::default()
            }
        )
        .is_err()
    );
    let animation = RasterAnimation {
        frames: vec![
            RasterFrame {
                rgba: patterned(),
                duration_ms: 50,
            },
            RasterFrame {
                rgba: patterned(),
                duration_ms: 50,
            },
        ],
        loop_count: 0,
    };
    let encoded = encode_animated_webp(&animation, WebpOptions::default()).unwrap();
    assert!(
        decode_animation(
            &encoded,
            DecodeLimits {
                max_frames: 1,
                ..DecodeLimits::default()
            }
        )
        .is_err()
    );
    assert!(
        decode_animation(
            &encoded,
            DecodeLimits {
                max_decoded_bytes: 5000,
                ..DecodeLimits::default()
            }
        )
        .is_err()
    );
    assert!(resize_lanczos(&patterned(), u32::MAX, u32::MAX).is_err());
    assert!(decode(b"not an image").is_err());
    assert!(
        encode_webp(
            &patterned(),
            WebpOptions {
                quality: f32::NAN,
                ..WebpOptions::default()
            }
        )
        .is_err()
    );
}

fn assert_pixel(actual: [u8; 4], expected: [u8; 4], resized: bool) {
    assert_eq!(actual[3], expected[3], "fitting changed opacity");
    for channel in 0..3 {
        assert!(actual[channel].abs_diff(expected[channel]) <= if resized { 2 } else { 0 });
    }
}

#[test]
fn preserves_existing_partial_alpha_fitting_cutout_trim_and_white_backdrop_cases() {
    for alpha in [1, 64, 128, 192, 254] {
        let rgba = RgbaImage::from_pixel(100, 100, Rgba([255, 0, 0, alpha]));
        assert_pixel(
            fit_canvas(&rgba, 100).unwrap()[(50, 50)].0,
            [255, 0, 0, alpha],
            false,
        );
    }
    let opaque = RgbaImage::from_pixel(100, 100, Rgba([12, 200, 90, 255]));
    assert_pixel(
        fit_canvas(&opaque, 100).unwrap()[(50, 50)].0,
        [12, 200, 90, 255],
        false,
    );
    let resized = RgbaImage::from_pixel(40, 40, Rgba([0, 100, 255, 96]));
    assert_pixel(
        fit_canvas(&resized, 100).unwrap()[(50, 50)].0,
        [0, 100, 255, 96],
        true,
    );
    let mut cutout = RgbaImage::from_pixel(60, 60, Rgba([255, 128, 0, 200]));
    for x in 20..40 {
        for y in 20..40 {
            cutout[(x, y)] = Rgba([0, 0, 0, 0]);
        }
    }
    let fitted = fit_canvas(&cutout, 100).unwrap();
    assert_eq!(fitted[(50, 50)][3], 0);
    assert_pixel(fitted[(5, 5)].0, [255, 128, 0, 200], true);
    let mut bordered = RgbaImage::new(80, 80);
    for x in 30..50 {
        for y in 30..50 {
            bordered[(x, y)] = Rgba([10, 220, 30, 150]);
        }
    }
    assert_pixel(
        fit_canvas(&bordered, 100).unwrap()[(50, 50)].0,
        [10, 220, 30, 150],
        true,
    );
    assert_eq!(
        fit_canvas(&RgbaImage::new(0, 10), 100).unwrap()[(50, 50)].0,
        [0, 0, 0, 0]
    );
    let red = RgbaImage::from_pixel(10, 10, Rgba([255, 0, 0, 128]));
    assert_eq!(on_white(&red)[(5, 5)].0, [255, 127, 127]);
    assert_eq!(red[(5, 5)].0, [255, 0, 0, 128]);
}

#[test]
fn tint_preserves_alpha_and_comparison_uses_premultiplied_threshold() {
    let rgba = patterned();
    let tinted = tint(&rgba, [10, 20, 30]);
    for (before, after) in rgba.pixels().zip(tinted.pixels()) {
        assert_eq!(after.0, [10, 20, 30, before[3]]);
    }
    let left = RgbaImage::from_pixel(64, 64, Rgba([255, 1, 4, 0]));
    let right = RgbaImage::from_pixel(64, 64, Rgba([0, 254, 1, 0]));
    assert!(same_rgba(&left, &right).unwrap());
    assert_eq!(same_frame(left.as_raw(), right.as_raw()), Some(true));
    assert_eq!(same_frame(&[0; 8], &[0; 8]), None);
    assert_eq!(frame_phash(&[0; 8]), None);
    let a = vec![0; 64 * 64 * 4];
    let b = vec![8; 64 * 64 * 4];
    assert_eq!(close_pixels(&a, &b), Some(true));
    assert_eq!(close_pixels(&a, &vec![9; 64 * 64 * 4]), Some(false));
}
