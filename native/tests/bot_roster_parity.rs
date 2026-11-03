//! Original bot and roster public-result assertions retain independent pre-port source outputs.
//! Source regeneration is a separate unfinished gate; pinned bytes alone never prove it.
use serde_json::Value;
use sha2::{Digest, Sha256};

#[test]
fn bot_oracle_rejects_coordinated_input_or_name_drift_before_source_execution() {
    let original: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/backend/bot.json")).unwrap();
    _native::source_bot_oracle::fixture_identity(&original).unwrap();
    let mut reordered = original.clone();
    reordered.sort_all_objects();
    _native::source_bot_oracle::fixture_identity(&reordered).unwrap();
    for mutation in ["consistent-result", "name", "order"] {
        let mut changed = original.clone();
        match mutation {
            "consistent-result" => {
                changed["cases"][4]["text"] = serde_json::json!("111111111111111");
                changed["cases"][4]["expected"] = serde_json::json!(["111111111111111"]);
            }
            "name" => changed["cases"][0]["name"] = serde_json::json!("uniquely-renamed"),
            "order" => changed["cases"].as_array_mut().unwrap().swap(0, 1),
            _ => unreachable!(),
        }
        assert!(
            _native::source_bot_oracle::fixture_identity(&changed)
                .unwrap_err()
                .contains("recipe drift"),
            "{mutation}"
        );
    }
}

#[test]
fn roster_oracle_pins_live_id_history_and_case_recipe_before_replay() {
    let original: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/backend/roster.json")).unwrap();
    _native::source_roster_oracle::fixture_identity(&original).unwrap();
    let mut reordered = original.clone();
    reordered.sort_all_objects();
    _native::source_roster_oracle::fixture_identity(&reordered).unwrap();
    for mutation in ["consistent-result", "name", "order", "extra"] {
        let mut changed = original.clone();
        match mutation {
            "consistent-result" => {
                changed["cases"][0]["st"]["custom_emoji_id"] = serde_json::json!("555555555");
                changed["cases"][0]["expected"]["custom_emoji_id"] = serde_json::json!("555555555");
            }
            "name" => changed["cases"][0]["name"] = serde_json::json!("uniquely-renamed"),
            "order" => changed["cases"].as_array_mut().unwrap().swap(0, 1),
            "extra" => changed["unchecked-metadata"] = serde_json::json!(true),
            _ => unreachable!(),
        }
        assert!(
            _native::source_roster_oracle::fixture_identity(&changed)
                .unwrap_err()
                .contains("recipe drift"),
            "{mutation}"
        );
    }
}

#[test]
fn original_bot_assertion_sources_refuse_forged_bytes_before_runtime_access() {
    use _native::source_git_inventory::Source;
    let sources = [
        (
            "tests/reference/emoji_bot.py",
            "0ae857dd8607f1e054c6825b3f1e6ecd19eaf2347cfc69a330ae0f6aefd0071f",
        ),
        (
            "tests/test_emoji_bot.py",
            "609454dce3ce7624975d4e56bfa4b05548dbc7ccfc3a2e0c48b8f87d923a5e19",
        ),
    ]
    .map(|(path, hash)| Source {
        baseline_revision: "1a36a968b006236bd0665a1da65c541938c06061".into(),
        path: path.into(),
        sha256: hash.into(),
        text: "raise AssertionError('must not execute')".into(),
    });
    assert!(
        _native::source_bot_tests::observe(&sources[0], &sources[1])
            .unwrap_err()
            .contains("source identity drift")
    );
}

#[test]
fn bot_command_sources_refuse_before_workspace_or_runtime_access() {
    use _native::source_git_inventory::Source;
    let sources = [
        (
            "tests/reference/emoji_bot.py",
            "0ae857dd8607f1e054c6825b3f1e6ecd19eaf2347cfc69a330ae0f6aefd0071f",
        ),
        (
            "tests/test_emoji_bot.py",
            "609454dce3ce7624975d4e56bfa4b05548dbc7ccfc3a2e0c48b8f87d923a5e19",
        ),
        (
            "emojikit/cli_env.py",
            "9e4c0a045f031bffd23fe5ff9e51a132363e9952a1cdde47d071bfc81234e887",
        ),
        (
            "tests/reference/packstate.py",
            "d6539619a473dd86d7706bb929a8cad759d29639cd8006a1e083a0504693f9e0",
        ),
    ]
    .map(|(path, hash)| Source {
        baseline_revision: "1a36a968b006236bd0665a1da65c541938c06061".into(),
        path: path.into(),
        sha256: hash.into(),
        text: "raise AssertionError('must not execute')".into(),
    });
    let refs = sources.iter().collect::<Vec<_>>();
    let missing = std::path::Path::new("must-not-be-inspected");
    assert!(
        _native::source_bot_command_tests::observe(missing, missing, &refs)
            .unwrap_err()
            .contains("source identity drift")
    );
    assert!(_native::source_bot_command_tests::source_identity(&[]).is_err());
}

