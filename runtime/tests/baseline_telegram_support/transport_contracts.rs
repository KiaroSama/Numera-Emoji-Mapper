use super::*;

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
            "transport_contracts::transport::test_network_retry_output_is_redacted",
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
