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
mod transport {
    use super::*;
    // tests.test_telegram_client.TokenRedaction.test_exception_text_never_carries_the_token
    #[test]
    fn test_exception_text_never_carries_the_token() {
        let server = Server::new(vec![]);
        let out = server.telegram().safe(&format!(
            "HTTPSConnectionPool: /bot{TOKEN}/addStickerToSet failed"
        ));
        assert!(!out.contains(TOKEN));
        assert!(out.contains("[REDACTED]"));
        assert!(server.finish().is_empty());
    }
    // tests.test_telegram_client.TokenRedaction.test_network_retry_output_is_redacted
    #[test]
    fn test_network_retry_output_is_redacted() {
        if child_driver() {
            return;
        }
        let root = Root::new();
        let server = Server::new(vec![fault("getMe"), fault("getMe")]);
        // Capture actual native console output as well as the durable log; run_child asserts both streams.
        run_child(
            "transport::test_network_retry_output_is_redacted",
            &root,
            &server.base,
            "redaction",
            &[],
        );
        assert_eq!(server.finish().len(), 2);
        let logs = std::fs::read_dir(root.0.join("logs"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "log"))
            .map(|p| std::fs::read_to_string(p).unwrap())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(!logs.contains(TOKEN));
        assert!(logs.contains("[REDACTED]"));
    }
    // tests.test_telegram_client.StickerSetInvalidScope.test_lookup_fails_fast_without_sleeping
    #[test]
    fn test_lookup_fails_fast_without_sleeping() {
        let server = Server::new(vec![missing()]);
        let mut waits = vec![];
        assert!(call(&server.telegram(), "getStickerSet", 5, &mut waits).is_err());
        assert!(
            waits.is_empty(),
            "plain lookup must not sleep on missing set"
        );
        assert_eq!(server.finish().len(), 1);
    }
    // tests.test_telegram_client.StickerSetInvalidScope.test_create_still_waits_for_the_name_lock
    #[test]
    fn test_create_still_waits_for_the_name_lock() {
        let server = Server::new(
            (0..3)
                .map(|_| reply("createNewStickerSet", reject("STICKERSET_INVALID")))
                .collect(),
        );
        let mut waits = vec![];
        assert!(call(&server.telegram(), "createNewStickerSet", 3, &mut waits).is_err());
        assert!(!waits.is_empty());
        assert!(waits.iter().sum::<Duration>() <= Duration::from_secs(300));
        assert_eq!(waits.len(), 2, "no sleep after final attempt");
        assert_eq!(server.finish().len(), 3);
    }
    // tests.test_telegram_client.TriStateLiveReads.test_exists
    #[test]
    fn test_exists() {
        let server = Server::new(vec![listed(json!([{}, {}]))]);
        let (state, set) = server.telegram().probe("s");
        assert_eq!(state, SetState::Exists);
        assert_eq!(set.unwrap()["stickers"].as_array().unwrap().len(), 2);
        server.finish();
    }
    // tests.test_telegram_client.TriStateLiveReads.test_missing_is_a_real_zero
    #[test]
    fn test_missing_is_a_real_zero() {
        let server = Server::new(vec![missing()]);
        let (state, set) = server.telegram().probe("s");
        assert_eq!(state, SetState::Missing);
        assert!(set.is_none());
        assert_ne!(state, SetState::Unknown);
        server.finish();
    }
    // tests.test_telegram_client.TriStateLiveReads.test_unknown_raises_instead_of_returning_zero
    #[test]
    fn test_unknown_raises_instead_of_returning_zero() {
        let server = Server::new(vec![fault("getStickerSet")]);
        let (state, set) = server.telegram().probe("s");
        assert_eq!(state, SetState::Unknown);
        assert_ne!(
            state,
            SetState::Missing,
            "unknown cannot become missing/zero"
        );
        assert!(set.is_none());
        server.finish();
    }
    // tests.test_telegram_body_shape.ANonObjectBodyGoesThroughTheRetryPath.test_a_list_body_is_retried_then_reported_not_an_attribute_error
    #[test]
    fn test_a_list_body_is_retried_then_reported_not_an_attribute_error() {
        for body in [json!([]), json!("ok"), json!(42)] {
            let server = Server::new(vec![reply("getMe", body.clone()), reply("getMe", body)]);
            let mut waits = vec![];
            assert!(matches!(
                call(&server.telegram(), "getMe", 2, &mut waits),
                Err(ApiError::Transport(_))
            ));
            assert_eq!(server.finish().len(), 2);
        }
    }
    // tests.test_telegram_body_shape.ANonObjectBodyGoesThroughTheRetryPath.test_an_add_with_a_body_that_is_not_an_object_asks_the_applied_check
    #[test]
    fn test_an_add_with_a_body_that_is_not_an_object_asks_the_applied_check() {
        let server = Server::new(vec![reply("addStickerToSet", json!([]))]);
        let mut fired = 0;
        let mut waits = vec![];
        let result = server
            .telegram()
            .call_with_wait(
                "addStickerToSet",
                &BTreeMap::new(),
                &[],
                2,
                Some(|| {
                    fired += 1;
                    Ok(Some(true))
                }),
                |d| waits.push(d),
            )
            .unwrap();
        assert_eq!(result, json!({"verified_applied":true}));
        assert_eq!(fired, 1);
        assert_eq!(waits, vec![Duration::from_secs(2)]);
        assert_eq!(server.finish().len(), 1);
    }
    // tests.test_telegram_client.WaitsReachTheLogFile.test_a_flood_wait_is_logged_not_only_printed
    #[test]
    fn test_a_flood_wait_is_logged_not_only_printed() {
        let root = Root::new();
        let log = root.log();
        let server = Server::new(vec![
            reply(
                "addStickerToSet",
                json!({"ok":false,"description":"Too Many Requests: retry after 7","parameters":{"retry_after":7}}),
            ),
            reply("addStickerToSet", ok(json!({"done":true}))),
        ]);
        let mut waits = vec![];
        assert_eq!(
            call(
                &server.telegram().with_log(&log),
                "addStickerToSet",
                5,
                &mut waits
            )
            .unwrap(),
            json!({"done":true})
        );
        let line = std::fs::read_to_string(log.path.as_ref().unwrap()).unwrap();
        assert!(
            line.contains("flood wait") && line.contains('7') && line.contains("addStickerToSet")
        );
        assert_eq!(waits, vec![Duration::from_secs(8)]);
        assert_eq!(server.finish().len(), 2);
    }
    // tests.test_telegram_client.WaitsReachTheLogFile.test_a_network_retry_is_logged_with_the_token_redacted
    #[test]
    fn test_a_network_retry_is_logged_with_the_token_redacted() {
        let root = Root::new();
        let log = root.log();
        let server = Server::new(vec![fault("getMe"), fault("getMe")]);
        let mut waits = vec![];
        assert!(call(&server.telegram().with_log(&log), "getMe", 2, &mut waits).is_err());
        let line = std::fs::read_to_string(log.path.as_ref().unwrap()).unwrap();
        assert!(line.contains("net retry"));
        assert!(!line.contains(TOKEN));
        assert!(line.contains("[REDACTED]"));
        assert_eq!(waits, vec![Duration::from_secs(3)]);
        assert_eq!(server.finish().len(), 2);
    }
}
mod added_identity {
    use super::*;
    fn check(after: Value, comparison: BTreeMap<String, Option<bool>>, retry: bool) -> Mutation {
        mutation(
            false,
            Some(members(&["a", "b"])),
            Some(listed(after)),
            None,
            comparison,
            retry,
            false,
        )
    }
    // tests.test_telegram_client.AddedCheckUsesIdentity.test_our_own_new_sticker_is_recognised
    #[test]
    fn test_our_own_new_sticker_is_recognised() {
        let m = mutation(
            false,
            Some(members(&["a", "b"])),
            Some(listing(&["a", "b", "MINE"])),
            Some(listing(&["a", "b", "MINE"])),
            matches(&[("MINE", Some(true))]),
            false,
            false,
        );
        assert_eq!(m.result.unwrap(), json!(3));
        assert_eq!(mutation_count_ref(&m.requests), 1);
        assert_eq!(m.compared, vec!["MINE", "MINE"]);
    }
    fn mutation_count_ref(r: &[Request]) -> usize {
        r.iter().filter(|r| r.method == "addStickerToSet").count()
    }
    // tests.test_telegram_client.AddedCheckUsesIdentity.test_nothing_new_means_not_applied
    #[test]
    fn test_nothing_new_means_not_applied() {
        let m = check(members(&["a", "b"]), matches(&[]), true);
        assert!(matches!(m.result, Err(ApiError::Rejected(_))));
        assert_eq!(mutation_count(&m), 2, "a definite no permits the retry");
        assert!(m.compared.is_empty());
    }
    // tests.test_telegram_client.AddedCheckUsesIdentity.test_one_new_but_FOREIGN_sticker_is_not_our_upload
    #[test]
    #[allow(non_snake_case)]
    fn test_one_new_but_FOREIGN_sticker_is_not_our_upload() {
        let m = check(
            members(&["a", "b", "THEIRS"]),
            matches(&[("THEIRS", Some(false))]),
            true,
        );
        assert!(matches!(m.result, Err(ApiError::Rejected(_))));
        assert_eq!(
            mutation_count(&m),
            2,
            "a stranger is a definite no, not upload success"
        );
        assert_eq!(m.compared, vec!["THEIRS"]);
    }
    // tests.test_telegram_client.AddedCheckUsesIdentity.test_one_new_sticker_we_cannot_verify_is_unknown
    #[test]
    fn test_one_new_sticker_we_cannot_verify_is_unknown() {
        let m = check(members(&["a", "b", "?"]), matches(&[("?", None)]), false);
        assert!(matches!(m.result, Err(ApiError::Ambiguous(_))));
        assert_eq!(mutation_count(&m), 1);
        assert_eq!(m.compared, vec!["?"]);
    }
    // tests.test_telegram_client.AddedCheckUsesIdentity.test_two_new_identities_are_unattributable
    #[test]
    fn test_two_new_identities_are_unattributable() {
        let m = check(
            members(&["a", "b", "THEIRS", "MINE"]),
            matches(&[("THEIRS", Some(true)), ("MINE", Some(true))]),
            false,
        );
        assert!(matches!(m.result, Err(ApiError::Ambiguous(_))));
        assert_eq!(mutation_count(&m), 1);
        assert!(m.compared.is_empty());
    }
    // tests.test_telegram_client.AddedCheckUsesIdentity.test_a_replacement_is_not_read_as_our_add
    #[test]
    fn test_a_replacement_is_not_read_as_our_add() {
        let m = check(
            members(&["a", "THEIRS", "MINE"]),
            matches(&[("THEIRS", Some(true)), ("MINE", Some(true))]),
            false,
        );
        assert!(matches!(m.result, Err(ApiError::Ambiguous(_))));
        assert_eq!(mutation_count(&m), 1);
        assert!(m.compared.is_empty());
    }
    // tests.test_telegram_client.AddedCheckUsesIdentity.test_without_a_snapshot_nothing_is_claimed
    #[test]
    fn test_without_a_snapshot_nothing_is_claimed() {
        let m = mutation(
            false,
            None,
            Some(listing(&["a", "b", "x"])),
            None,
            matches(&[("x", Some(true))]),
            false,
            false,
        );
        assert!(matches!(m.result, Err(ApiError::Ambiguous(_))));
        assert_eq!(mutation_count(&m), 1);
        assert!(m.compared.is_empty());
    }
    // tests.test_telegram_client.AddedCheckUsesIdentity.test_stickers_without_identity_are_unknown_not_applied
    #[test]
    fn test_stickers_without_identity_are_unknown_not_applied() {
        let m = mutation(
            false,
            Some(members(&["None"])),
            Some(listed(json!([{"i":0},{"i":1}]))),
            None,
            matches(&[]),
            false,
            false,
        );
        assert!(matches!(m.result, Err(ApiError::Ambiguous(_))));
        assert_eq!(mutation_count(&m), 1);
        assert!(m.compared.is_empty());
    }
    // tests.test_telegram_client.AddedCheckUsesIdentity.test_duplicate_identities_are_not_trusted
    #[test]
    fn test_duplicate_identities_are_not_trusted() {
        let after = members(&["same", "same"]);
        assert!(rt::telegram::usable_fuids(after.as_array().unwrap()).is_none());
        let m = mutation(
            false,
            Some(members(&["x"])),
            Some(listed(after)),
            None,
            matches(&[]),
            false,
            false,
        );
        assert!(matches!(m.result, Err(ApiError::Ambiguous(_))));
        assert_eq!(mutation_count(&m), 1);
    }
    // tests.test_telegram_client.AddedCheckUsesIdentity.test_usable_fuids_accepts_a_well_formed_set
    #[test]
    fn test_usable_fuids_accepts_a_well_formed_set() {
        assert_eq!(
            rt::telegram::usable_fuids(members(&["a", "b"]).as_array().unwrap()),
            Some(BTreeSet::from(["a".into(), "b".into()]))
        );
    }
    // tests.test_telegram_client.AddedCheckUsesIdentity.test_usable_fuids_rejects_a_missing_id
    #[test]
    fn test_usable_fuids_rejects_a_missing_id() {
        assert!(rt::telegram::usable_fuids(members(&["a", ""]).as_array().unwrap()).is_none());
    }
}
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
mod bookkeeping {
    use super::*;
    // tests.test_telegram_client.BookkeepingFollowsLiveState.test_a_foreign_sticker_during_a_retry_does_not_drift_the_count
    #[test]
    fn test_a_foreign_sticker_during_a_retry_does_not_drift_the_count() {
        if child_driver() {
            return;
        }
        let root = Root::new();
        let src = root.0.join("src");
        std::fs::create_dir(&src).unwrap();
        let b = src.join("b.png");
        let c = src.join("c.png");
        image::RgbaImage::from_pixel(100, 100, image::Rgba([200, 0, 0, 255]))
            .save(&b)
            .unwrap();
        image::RgbaImage::from_pixel(100, 100, image::Rgba([0, 200, 0, 255]))
            .save(&c)
            .unwrap();
        let foreign = root.0.join("foreign.png");
        image::RgbaImage::from_pixel(100, 100, image::Rgba([0, 0, 200, 255]))
            .save(&foreign)
            .unwrap();
        let bytes = std::fs::read(&b).unwrap();
        let foreign_bytes = std::fs::read(&foreign).unwrap();
        let state = root.0.join("state.json");
        std::fs::write(&state,json!({"base":"t","per_set":200,"done":["a"],"sent":[],"sets":[{"name":"t1_by_bot","title":"T 1","count":1,"index":1}],"in_flight":null}).to_string()).unwrap();
        // The production codec companion is copied into this owned installed-root fixture.
        let executable = root.0.join(if cfg!(windows) {
            "numera-emoji.exe"
        } else {
            "numera-emoji"
        });
        std::fs::copy(env!("CARGO_BIN_EXE_numera-emoji"), executable).unwrap();
        std::fs::create_dir(root.0.join("assets")).unwrap();
        std::fs::write(root.0.join("assets/panel.html"), b"owned codec-root marker").unwrap();
        let mut script = vec![
            reply("getMe", ok(json!({"username":"bot"}))),
            listing(&["live0"]),
            listing(&["live0"]),
        ];
        let mut first = fault("addStickerToSet");
        first.observe = Some(state.clone());
        script.push(first);
        script.push(listing(&["live0", "foreign"]));
        download(&mut script, "foreign", &foreign_bytes);
        let mut second = reply("addStickerToSet", ok(json!(true)));
        second.observe = Some(state.clone());
        script.push(second);
        script.push(listing(&["live0", "foreign", "ours2"]));
        download(&mut script, "foreign", &foreign_bytes);
        download(&mut script, "ours2", &bytes);
        script.push(listing(&["live0", "foreign", "ours2"]));
        let mut third = reply("addStickerToSet", ok(json!(true)));
        third.observe = Some(state.clone());
        script.push(third);
        script.push(reply("sendMessage", ok(json!({}))));
        let server = Server::new(script);
        run_child(
            "bookkeeping::test_a_foreign_sticker_during_a_retry_does_not_drift_the_count",
            &root,
            &server.base,
            "ticker",
            &ids(&[
                "--base",
                "t",
                "--title",
                "T",
                "--user-id",
                "1",
                "--source-dir",
                src.to_str().unwrap(),
                "--token-env",
                "FAKE_TOKEN",
                "--state",
                state.to_str().unwrap(),
            ]),
        );
        let requests = server.finish();
        let intents = requests
            .iter()
            .filter(|r| r.method == "addStickerToSet")
            .map(|r| r.observation.as_ref().unwrap()["in_flight"].clone())
            .collect::<Vec<_>>();
        let saved: Value = serde_json::from_slice(&std::fs::read(state).unwrap()).unwrap();
        let actual_live = 1 + requests
            .iter()
            .filter(|r| r.method == "addStickerToSet")
            .count();
        assert_eq!(
            saved["sets"][0]["count"],
            json!(actual_live),
            "recorded count must describe actual live0+foreign+oursb+oursc"
        );
        assert_eq!(actual_live, 4);
        let adds = requests
            .iter()
            .filter(|r| r.method == "addStickerToSet")
            .collect::<Vec<_>>();
        assert_eq!(adds[0].files["file0"], bytes);
        assert_eq!(adds[1].files["file0"], bytes);
        assert_eq!(adds[2].files["file0"], std::fs::read(&c).unwrap());
        assert_eq!(
            intents
                .iter()
                .map(|i| i["expected_before"].clone())
                .collect::<Vec<_>>(),
            vec![json!(1), json!(1), json!(3)]
        );
        assert_eq!(
            intents.iter().map(|i| i["key"].clone()).collect::<Vec<_>>(),
            vec![json!("b"), json!("b"), json!("c")]
        );
        let mut done: Vec<String> = serde_json::from_value(saved["done"].clone()).unwrap();
        done.sort();
        assert_eq!(done, ids(&["a", "b", "c"]));
    }