#[test]
fn original_bot_extraction_keeps_entity_source_order_and_refuses_markup() {
    use _native::bot_payloads::extract;
    use serde_json::json;
    let entities = |ids: &[&str]| {
        ids.iter()
            .map(|id| {
                json!({
                    "type":"custom_emoji","custom_emoji_id":id
                })
            })
            .collect::<Vec<_>>()
    };
    for (message, expected) in [
        (json!({"text":"hi"}), vec![]),
        (
            json!({"entities":entities(&["111","222","111","333"])}),
            vec!["111", "222", "333"],
        ),
        (json!({"caption_entities":entities(&["9"])}), vec!["9"]),
        (
            json!({"quote":{"text":"wave warn stop","entities":entities(&["111","222","333"])}}),
            vec!["111", "222", "333"],
        ),
        (
            json!({"external_reply":{"quote":{"entities":entities(&["444","555"])}}}),
            vec!["444", "555"],
        ),
        (
            json!({"entities":entities(&["111"]),"quote":{"entities":entities(&["222","111"])}}),
            vec!["111", "222"],
        ),
        (
            json!({"entities":entities(&["1\"><b>x</b>","222"])}),
            vec!["222"],
        ),
        (
            json!({"entities":entities(&["5789012345678901234"])}),
            vec!["5789012345678901234"],
        ),
    ] {
        assert_eq!(extract(&message), expected);
    }
    // The native public sink refuses malformed IDs, rather than exposing the original private span.
    let bad = "1\"><b>x</b>";
    let escaped = _native::bot_payloads::escape(bad);
    assert!(!escaped.contains("<b>"));
    assert!(escaped.contains("&lt;b&gt;"));
    assert!(_native::bot_payloads::payloads(&[bad.into()], &json!({}), true).is_err());
}

#[test]
fn original_bot_typed_lists_preserve_whole_message_grammar_and_unicode_decimals() {
    use _native::bot_payloads::typed;
    let a = "5893098741073715471";
    let b = "5893098741073715472";
    for text in [
        a.into(),
        format!("{a}\n{b}"),
        format!("{a}, {b}"),
        format!("{a},{b}"),
        format!("{a} {a}"),
    ] {
        assert_eq!(
            typed(&text),
            if text.contains(b) {
                vec![a, b]
            } else {
                vec![a]
            }
        );
    }
    for text in [
        format!("my chat id is {a} ok"),
        "hello".into(),
        "42".into(),
        "".into(),
        "  ".into(),
        format!(",{a}"),
        format!(";{a}"),
        format!("{a},"),
        format!("{a};"),
        format!("{a}, \n"),
    ] {
        assert!(typed(&text).is_empty(), "{text:?}");
    }
    let unicode = "۵۸۹۳۰۹۸۷۴۱۰۷۳۷۱۵۴۷۱";
    assert_eq!(
        typed(&format!("\u{1c}{unicode}\u{1f};, {a}\u{1e}")),
        vec![unicode, a]
    );
    assert!(typed(&"²".repeat(19)).is_empty());
    assert!(typed(&"9".repeat(26)).is_empty());
}

