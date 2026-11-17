//! Native supplemental payload cross-product beyond individually mapped originals.
use numera_runtime as _native;
// Native supplemental: native/tests/bot_roster_parity.rs::original_bot_payloads_keep_quotes_fallback_batch_alignment_and_complete_copy_coverage
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