    // tests.test_telegram_client.BookkeepingFollowsLiveState.test_an_unknown_size_after_a_retried_add_is_not_guessed
    #[test]
    fn test_an_unknown_size_after_a_retried_add_is_not_guessed() {
        let m = mutation_with_retry(
            false,
            Some(members(&["live0"])),
            Some(listing(&["live0", "foreign"])),
            Some(fault("getStickerSet")),
            matches(&[("foreign", Some(false))]),
            Some(ok(json!(true))),
            false,
        );
        assert!(matches!(m.result, Err(ApiError::Ambiguous(_))));
        assert_eq!(mutation_count(&m), 2);
    }
    // tests.test_telegram_client.BookkeepingFollowsLiveState.test_a_foreign_sticker_alone_does_not_certify_the_size
    #[test]
    fn test_a_foreign_sticker_alone_does_not_certify_the_size() {
        let m = mutation_with_retry(
            false,
            Some(members(&["live0"])),
            Some(listing(&["live0", "foreign"])),
            Some(listing(&["live0", "foreign"])),
            matches(&[("foreign", Some(false))]),
            Some(ok(json!(true))),
            false,
        );
        assert!(matches!(m.result, Err(ApiError::Ambiguous(_))));
        assert_eq!(mutation_count(&m), 2);
    }
    // tests.test_telegram_client.BookkeepingFollowsLiveState.test_an_undisturbed_add_costs_no_extra_probe
    #[test]
    fn test_an_undisturbed_add_costs_no_extra_probe() {
        let m = mutation(
            false,
            Some(members(&["live0"])),
            None,
            None,
            matches(&[]),
            false,
            false,
        );
        assert_eq!(
            m.result.unwrap(),
            Value::Null,
            "clean add stays at assumed+1"
        );
        assert_eq!(
            m.requests
                .iter()
                .filter(|r| r.method == "getStickerSet")
                .count(),
            1,
            "only preadd identity snapshot"
        );
    }
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
