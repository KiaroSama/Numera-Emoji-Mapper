//! Pinned source image attribution is invariant to live reorder and refuses duplicate ambiguity.
use serde_json::Value;
#[test]
fn every_live_sticker_and_recorded_source_have_exactly_one_content_match() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/coin-map-identity.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_sha256"],
        "5fc72119e143b9d20af1f9280921d2503327f6c574e7b34e9d93a7ba1e27310e"
    );
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let want = input["want"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(name, hash)| (name.clone(), hash.as_u64().unwrap()))
            .collect::<Vec<_>>();
        let live = input["live"]
            .as_array()
            .unwrap()
            .iter()
            .map(|pair| {
                (
                    pair[0].as_str().unwrap().to_owned(),
                    pair[1].as_u64().unwrap(),
                )
            })
            .collect::<Vec<_>>();
        let result = _native::coin_map_identity::resolve(
            &want,
            &live,
            |name| {
                !input["missing"]
                    .as_array()
                    .is_some_and(|values| values.iter().any(|value| value == name))
            },
            |cid, hash| {
                if input["unreadable"]
                    .as_array()
                    .is_some_and(|values| values.iter().any(|value| value == cid))
                {
                    Err("controlled decode failure".into())
                } else {
                    Ok(hash)
                }
            },
        );
        if let Some(expected) = case["expected"].get("mapping") {
            assert_eq!(serde_json::json!(result.unwrap()), *expected);
        } else {
            assert_eq!(
                result.unwrap_err(),
                case["expected"]["error"].as_str().unwrap()
            );
        }
    }
}
