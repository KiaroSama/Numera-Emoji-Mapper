//! Deletion is complete only when each old set is independently confirmed missing.
use crate::telegram::SetState;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::time::Duration;

pub trait DeleteIo {
    fn delete(&mut self, name: &str) -> Result<(), String>;
    fn probe(&mut self, name: &str) -> SetState;
    fn save(&mut self, state: &Value) -> Result<(), String>;
    fn report(&mut self, text: &str);
    fn wait(&mut self, duration: Duration);
}
pub fn delete<I: DeleteIo>(
    names: Option<&[String]>,
    state: &mut Value,
    io: &mut I,
) -> Result<bool, String> {
    let Some(names) = names else {
        return Ok(true);
    };
    let rows = state
        .as_object_mut()
        .ok_or("rebuild deletion state must be an object")?
        .entry("deleted_old_packs")
        .or_insert_with(|| json!([]))
        .as_array()
        .ok_or("deleted_old_packs must be a list")?;
    let mut gone = rows
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_owned)
                .ok_or("deleted pack name must be a string")
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    for name in names {
        if gone.contains(name) {
            continue;
        }
        if let Err(error) = io.delete(name) {
            io.report(&format!("  (old pack {name}: {error})"));
        }
        match io.probe(name) {
            SetState::Unknown => return Err(format!("cannot confirm old pack {name} was deleted")),
            SetState::Missing => {
                gone.insert(name.clone());
                state["deleted_old_packs"] = json!(gone);
                io.save(state)?;
                io.report(&format!("  deleted old pack {name}"));
            }
            SetState::Exists => io.report(&format!("  old pack {name} still exists after delete")),
        }
        io.wait(Duration::from_millis(500));
    }
    Ok(names.iter().all(|name| gone.contains(name)))
}
