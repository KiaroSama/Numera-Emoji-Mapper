//! Original Telegram transport and bot methods as independent interpreter-free Rust cases.
#[path = "baseline_telegram_support/mod.rs"]
mod support;
use numera_runtime as rt;
use rt::telegram::{ApiError, SetState};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;
use support::*;

mod html_boundary {
    use super::*;
    // tests.test_emoji_bot.TestIdsReachingTheHtmlSinkAreDecimalOnly.test_an_id_with_markup_is_dropped_at_the_boundary
    #[test]
    fn test_an_id_with_markup_is_dropped_at_the_boundary() {
        assert_eq!(
            rt::bot_payloads::extract(&entities(&["1\"><b>x</b>", "222"])),
            ids(&["222"])
        );
    }
    // tests.test_emoji_bot.TestIdsReachingTheHtmlSinkAreDecimalOnly.test_the_rendered_message_contains_no_injected_markup
    #[test]
    fn test_the_rendered_message_contains_no_injected_markup() {
        let bad = "1\"><b>x</b>";
        let escaped = rt::bot_payloads::escape(bad);
        assert!(!escaped.contains("<b>"));
        assert!(escaped.contains("&lt;b&gt;"));
        assert!(rt::bot_payloads::payloads(&ids(&[bad]), &json!({}), true).is_err());
    }
    // tests.test_emoji_bot.TestIdsReachingTheHtmlSinkAreDecimalOnly.test_ordinary_ids_are_untouched
    #[test]
    fn test_ordinary_ids_are_untouched() {
        assert_eq!(
            rt::bot_payloads::extract(&entities(&["5789012345678901234"])),
            ids(&["5789012345678901234"])
        );
    }
}
mod extract {
    use super::*;
    // tests.test_emoji_bot.TestExtract.test_none
    #[test]
    fn test_none() {
        assert_eq!(
            rt::bot_payloads::extract(&json!({"text":"hi"})),
            Vec::<String>::new()
        );
    }
    // tests.test_emoji_bot.TestExtract.test_order_and_dedup
    #[test]
    fn test_order_and_dedup() {
        assert_eq!(
            rt::bot_payloads::extract(&entities(&["111", "222", "111", "333"])),
            ids(&["111", "222", "333"])
        );
    }
    // tests.test_emoji_bot.TestExtract.test_caption_entities
    #[test]
    fn test_caption_entities() {
        assert_eq!(
            rt::bot_payloads::extract(
                &json!({"caption_entities":[{"type":"custom_emoji","custom_emoji_id":"9"}]})
            ),
            ids(&["9"])
        );
    }
    // tests.test_emoji_bot.TestExtract.test_quote_entities_are_scanned
    #[test]
    fn test_quote_entities_are_scanned() {
        let m = json!({"text":"reply text","quote":{"text":"wave warn stop","position":0,"entities":[{"type":"custom_emoji","offset":0,"length":2,"custom_emoji_id":"111"},{"type":"custom_emoji","offset":5,"length":2,"custom_emoji_id":"222"},{"type":"custom_emoji","offset":10,"length":2,"custom_emoji_id":"333"}]}});
        assert_eq!(rt::bot_payloads::extract(&m), ids(&["111", "222", "333"]));
    }
    // tests.test_emoji_bot.TestExtract.test_external_reply_quote_entities_are_scanned
    #[test]
    fn test_external_reply_quote_entities_are_scanned() {
        let m = json!({"external_reply":{"quote":{"text":"aa bb","entities":[{"type":"custom_emoji","custom_emoji_id":"444"},{"type":"custom_emoji","custom_emoji_id":"555"}]}}});
        assert_eq!(rt::bot_payloads::extract(&m), ids(&["444", "555"]));
    }
    // tests.test_emoji_bot.TestExtract.test_quote_plus_own_entities_dedup_and_order
    #[test]
    fn test_quote_plus_own_entities_dedup_and_order() {
        let m = json!({"entities":[{"type":"custom_emoji","custom_emoji_id":"111"}],"quote":{"entities":[{"type":"custom_emoji","custom_emoji_id":"222"},{"type":"custom_emoji","custom_emoji_id":"111"}]}});
        assert_eq!(rt::bot_payloads::extract(&m), ids(&["111", "222"]));
    }
}
mod build_payloads {
    use super::*;
    // tests.test_emoji_bot.TestBuildPayloads.test_empty
    #[test]
    fn test_empty() {
        let p = payload(&[], json!({}), true);
        assert_eq!(p.as_array().unwrap().len(), 1);
        assert!(p[0][0].as_str().unwrap().contains("No premium"));
        assert_eq!(p[0][1]["inline_keyboard"], json!([]));
    }
    // tests.test_emoji_bot.TestBuildPayloads.test_rich_renders_real_premium_emoji
    #[test]
    fn test_rich_renders_real_premium_emoji() {
        let p = payload(&ids(&["5899"]), json!({"5899":"👋"}), true);
        assert!(
            p[0][0]
                .as_str()
                .unwrap()
                .contains("<tg-emoji emoji-id=\"5899\">👋</tg-emoji> <code>5899</code>")
        );
    }
    // tests.test_emoji_bot.TestBuildPayloads.test_single_collapsed_quote_only
    #[test]
    fn test_single_collapsed_quote_only() {
        let p = payload(&ids(&["1", "2"]), json!({}), true);
        let t = p[0][0].as_str().unwrap();
        assert_eq!(t.matches("<blockquote expandable>").count(), 1);
        assert!(!t.contains("<pre>"));
        assert!(!t.contains("IDs only"));
    }
    // tests.test_emoji_bot.TestBuildPayloads.test_copy_all_button_copies_every_id
    #[test]
    fn test_copy_all_button_copies_every_id() {
        let ids = (1000..1005).map(|i| i.to_string()).collect::<Vec<_>>();
        let p = payload(&ids, json!({}), true);
        let rows = p[0][1]["inline_keyboard"].as_array().unwrap();
        assert_eq!(
            rows.last().unwrap()[0]["copy_text"]["text"],
            format!("{}\n", ids.join("\n"))
        );
        for id in ids {
            assert_eq!(
                p[0][0]
                    .as_str()
                    .unwrap()
                    .matches(&format!("<code>{id}</code>"))
                    .count(),
                1
            );
        }
    }
    // tests.test_emoji_bot.TestBuildPayloads.test_50_ids_batched_by_message_limit_not_button_limit
    #[test]
    fn test_50_ids_batched_by_message_limit_not_button_limit() {
        let ids = series(50);
        let p = payload(&ids, json!({}), true);
        assert!(p.as_array().unwrap().len() < 5);
        for row in p.as_array().unwrap() {
            let buttons = row[1]["inline_keyboard"].as_array().unwrap();
            assert!(!buttons.is_empty());
            for button in buttons {
                let piece = button[0]["copy_text"]["text"].as_str().unwrap();
                assert!(piece.chars().count() <= 256);
                assert!(piece.ends_with('\n'));
                for id in piece.trim_end_matches('\n').split('\n') {
                    assert!(
                        row[0]
                            .as_str()
                            .unwrap()
                            .contains(&format!("<code>{id}</code>"))
                    );
                }
            }
        }
        assert_eq!(copied(&p), ids);
    }
    // tests.test_emoji_bot.TestBuildPayloads.test_copy_buttons_chunked_under_limit
    #[test]
    fn test_copy_buttons_chunked_under_limit() {
        let p = payload(&series(30), json!({}), true);
        let b = p[0][1]["inline_keyboard"].as_array().unwrap();
        assert!(b.len() > 1);
        for row in b {
            assert!(
                row[0]["copy_text"]["text"]
                    .as_str()
                    .unwrap()
                    .chars()
                    .count()
                    <= 256
            );
        }
    }
    // tests.test_emoji_bot.TestBuildPayloads.test_plain_uses_fallback_char_only
    #[test]
    fn test_plain_uses_fallback_char_only() {
        let p = payload(&ids(&["5899"]), json!({"5899":"👋"}), false);
        let t = p[0][0].as_str().unwrap();
        assert!(t.contains("👋 <code>5899</code>"));
        assert!(!t.contains("tg-emoji"));
    }
    // tests.test_emoji_bot.TestBuildPayloads.test_rich_default_and_missing_label_uses_star
    #[test]
    fn test_rich_default_and_missing_label_uses_star() {
        let p =
            rt::bot_payloads::execute(&json!({"operation":"payloads","ids":["777"],"labels":{}}))
                .unwrap();
        assert!(
            p[0][0]
                .as_str()
                .unwrap()
                .contains("<tg-emoji emoji-id=\"777\">⭐</tg-emoji>")
        );
    }
    // tests.test_emoji_bot.TestBuildPayloads.test_large_list_splits_into_multiple_messages
    #[test]
    fn test_large_list_splits_into_multiple_messages() {
        let ids = series(400);
        let p = payload(&ids, json!({}), true);
        assert!(p.as_array().unwrap().len() > 1);
        let texts = p
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r[0].as_str().unwrap())
            .collect::<Vec<_>>();
        for t in &texts {
            assert!(t.chars().count() <= 4096);
        }
        assert!(texts[0].contains("part 1/"));
        let joined = texts.join("\n");
        for id in ids {
            assert_eq!(joined.matches(&format!("<code>{id}</code>")).count(), 1);
        }
    }
    // tests.test_emoji_bot.TestBuildPayloads.test_rich_and_plain_batch_alignment
    #[test]
    fn test_rich_and_plain_batch_alignment() {
        let ids = series(400);
        let rich = payload(&ids, json!({}), true);
        let plain = payload(&ids, json!({}), false);
        assert_eq!(
            rich.as_array().unwrap().len(),
            plain.as_array().unwrap().len()
        );
        assert_eq!(copied(&rich), copied(&plain));
    }
}
mod typed_ids {
    use super::*;
    // tests.test_emoji_bot.TestIdsTypedAsText.test_every_shape_people_actually_paste
    #[test]
    fn test_every_shape_people_actually_paste() {
        for (s, want) in [
            (A.into(), ids(&[A])),
            (format!("{A}\n{B}"), ids(&[A, B])),
            (format!("{A}, {B}"), ids(&[A, B])),
            (format!("{A},{B}"), ids(&[A, B])),
            (format!("{A} {A}"), ids(&[A])),
        ] {
            assert_eq!(rt::bot_payloads::typed(&s), want);
        }
    }
    // tests.test_emoji_bot.TestIdsTypedAsText.test_prose_holding_a_long_number_is_not_a_lookup
    #[test]
    fn test_prose_holding_a_long_number_is_not_a_lookup() {
        for text in [
            format!("my chat id is {A} ok"),
            "hello".into(),
            "42".into(),
            "".into(),
            "  ".into(),
        ] {
            assert!(rt::bot_payloads::typed(&text).is_empty(), "{text:?}");
        }
    }
    // tests.test_emoji_bot.TestIdsTypedAsText.test_it_resolves_the_ids_rather_than_trusting_them
    #[test]
    fn test_it_resolves_the_ids_rather_than_trusting_them() {
        let server = Server::new(vec![
            reply(
                "getCustomEmojiStickers",
                ok(json!([{"custom_emoji_id":A,"emoji":"✅"}])),
            ),
            reply(
                "getCustomEmojiStickers",
                ok(json!([{"custom_emoji_id":A,"emoji":"✅"}])),
            ),
            reply("sendMessage", ok(json!({"message_id":1}))),
        ]);
        let root = Root::new();
        rt::emoji_bot::handle(
            &server.telegram(),
            42,
            &message(900, &format!("{A} {B}"), "private"),
            &BTreeSet::from([900]),
            &BTreeSet::new(),
            &root.log(),
        )
        .unwrap();
        let requests = server.finish();
        let sent = requests.last().unwrap().form["text"].clone();
        assert!(sent.contains("does not know"));
        assert!(sent.contains(A) && sent.contains(B));
        assert!(sent.contains(&format!("emoji-id=\"{A}\"")));
        assert!(!sent.contains(&format!("emoji-id=\"{B}\"")));
    }
    // tests.test_emoji_bot.TestIdsTypedAsText.test_when_nothing_resolves_it_says_so_once
    #[test]
    fn test_when_nothing_resolves_it_says_so_once() {
        let server = Server::new(vec![
            reply("getCustomEmojiStickers", ok(json!([]))),
            reply("sendMessage", ok(json!({}))),
        ]);
        let root = Root::new();
        rt::emoji_bot::handle(
            &server.telegram(),
            42,
            &message(900, A, "private"),
            &BTreeSet::from([900]),
            &BTreeSet::new(),
            &root.log(),
        )
        .unwrap();
        let requests = server.finish();
        let sends = requests
            .iter()
            .filter(|r| r.method == "sendMessage")
            .collect::<Vec<_>>();
        assert_eq!(sends.len(), 1);
        assert!(sends[0].form["text"].contains("does not know"));
    }
}
mod access_control {
    use super::*;
    fn handle(
        update: Value,
        allowed: BTreeSet<i64>,
        channels: BTreeSet<i64>,
        replies: Vec<Reply>,
    ) -> Vec<Request> {
        let root = Root::new();
        let server = Server::new(replies);
        rt::emoji_bot::handle(
            &server.telegram(),
            42,
            &update,
            &allowed,
            &channels,
            &root.log(),
        )
        .unwrap();
        server.finish()
    }
    fn sent() -> Reply {
        reply("sendMessage", ok(json!({"message_id":1})))
    }
    // tests.test_emoji_bot.TestAccessControl.test_authorized_user_is_served
    #[test]
    fn test_authorized_user_is_served() {
        let r = handle(
            message(42, "/start", "private"),
            BTreeSet::from([42]),
            BTreeSet::new(),
            vec![sent()],
        );
        assert!(r.iter().any(|r| r.method == "sendMessage"));
        assert!(!r[0].form["text"].contains("not on its access list"));
    }
    // tests.test_emoji_bot.TestAccessControl.test_unauthorized_private_user_gets_only_a_denial
    #[test]
    fn test_unauthorized_private_user_gets_only_a_denial() {
        let r = handle(
            message(999, "/start", "private"),
            BTreeSet::from([42]),
            BTreeSet::new(),
            vec![sent()],
        );
        assert_eq!(r.len(), 1);
        assert!(r[0].form["text"].contains("not on its access list"));
    }
    // tests.test_emoji_bot.TestAccessControl.test_unauthorized_user_cannot_extract_ids
    #[test]
    fn test_unauthorized_user_cannot_extract_ids() {
        let mut m = message(999, "hi", "private");
        m["message"]["entities"] = entities(&["111111111"])["entities"].clone();
        let r = handle(m, BTreeSet::from([42]), BTreeSet::new(), vec![sent()]);
        assert!(
            !r.iter()
                .map(|r| r.form["text"].as_str())
                .collect::<Vec<_>>()
                .join(" ")
                .contains("111111111")
        );
    }
    // tests.test_emoji_bot.TestAccessControl.test_unauthorized_group_message_is_silently_ignored
    #[test]
    fn test_unauthorized_group_message_is_silently_ignored() {
        let mut m = message(999, "hi", "supergroup");
        m["message"]["entities"] = entities(&["111111111"])["entities"].clone();
        assert!(handle(m, BTreeSet::from([42]), BTreeSet::new(), vec![]).is_empty());
    }
    // tests.test_emoji_bot.TestAccessControl.test_a_listed_channel_post_reaches_the_owner
    #[test]
    fn test_a_listed_channel_post_reaches_the_owner() {
        let r = handle(
            post(),
            BTreeSet::from([42]),
            BTreeSet::from([CHANNEL]),
            vec![reply("getCustomEmojiStickers", ok(json!([]))), sent()],
        );
        assert!(
            r.iter()
                .filter(|r| r.method == "sendMessage")
                .any(|r| r.form["chat_id"] == "42")
        );
        assert!(r.last().unwrap().form["text"].contains("Somebody&#x27;s channel"));
    }
    // tests.test_emoji_bot.TestAccessControl.test_an_unlisted_channel_post_is_ignored
    #[test]
    fn test_an_unlisted_channel_post_is_ignored() {
        assert!(
            handle(
                post(),
                BTreeSet::from([42]),
                BTreeSet::from([-100777]),
                vec![]
            )
            .is_empty()
        );
    }
    // tests.test_emoji_bot.TestAccessControl.test_no_channel_list_answers_no_channel
    #[test]
    fn test_no_channel_list_answers_no_channel() {
        assert!(handle(post(), BTreeSet::from([42]), BTreeSet::new(), vec![]).is_empty());
    }
}
#[path = "baseline_telegram_support/added_identity.rs"]
mod added_identity;
#[path = "baseline_telegram_support/transport_contracts.rs"]
mod transport_contracts;
mod created_identity {
    use super::*;
    // tests.test_telegram_client.CreateAdoptionVerifiesContent.test_existence_alone_is_not_adopted
    #[test]
    fn test_existence_alone_is_not_adopted() {
        let m = mutation(
            true,
            None,
            Some(listing(&["foreign"])),
            None,
            matches(&[("foreign", Some(false))]),
            true,
            false,
        );
        assert!(matches!(m.result, Err(ApiError::Rejected(_))));
        assert_eq!(mutation_count(&m), 2);
        assert_eq!(m.compared, vec!["foreign"]);
    }
    // tests.test_telegram_client.CreateAdoptionVerifiesContent.test_our_own_image_is_adopted
    #[test]
    fn test_our_own_image_is_adopted() {
        let m = mutation(
            true,
            None,
            Some(listing(&["ours"])),
            None,
            matches(&[("ours", Some(true))]),
            false,
            false,
        );
        assert_eq!(mutation_count(&m), 1);
        assert_eq!(m.result.unwrap(), Value::Null);
        assert_eq!(m.compared, vec!["ours"]);
    }
    // tests.test_telegram_client.CreateAdoptionVerifiesContent.test_a_populated_set_is_not_the_shape_we_would_have_left
    #[test]
    fn test_a_populated_set_is_not_the_shape_we_would_have_left() {
        let m = mutation(
            true,
            None,
            Some(listing(&["a", "b"])),
            None,
            matches(&[("a", Some(true)), ("b", Some(true))]),
            false,
            false,
        );
        assert!(matches!(m.result, Err(ApiError::Ambiguous(_))));
        assert_eq!(mutation_count(&m), 1);
        assert!(m.compared.is_empty());
    }
    // tests.test_telegram_client.CreateAdoptionVerifiesContent.test_unverifiable_content_stays_unknown
    #[test]
    fn test_unverifiable_content_stays_unknown() {
        let m = mutation(
            true,
            None,
            Some(listing(&["x"])),
            None,
            matches(&[("x", None)]),
            false,
            false,
        );
        assert!(matches!(m.result, Err(ApiError::Ambiguous(_))));
        assert_eq!(mutation_count(&m), 1);
        assert_eq!(m.compared, vec!["x"]);
    }
    // tests.test_telegram_client.CreateAdoptionVerifiesContent.test_missing_set_is_a_definite_no
    #[test]
    fn test_missing_set_is_a_definite_no() {
        let m = mutation(true, None, Some(missing()), None, matches(&[]), true, false);
        assert!(matches!(m.result, Err(ApiError::Rejected(_))));
        assert_eq!(mutation_count(&m), 2);
        assert!(m.compared.is_empty());
    }
}
mod post_add_size {
    use super::*;
    fn size(readback: Reply, values: BTreeMap<String, Option<bool>>) -> Mutation {
        mutation(
            false,
            Some(members(&["old"])),
            Some(listing(&["old", "OURS"])),
            Some(readback),
            values,
            false,
            false,
        )
    }
    // tests.test_telegram_client.PostAddSizeIsMeasured.test_a_foreign_arrival_alone_does_not_book_a_size
    #[test]
    fn test_a_foreign_arrival_alone_does_not_book_a_size() {
        let m = size(
            listing(&["old", "THEIRS"]),
            matches(&[("OURS", Some(true)), ("THEIRS", Some(false))]),
        );
        assert!(matches!(m.result, Err(ApiError::Ambiguous(_))));
        assert_eq!(m.compared, vec!["OURS", "THEIRS"]);
        assert_eq!(mutation_count(&m), 1);
    }
    // tests.test_telegram_client.PostAddSizeIsMeasured.test_an_unverifiable_arrival_does_not_book_a_size
    #[test]
    fn test_an_unverifiable_arrival_does_not_book_a_size() {
        let m = size(
            listing(&["old", "UNREADABLE"]),
            matches(&[("OURS", Some(true)), ("UNREADABLE", None)]),
        );
        assert!(matches!(m.result, Err(ApiError::Ambiguous(_))));
        assert_eq!(m.compared, vec!["OURS", "UNREADABLE"]);
        assert_eq!(mutation_count(&m), 1);
    }
    // tests.test_telegram_client.PostAddSizeIsMeasured.test_our_sticker_present_books_the_true_size
    #[test]
    fn test_our_sticker_present_books_the_true_size() {
        let m = size(
            listing(&["old", "THEIRS", "OURS"]),
            matches(&[("OURS", Some(true)), ("THEIRS", Some(false))]),
        );
        assert_eq!(mutation_count(&m), 1);
        assert_eq!(m.result.unwrap(), json!(3));
        assert_eq!(m.compared, vec!["OURS", "THEIRS", "OURS"]);
    }
    // tests.test_telegram_client.PostAddSizeIsMeasured.test_a_missing_set_is_never_booked_as_zero
    #[test]
    fn test_a_missing_set_is_never_booked_as_zero() {
        let m = size(missing(), matches(&[("OURS", Some(true))]));
        assert!(matches!(m.result, Err(ApiError::Ambiguous(_))));
        assert_eq!(m.compared, vec!["OURS"]);
        assert_eq!(mutation_count(&m), 1);
    }
}
mod repainting {
    use super::*;
    // tests.test_telegram_client.RepaintingIsCreationOnly.test_the_flag_is_sent_when_asked_for
    #[test]
    fn test_the_flag_is_sent_when_asked_for() {
        let m = mutation(true, None, None, None, matches(&[]), false, true);
        assert_eq!(m.requests[0].method, "createNewStickerSet");
        assert_eq!(
            m.requests[0]
                .form
                .get("needs_repainting")
                .map(String::as_str),
            Some("true")
        );
        assert!(m.result.is_ok());
        assert!(m.waits.is_empty());
    }
    // tests.test_telegram_client.RepaintingIsCreationOnly.test_it_is_ABSENT_by_default_not_false
    #[test]
    #[allow(non_snake_case)]
    fn test_it_is_ABSENT_by_default_not_false() {
        let m = mutation(true, None, None, None, matches(&[]), false, false);
        assert!(!m.requests[0].form.contains_key("needs_repainting"));
        assert!(m.result.is_ok());
        assert!(m.waits.is_empty());
    }
}
mod main_wiring {
    use super::*;
    fn run(test: &str) -> Vec<Request> {
        let root = Root::new();
        let mut update = message(424242, "/start", "private");
        update["update_id"] = json!(7);
        let server = Server::new(vec![
            reply("getMe", ok(json!({"username":"bot","id":1}))),
            reply("setMyCommands", ok(json!(true))),
            reply("getUpdates", ok(json!([update]))),
            reply("sendMessage", ok(json!({"message_id":1}))),
            reply(
                "getUpdates",
                reject("Conflict: deliberate end of synthetic polling"),
            ),
        ]);
        run_child(test, &root, &server.base, "bot", &[]);
        let requests = server.finish();
        assert_eq!(requests.len(), 5);
        let polls = requests
            .iter()
            .filter(|r| r.method == "getUpdates")
            .collect::<Vec<_>>();
        assert_eq!(polls[0].form["offset"], "0");
        assert_eq!(polls[1].form["offset"], "8");
        assert_eq!(
            serde_json::from_str::<Value>(&polls[0].form["allowed_updates"]).unwrap(),
            json!(["message", "channel_post", "my_chat_member"])
        );
        assert_eq!(
            serde_json::from_slice::<Value>(
                &std::fs::read(root.0.join("state_emoji_bot.json")).unwrap()
            )
            .unwrap(),
            json!({"offset":8})
        );
        requests
    }
    // tests.test_emoji_bot.TestMainWiring.test_main_passes_numeric_ids_not_update_types
    #[test]
    fn test_main_passes_numeric_ids_not_update_types() {
        if child_driver() {
            return;
        }
        let r = run("main_wiring::test_main_passes_numeric_ids_not_update_types");
        let sends = r
            .iter()
            .filter(|r| r.method == "sendMessage")
            .collect::<Vec<_>>();
        assert_eq!(sends.len(), 1, "handle_update was not reached");
        assert_eq!(sends[0].form["chat_id"].parse::<i64>().unwrap(), 424242);
        assert!(
            !sends[0].form["text"].contains("not on its access list"),
            "main must pass numeric allowlist"
        );
    }
    // tests.test_emoji_bot.TestMainWiring.test_the_owner_is_actually_authorized_end_to_end
    #[test]
    fn test_the_owner_is_actually_authorized_end_to_end() {
        if child_driver() {
            return;
        }
        let r = run("main_wiring::test_the_owner_is_actually_authorized_end_to_end");
        assert!(
            r.iter().any(|r| r.method == "sendMessage"
                && r.form["chat_id"] == "424242"
                && !r.form["text"].contains("not on its access list")),
            "owner must be authorized through real wiring"
        );
    }
}
#[path = "baseline_telegram_support/bookkeeping.rs"]
mod bookkeeping;

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
