//! Real disk keyword inventory reuses pinned CSV/image policies without provider network.
use serde_json::json;
use std::path::PathBuf;

#[test]
fn disk_inventory_prefers_svg_and_keeps_previous_names_for_unanswered_lookup() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let scratch = root
        .join("logs/tmp")
        .join(format!("keyword inventory {}", std::process::id()));
    std::fs::create_dir_all(scratch.join("logos/svg")).unwrap();
    std::fs::create_dir(scratch.join("logos/png")).unwrap();
    std::fs::write(
        scratch.join("logos/svg/btc.svg"),
        b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>",
    )
    .unwrap();
    let image =
        std::fs::read(root.join("tests/fixtures/native/static-codec/alpha-edge.png")).unwrap();
    std::fs::write(scratch.join("logos/png/btc.png"), &image).unwrap();
    std::fs::write(scratch.join("logos/png/eth.png"), &image).unwrap();
    std::fs::write(scratch.join("logos/png/bad.png"), b"error page").unwrap();
    std::fs::write(
        scratch.join("keywords.csv"),
        b"ticker,name,format,file,keywords\r\neth,Ethereum,png,logos/png/eth.png,eth\r\n",
    )
    .unwrap();
    let names = json!({"btc":"Bitcoin"});
    let rows = _native::coin_keywords_cli::build(&scratch, names.as_object().unwrap()).unwrap();
    assert_eq!(rows["btc"]["format"], "svg");
    assert_eq!(rows["eth"]["format"], "png");
    let csv = std::fs::read_to_string(scratch.join("keywords.csv")).unwrap();
    assert!(csv.contains("btc,Bitcoin,svg,logos/svg/btc.svg,\"btc, Bitcoin\"\r\n"));
    assert!(csv.contains("eth,Ethereum,png,logos/png/eth.png,\"eth, Ethereum\"\r\n"));
    assert!(!csv.contains("bad,"));
    std::fs::remove_dir_all(scratch).unwrap();
}
