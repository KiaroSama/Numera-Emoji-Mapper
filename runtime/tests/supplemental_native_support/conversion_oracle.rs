use serde_json::Value;
use sha2::{Digest, Sha256};
pub fn expected(input: &str) -> Value {
    let text =
        include_str!("../../../tests/fixtures/native/static-convert.json").replace("\r\n", "\n");
    let bytes = text.as_bytes();
    let digest = Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    assert_eq!(
        digest,
        "3d9ee8fa644edb16854b0c2fc8f323c32a8250179c0a4c007572178bba34ec50"
    );
    let fixture: Value = serde_json::from_slice(bytes).unwrap();
    fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|case| case["input"] == input)
        .unwrap_or_else(|| panic!("Frozen conversion recipe has no {input}"))["expected"]
        .clone()
}
