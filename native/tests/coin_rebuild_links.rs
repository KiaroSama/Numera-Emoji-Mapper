//! Independent source observations distinguish optional notify from required final-link saves.
use serde_json::{Value, json};
use std::cell::RefCell;

#[test]
fn pinned_link_and_notification_effects_preserve_order_resends_and_failure_semantics() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/native/coin-rebuild-links.json"
    ))
    .unwrap();
    assert_eq!(
        fixture["source_sha256"]["coins/_dedup_map.py"],
        "5fc72119e143b9d20af1f9280921d2503327f6c574e7b34e9d93a7ba1e27310e"
    );
    assert_eq!(
        fixture["source_sha256"]["coins/rebuild_dedup.py"],
        "17771c6245dedf36d6c66deba99bb6405a624505962f74251f7f5a7f7e4eb54f"
    );
    for case in fixture["cases"].as_array().unwrap() {
        let input = &case["input"];
        let mut state = input["state"].clone();
        let events = RefCell::new(Vec::new());
        let output = RefCell::new(String::new());
        let announce = |packs: &[Value], note: &str, style: &str| {
            events
                .borrow_mut()
                .push(json!(["announce", packs, note, style]));
            if input["fail"] == "send" {
                Err("controlled announce refusal".into())
            } else {
                Ok(input["destination"].as_str().unwrap_or("[set]").to_owned())
            }
        };
        let save = |value: &Value| {
            events.borrow_mut().push(json!(["save", value]));
            if input["fail"] == "save" {
                Err("controlled state refusal".into())
            } else {
                Ok(())
            }
        };
        let report = |text: &str| {
            let mut output = output.borrow_mut();
            output.push_str(text);
            output.push('\n');
        };
        let result = if input["operation"] == "notify" {
            _native::coin_rebuild_links::notify(
                &mut state,
                "cryptoemoji2_by_YourEmojiBot",
                || {
                    announce(
                        &[json!({"name":"cryptoemoji2_by_YourEmojiBot","title":"Fixture 2"})],
                        "",
                        "cards",
                    )
                },
                save,
                report,
            )
        } else {
            _native::coin_rebuild_links::final_links(
                &mut state,
                "Fixture",
                |packs, note| announce(packs, note, "list"),
                save,
                report,
            )
        };
        match result {
            Ok(()) => assert!(case["expected"]["error"].is_null(), "{}", input["name"]),
            Err(error) => assert_eq!(json!(error), case["expected"]["error"], "{}", input["name"]),
        }
        // The actual lease is tested at the CLI seam, not reproduced by this policy fixture.
        let expected = case["expected"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|e| e[0] != "lock" && e[0] != "unlock")
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(state, case["expected"]["state"], "{}", input["name"]);
        assert_eq!(events.into_inner(), expected, "{}", input["name"]);
        assert_eq!(
            json!(output.into_inner()),
            case["expected"]["output"],
            "{}",
            input["name"]
        );
    }
}
