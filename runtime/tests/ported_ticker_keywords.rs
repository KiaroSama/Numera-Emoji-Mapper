// Ported from native/tests/ticker_keywords.rs at 45a96f9de70da8429b83c047e0626b6ed6694b20.
// Original source SHA256: 789971093a740e106d4ca905ca2f2bfdeafd637a560255f218c4bae102fddbfe.
// Original test identities and behavioral assertions retained against the production crate.
extern crate numera_runtime as _native;

// Standards-defined CSV quoting and Unicode values remain unchanged at the keyword boundary.
use _native::ticker_keywords::load;
use std::path::PathBuf;
#[test]
fn quoted_multiline_keyword_fields_are_not_split_or_trimmed() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf();
    let path = root
        .join("logs")
        .join(format!("keyword-test-{}.csv", std::process::id()));
    let bytes =
        "ticker,keywords\nAAA,\"alpha, beta\"\nBBB,\"line one\nline دو\"\nCCC,\nAAA,replaced\n";
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    std::fs::write(&path, bytes.as_bytes()).unwrap();
    let result = load(&path);
    std::fs::remove_file(&path).unwrap();
    let result = result.unwrap();
    assert_eq!(result["aaa"], "replaced");
    assert_eq!(result["bbb"], "line one\nline دو");
    assert_eq!(result["ccc"], "CCC");
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
