//! Immutable historical fixture/recipe integrity only; no source execution or interpreter.
use serde_json::Value;
fn hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn fixtures() -> Result<(Value, Value), String> {
    let mut values = Vec::new();
    for (text, expected) in [
        (
            include_str!("../../../tests/fixtures/native/static-convert.json"),
            "3d9ee8fa644edb16854b0c2fc8f323c32a8250179c0a4c007572178bba34ec50",
        ),
        (
            include_str!("../../../tests/fixtures/native/provider-image.json"),
            "ee26433758106a054b876a46f83e6f2934e1590584949cad875dc4c53958752a",
        ),
    ] {
        let text = text.replace("\r\n", "\n");
        if hash(text.as_bytes()) != expected {
            return Err("pinned original static/provider fixture bytes drift".into());
        }
        values.push(
            serde_json::from_str(&text).map_err(|_| "invalid original static/provider fixture")?,
        );
    }
    let provider = values.pop().ok_or("missing provider fixture")?;
    let static_fixture = values.pop().ok_or("missing static fixture")?;
    Ok((static_fixture, provider))
}

fn recipe(fixture: &Value, count: usize) -> Result<Value, String> {
    let mut value = fixture.clone();
    let cases = value["cases"]
        .as_array_mut()
        .filter(|cases| cases.len() == count)
        .ok_or("static oracle requires its exact original case count")?;
    for case in cases {
        let case = case.as_object_mut().ok_or("invalid static oracle case")?;
        case.remove("expected");
        case.remove("error");
    }
    Ok(value)
}

pub fn fixture_identity(static_fixture: &Value, provider_fixture: &Value) -> Result<(), String> {
    let (original_static, original_provider) = fixtures()?;
    if recipe(static_fixture, 5)? != recipe(&original_static, 5)?
        || recipe(provider_fixture, 7)? != recipe(&original_provider, 7)?
    {
        return Err("static oracle recipe drift; refusing execution".into());
    }
    Ok(())
}
