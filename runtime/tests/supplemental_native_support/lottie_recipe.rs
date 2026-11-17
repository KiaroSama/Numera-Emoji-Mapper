// Retained pure input recipe checks from native/src/source_lottie_oracle.rs; SHA256 88b4340bf64b55475cf3a3f8b5848b80df4f38696c14db62c63ddde28d4f2f35.
use serde_json::Value;
const FIXTURE_HASH: &str = "bfce4eafbb267ca24bcd5c02faac7a1ef4474edb5b164cc15dc12337b072bf40";

pub(crate) fn hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn inputs(fixture: &Value) -> Result<Value, String> {
    let mut recipe = fixture.clone();
    let cases = recipe["cases"]
        .as_array_mut()
        .filter(|cases| cases.len() == 84)
        .ok_or("Lottie oracle requires84 original observations")?;
    for case in cases {
        let case = case.as_object_mut().ok_or("invalid Lottie oracle case")?;
        if case.remove("expected").is_none() || case.remove("source_error_kind").is_none() {
            return Err("Lottie oracle case requires result and class observations".into());
        }
    }
    Ok(recipe)
}

pub fn fixture_identity(fixture: &Value) -> Result<(), String> {
    // Git text checkouts may translate newlines, never number tokens or Unicode.
    let original =
        include_str!("../../../tests/fixtures/native/lottie-validation.json").replace("\r\n", "\n");
    if hash(original.as_bytes()) != FIXTURE_HASH {
        return Err("pinned original Lottie fixture bytes drift".into());
    }
    let original: Value =
        serde_json::from_str(&original).map_err(|_| "invalid pinned original Lottie fixture")?;
    if inputs(fixture)? != inputs(&original)? {
        return Err("Lottie oracle recipe drift; refusing execution".into());
    }
    Ok(())
}

fn file_inputs(fixture: &Value) -> Result<Value, String> {
    let mut input = fixture.clone();
    let cases = input["validations"]
        .as_array_mut()
        .filter(|cases| cases.len() == 7)
        .ok_or("Lottie file oracle requires7 original observations")?;
    for case in cases {
        let case = case
            .as_object_mut()
            .ok_or("invalid Lottie file observation")?;
        for field in ["expected", "error_kind", "error"] {
            case.remove(field);
        }
    }
    Ok(input)
}

pub fn file_fixture_identity(fixture: &Value) -> Result<(), String> {
    let original =
        include_str!("../../../tests/fixtures/native/lottie-file.json").replace("\r\n", "\n");
    if hash(original.as_bytes())
        != "d6393b50d2f893e5e7733cc21162369d75665ebe0235a4da9a86ec59f9722356"
    {
        return Err("pinned original Lottie file fixture bytes drift".into());
    }
    let original: Value =
        serde_json::from_str(&original).map_err(|_| "invalid original Lottie file fixture")?;
    if file_inputs(fixture)? != file_inputs(&original)? {
        return Err("Lottie file oracle recipe drift; refusing execution".into());
    }
    Ok(())
}
