//! Pinned input recipe admission retained without deleted Python-source execution.
use serde_json::Value;
fn hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
const RECIPE: &str = "9a001c55123b24e6b559042a7e7c6c4f14d2b91107cf0863c1ac41773e173522";
pub fn fixture_identity(fixture: &Value) -> Result<(), String> {
    let mut recipe = fixture.clone();
    let cases = recipe
        .get_mut("cases")
        .and_then(Value::as_array_mut)
        .filter(|cases| cases.len() == 7)
        .ok_or("audit oracle requires7 original observations")?;
    for case in cases {
        let case = case.as_object_mut().ok_or("invalid original audit case")?;
        if !case.get("input").is_some_and(Value::is_object)
            || !case.get("expected").is_some_and(Value::is_object)
        {
            return Err("audit oracle requires input and expected objects".into());
        }
        case.remove("expected");
    }
    let bytes = serde_json::to_vec(&recipe).map_err(|_| "cannot encode audit oracle recipe")?;
    if hash(&bytes) != RECIPE {
        return Err("audit oracle recipe drift; refusing execution".into());
    }
    Ok(())
}

pub fn image_fixture_identity(fixture: &Value) -> Result<(), String> {
    let mut recipe = fixture.clone();
    let cases = recipe
        .get_mut("cases")
        .and_then(Value::as_array_mut)
        .filter(|cases| cases.len() == 7)
        .ok_or("audit image oracle requires7 original observations")?;
    for case in cases {
        let case = case
            .as_object_mut()
            .ok_or("invalid original audit image case")?;
        if !case
            .remove("expected")
            .is_some_and(|value| value.is_object())
        {
            return Err("audit image oracle requires expected objects".into());
        }
    }
    let bytes = serde_json::to_vec(&recipe).map_err(|_| "cannot encode audit image recipe")?;
    if hash(&bytes) != "34d71d3aefed0de9113778d35da964cf27b41b0c75f6d7da82bcec1fbdf2e3a2" {
        return Err("audit image oracle recipe drift; refusing execution".into());
    }
    Ok(())
}
