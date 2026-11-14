//! Pin the original whole-roster setup, never the documents or Markdown it must regenerate.
use serde_json::Value;

const FIXTURE: &str = "d173116:pack_manifest.build_pack";
const RECIPE_HASH: &str = "7cd9e9a291b1ae273ca8cd2fcca06e650d32dabd8fe75b5ed4e72377f54f761a";

pub(crate) fn inputs(fixture: &Value) -> Result<Value, String> {
    crate::source_fixture::cases(fixture, FIXTURE, 4)?;
    let mut inputs = fixture.clone();
    for case in inputs["cases"]
        .as_array_mut()
        .ok_or("roster pack cases must be a list")?
    {
        let case = case
            .as_object_mut()
            .ok_or("roster pack case must be an object")?;
        for observation in ["expected", "error", "markdown"] {
            case.remove(observation);
        }
    }
    let index = inputs
        .get_mut("index_case")
        .and_then(Value::as_object_mut)
        .ok_or("roster pack requires index_case")?;
    for observation in ["documents", "expected", "markdown"] {
        index.remove(observation);
    }
    Ok(inputs)
}

pub(crate) fn identity(fixture: &Value) -> Result<(), String> {
    // Shared recipe admission also checks unique nonempty names and exactly one outcome.
    let mut projection = inputs(fixture)?;
    for case in projection["cases"]
        .as_array_mut()
        .ok_or("roster pack cases must be a list")?
    {
        case["expected"] = Value::Null;
    }
    crate::source_fixture::recipe(&projection, FIXTURE, 4, RECIPE_HASH)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn fixture() -> Value {
        serde_json::from_str(include_str!(
            "../../tests/fixtures/backend/roster-pack.json"
        ))
        .unwrap()
    }
    #[test]
    fn original_recipe_keeps_input_identity_but_excludes_every_generated_observation() {
        let original = fixture();
        identity(&original).unwrap();
        let projected = inputs(&original).unwrap();
        assert!(
            projected["cases"]
                .as_array()
                .unwrap()
                .iter()
                .all(|case| !case.as_object().unwrap().contains_key("expected")
                    && !case.as_object().unwrap().contains_key("markdown"))
        );
        assert!(
            !projected["index_case"]
                .as_object()
                .unwrap()
                .contains_key("documents")
        );
        for mutation in [
            "result",
            "markdown",
            "documents",
            "index-result",
            "index-markdown",
        ] {
            let mut changed = original.clone();
            match mutation {
                "result" => changed["cases"][0]["expected"]["count"] = json!(999),
                "markdown" => changed["cases"][0]["markdown"] = json!("target-derived"),
                "documents" => {
                    changed["index_case"]["documents"] = json!([{"target-derived":true}])
                }
                "index-result" => changed["index_case"]["expected"]["emoji_count"] = json!(999),
                "index-markdown" => changed["index_case"]["markdown"] = json!("target-derived"),
                _ => unreachable!(),
            }
            identity(&changed).unwrap();
            assert_eq!(inputs(&changed).unwrap(), projected, "{mutation}");
        }
    }
    #[test]
    fn coordinated_inputs_results_root_order_and_foreign_index_data_cannot_change_recipe() {
        let original = fixture();
        for mutation in [
            "input-result",
            "name",
            "order",
            "root",
            "family",
            "old",
            "inputs",
        ] {
            let mut changed = original.clone();
            match mutation {
                "input-result" => {
                    changed["cases"][0]["record"]["title"] = json!("altered");
                    changed["cases"][0]["expected"]["title"] = json!("altered");
                }
                "name" => changed["cases"][0]["name"] = json!("new-name"),
                "order" => changed["cases"].as_array_mut().unwrap().swap(0, 1),
                "root" => changed["extra"] = json!(true),
                "family" => changed["index_case"]["family"] = json!("coins"),
                "old" => {
                    changed["index_case"]["old"]["inputs"]["ticker_to_id.json"] = json!([9, 9])
                }
                "inputs" => changed["index_case"]["inputs"]["catalog.db"] = json!([7, 7]),
                _ => unreachable!(),
            }
            assert!(
                identity(&changed).unwrap_err().contains("recipe drift"),
                "{mutation}"
            );
        }
    }
}