#[test]
fn original_bot_payloads_keep_quotes_fallback_batch_alignment_and_complete_copy_coverage() {
    use _native::bot_payloads::payloads;
    use serde_json::json;
    let empty = payloads(&[], &json!({}), true).unwrap();
    assert_eq!(empty.as_array().unwrap().len(), 1);
    assert!(empty[0][0].as_str().unwrap().contains("No premium"));
    assert_eq!(empty[0][1]["inline_keyboard"], json!([]));
    let rich = payloads(&["5899".into()], &json!({"5899":"👋"}), true).unwrap();
    assert!(
        rich[0][0]
            .as_str()
            .unwrap()
            .contains("<tg-emoji emoji-id=\"5899\">👋</tg-emoji> <code>5899</code>")
    );
    let plain = payloads(&["5899".into()], &json!({"5899":"👋"}), false).unwrap();
    assert!(
        plain[0][0]
            .as_str()
            .unwrap()
            .contains("👋 <code>5899</code>")
    );
    assert!(!plain[0][0].as_str().unwrap().contains("tg-emoji"));
    let fallback = payloads(&["777".into()], &json!({}), true).unwrap();
    assert!(
        fallback[0][0]
            .as_str()
            .unwrap()
            .contains("<tg-emoji emoji-id=\"777\">⭐</tg-emoji>")
    );
    let five = (1000..1005).map(|id| id.to_string()).collect::<Vec<_>>();
    let small = payloads(&five, &json!({}), true).unwrap();
    assert_eq!(
        small[0][1]["inline_keyboard"][0][0]["copy_text"]["text"],
        format!("{}\n", five.join("\n"))
    );
    for id in &five {
        assert_eq!(
            small[0][0]
                .as_str()
                .unwrap()
                .matches(&format!("<code>{id}</code>"))
                .count(),
            1
        );
    }
    for ids in [
        vec!["1".into(), "2".into()],
        vec!["111111111".into(); 3],
        vec!["111111111".into()],
        (0..12)
            .map(|i| (1_000_000_000_000_000_000_u64 + i).to_string())
            .collect(),
        vec!["9".repeat(25); 12],
        vec!["9".repeat(25); 50],
        (0..30)
            .map(|i| (1_000_000_000_000_000_000_u64 + i).to_string())
            .collect(),
        (0..50)
            .map(|i| (1_000_000_000_000_000_000_u64 + i).to_string())
            .collect(),
        (0..400)
            .map(|i| (1_000_000_000_000_000_000_u64 + i).to_string())
            .collect(),
    ] {
        let rich = payloads(&ids, &json!({}), true).unwrap();
        let plain = payloads(&ids, &json!({}), false).unwrap();
        let rich = rich.as_array().unwrap();
        assert_eq!(rich.len(), plain.as_array().unwrap().len());
        if ids.len() == 50 && ids[0].len() == 19 {
            assert!(rich.len() < 5);
        }
        if ids.len() == 400 {
            assert!(rich.len() > 1);
            assert!(rich[0][0].as_str().unwrap().contains("part 1/"));
        }
        let mut all = Vec::new();
        for (index, row) in rich.iter().enumerate() {
            let text = row[0].as_str().unwrap();
            assert!(text.chars().count() <= 4096);
            assert_eq!(text.matches("<blockquote expandable>").count(), 1);
            assert!(!text.contains("<pre>") && !text.contains("IDs only"));
            let buttons = row[1]["inline_keyboard"].as_array().unwrap();
            assert!(!buttons.is_empty());
            if ids.len() <= 3 {
                assert_eq!(buttons.len(), 1);
            }
            if ids.len() == 12 && ids[0].len() == 25 {
                assert!(buttons.len() > 1);
            }
            if ids.len() == 30 && index == 0 {
                assert!(buttons.len() > 1);
            }
            assert_eq!(row[1], plain[index][1]);
            for button in buttons {
                let copied = button[0]["copy_text"]["text"].as_str().unwrap();
                assert!(copied.chars().count() <= 256 && copied.ends_with('\n'));
                for id in copied.trim_end_matches('\n').split('\n') {
                    assert!(text.contains(&format!("<code>{id}</code>")));
                    all.push(id.to_owned());
                }
            }
        }
        assert_eq!(all, ids);
        if ids.iter().collect::<std::collections::BTreeSet<_>>().len() == ids.len() {
            let text = rich
                .iter()
                .map(|row| row[0].as_str().unwrap())
                .collect::<Vec<_>>()
                .join("\n");
            for id in ids {
                assert_eq!(text.matches(&format!("<code>{id}</code>")).count(), 1);
            }
        }
    }
}

#[test]
fn shared_worker_emoji_messages_preserve_every_extracted_and_typed_id() {
    let bytes = include_bytes!("../../tests/fixtures/contracts/emoji_messages.json");
    let hash = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(
        hash,
        "25e3883b746f98dd366361536b327a143583d8fedde2a7003cbb9d50a1e3d7db"
    );
    let cases: Value = serde_json::from_slice(bytes).unwrap();
    let cases = cases.as_array().unwrap();
    assert_eq!(cases.len(), 7);
    for case in cases {
        let message = &case["message"];
        assert_eq!(
            serde_json::json!(_native::bot_payloads::extract(message)),
            case["ids"],
            "{}",
            case["name"]
        );
        assert_eq!(
            serde_json::json!(_native::bot_payloads::typed(
                message["text"].as_str().unwrap_or("")
            )),
            case["typed"],
            "{}",
            case["name"]
        );
    }
}

#[test]
fn original_bot_payload_and_roster_row_results_match_every_source_variant() {
    let corpora: [(&[u8], &str, &str, usize); 2] = [
        (
            include_bytes!("../../tests/fixtures/backend/bot.json"),
            "00f3a5e7f703fc95fb027bb6ad1278a428c98f801f6619214a1a5933b37c04ba",
            "d173116:emoji_bot.pure_payloads",
            6,
        ),
        (
            include_bytes!("../../tests/fixtures/backend/roster.json"),
            "039eee42bf316e1f8ad21f6d6f2dca7d8a2be99970381dadfaa9583256f907b1",
            "d173116:pack_manifest._row",
            4,
        ),
    ];
    for (bytes, expected_hash, source, count) in corpora {
        let hash = Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(hash, expected_hash, "independent source fixture drift");
        let document: Value = serde_json::from_slice(bytes).unwrap();
        for case in _native::source_fixture::cases(&document, source, count).unwrap() {
            let result = if source == "d173116:emoji_bot.pure_payloads" {
                _native::bot_payloads::execute(case)
            } else {
                _native::roster::row(case)
            };
            assert_eq!(result.unwrap(), case["expected"], "{}", case["name"]);
        }
    }
}
