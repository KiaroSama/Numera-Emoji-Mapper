// Ported from native/tests/coin_replace_recovery.rs at 45a96f9de70da8429b83c047e0626b6ed6694b20.
// Original source SHA256: 62f2bb5bf93e0926a58f8986df13597e6c7529efe8b24d747ab639cf1d2ebefc.
// Original test identities and behavioral assertions retained against the production crate.
extern crate numera_runtime as _native;

// Independent source recovery requires matching targets and image proof before any map edit.
use _native::coin_replace_recovery::{RecoveryIo, recover};
use serde_json::{Map, Value, json};
struct Fixture {
    case: Value,
    actions: Vec<String>,
    mapping: Map<String, Value>,
}
impl RecoveryIo for Fixture {
    fn live(&mut self, _name: &str) -> Result<Vec<Value>, String> {
        self.actions.push("read".into());
        if self.case["read_error"] == true {
            return Err("controlled unknown".into());
        }
        Ok(self.case["after"]
            .as_array()
            .unwrap()
            .iter()
            .map(|cid| json!({"custom_emoji_id":cid,"file_id":cid}))
            .collect())
    }
    fn proof(&mut self, _sticker: &Value, _hash: u64) -> Result<bool, String> {
        self.actions.push("proof".into());
        Ok(self.case["ours"].as_bool().unwrap())
    }
    fn promote(&mut self, _intent: &Value) -> Result<(), String> {
        self.actions.push("promote".into());
        Ok(())
    }
    fn repoint(&mut self, old: &str, new: &str) -> Result<(), String> {
        self.actions.push("repoint".into());
        for value in self.mapping.values_mut() {
            if value.as_str() == Some(old) {
                *value = json!(new);
            }
        }
        Ok(())
    }
    fn clear(&mut self) -> Result<(), String> {
        self.actions.push("clear".into());
        Ok(())
    }
    fn clean(&mut self, _intent: &Value) -> Result<(), String> {
        self.actions.push("cleanup".into());
        Ok(())
    }
}
#[test]
fn source_recovery_refuses_wrong_targets_unknown_art_and_repoints_idempotently() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/coin-replace-recovery.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_sha256"],
        "86f17862ebe5019e05965fa7aa30380a20226b3632f192f051525290ae7f7031"
    );
    for case in fixture["cases"].as_array().unwrap() {
        let mut io = Fixture {
            case: case.clone(),
            actions: vec![],
            mapping: case["initial"].as_object().unwrap().clone(),
        };
        assert_eq!(
            recover(Some(&case["intent"]), "map", "state", &mut io).unwrap(),
            case["expected"]["result"].as_bool().unwrap()
        );
        assert_eq!(json!(io.actions), case["expected"]["actions"]);
        assert_eq!(json!(io.mapping), case["expected"]["mapping"]);
    }
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
