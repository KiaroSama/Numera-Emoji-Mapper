use super::*;
fn fetch(root: &Root, market: Value, download: Vec<u8>) -> (u8, Vec<String>) {
    let mut asked = vec![];
    let result = rt::coin_gecko_fetch::fetch_with(
        &root.0,
        1,
        Duration::ZERO,
        |url, binary, _| {
            asked.push(url.into());
            Ok(if binary {
                rt::coin_gecko_fetch::Payload::Bytes(download.clone())
            } else {
                rt::coin_gecko_fetch::Payload::Json(market.clone())
            })
        },
        |_| Ok(()),
    )
    .unwrap();
    (result, asked)
}
// tests.test_coin_logo_cache.FetchLogosCacheValidation.test_valid_image_rejects_error_bodies_and_truncation
#[test]
fn test_valid_image_rejects_error_bodies_and_truncation() {
    let t = Root::new();
    let good = solid(32, [20, 120, 200, 255]);
    for (name, bytes, want) in [
        ("good.png", good.clone(), true),
        ("html.png", HTML.into(), false),
        ("cut.png", good[..60].to_vec(), false),
        ("blank.png", solid(32, [0, 0, 0, 0]), false),
    ] {
        let p = t.0.join(name);
        std::fs::write(&p, bytes).unwrap();
        assert_eq!(rt::codec_image::valid_logo(&p), want, "{name}");
    }
}
// tests.test_coin_logo_cache.FetchLogosCacheValidation.test_bad_download_never_reaches_the_destination
#[test]
fn test_bad_download_never_reaches_the_destination() {
    let t = Root::new();
    let (rc, _) = fetch(
        &t,
        json!([{"symbol":"ABC","name":"","image":"http://x/abc.png"}]),
        HTML.into(),
    );
    assert_eq!(rc, 0);
    assert!(!t.0.join("logos/png/abc.png").exists());
    assert_eq!(std::fs::read_dir(t.0.join("logos/png")).unwrap().count(), 0);
}
// tests.test_coin_logo_cache.FetchLogosCacheValidation.test_corrupt_cached_png_is_redownloaded
#[test]
fn test_corrupt_cached_png_is_redownloaded() {
    let t = Root::new();
    std::fs::create_dir_all(t.0.join("logos/png")).unwrap();
    std::fs::write(t.0.join("logos/png/btc.png"), HTML).unwrap();
    let (rc, asked) = fetch(
        &t,
        json!([{"symbol":"BTC","name":"Bitcoin","image":"http://img.test/btc.png"}]),
        solid(32, [20, 120, 200, 255]),
    );
    assert_eq!(rc, 0);
    assert!(asked.contains(&"http://img.test/btc.png".into()));
    assert!(rt::codec_image::valid_logo(&t.0.join("logos/png/btc.png")));
}
// tests.test_coin_logo_cache.FetchLogosCacheValidation.test_good_cached_png_is_not_redownloaded
#[test]
fn test_good_cached_png_is_not_redownloaded() {
    let t = Root::new();
    std::fs::create_dir_all(t.0.join("logos/png")).unwrap();
    let bytes = solid(32, [20, 120, 200, 255]);
    std::fs::write(t.0.join("logos/png/btc.png"), &bytes).unwrap();
    let (_, asked) = fetch(
        &t,
        json!([{"symbol":"BTC","name":"Bitcoin","image":"http://img.test/btc.png"}]),
        bytes,
    );
    assert!(!asked.contains(&"http://img.test/btc.png".into()));
}
// tests.test_coin_logo_cache.FetchLogosCacheValidation.test_corrupt_leftover_png_is_not_advertised
#[test]
fn test_corrupt_leftover_png_is_not_advertised() {
    let t = Root::new();
    let dir = t.0.join("logos/png");
    std::fs::create_dir_all(&dir).unwrap();
    let good = solid(32, [20, 120, 200, 255]);
    std::fs::write(dir.join("junk.png"), HTML).unwrap();
    std::fs::write(dir.join("cut.png"), &good[..60]).unwrap();
    std::fs::write(dir.join("ok.png"), good).unwrap();
    assert_eq!(fetch(&t, json!([]), vec![]).0, 0);
    let rows = csv_rows(&t.0.join("keywords.csv"));
    assert!(rows.contains_key("ok"));
    assert!(!rows.contains_key("junk"));
    assert!(!rows.contains_key("cut"));
}
