use super::*;
// tests.test_coin_cli_args.AnUnrecognisedArgumentCannotSelectTheLiveBranch.test_a_misspelled_dry_flag_is_refused_by_both_providers
#[test]
fn test_a_misspelled_dry_flag_is_refused_by_both_providers() {
    for provider in ["paprika", "cmc"] {
        for typo in ["--dryy", "--dry-run", "--dr", "-dry", "dry"] {
            let root = empty_provider();
            let before = doc(&root.0.join("ticker_to_id.json"));
            let server = Server::new(vec![]);
            let result = command(
                &root,
                &server.base,
                &server.base,
                &ids(&[&format!("coin-fetch-{provider}"), typo]),
                &[],
            );
            assert_eq!(
                result.code, 2,
                "{provider} {typo}: {}{}",
                result.stdout, result.stderr
            );
            assert!(result.stderr.to_lowercase().contains("usage:"));
            assert_eq!(doc(&root.0.join("ticker_to_id.json")), before);
            assert!(server.finish().is_empty());
        }
    }
}
// tests.test_coin_cli_args.AnUnrecognisedArgumentCannotSelectTheLiveBranch.test_the_real_dry_flag_still_parses
#[test]
fn test_the_real_dry_flag_still_parses() {
    for provider in ["paprika", "cmc"] {
        let root = empty_provider();
        let server = Server::new(vec![]);
        let result = command(
            &root,
            &server.base,
            &server.base,
            &ids(&[
                &format!("coin-fetch-{provider}"),
                "--coin-dir",
                root.0.to_str().unwrap(),
                "--dry",
            ]),
            &[("CMC_API_KEY", "k")],
        );
        assert_eq!(
            result.code, 0,
            "{provider}: {}{}",
            result.stdout, result.stderr
        );
        assert!(
            result
                .stdout
                .contains("dry run: no downloads/pack changes.")
        );
        assert!(server.finish().is_empty());
        assert_eq!(
            doc(&root.0.join("ticker_to_id.json")),
            serde_json::json!({})
        );
        assert!(!root.0.join("logos").exists());
        assert!(!root.0.join("rebuild_dedup_state.json").exists());
    }
}
// tests.test_coin_cli_args.AnUnrecognisedArgumentCannotSelectTheLiveBranch.test_an_unset_token_stops_a_live_run_before_any_download
#[test]
fn test_an_unset_token_stops_a_live_run_before_any_download() {
    for provider in ["paprika", "cmc"] {
        let root = empty_provider();
        let server = Server::new(vec![]);
        let result = command(
            &root,
            &server.base,
            &server.base,
            &ids(&[
                &format!("coin-fetch-{provider}"),
                "--coin-dir",
                root.0.to_str().unwrap(),
            ]),
            &[("CMC_API_KEY", "k")],
        );
        assert_eq!(
            result.code, 2,
            "{provider}: {}{}",
            result.stdout, result.stderr
        );
        assert!(result.stderr.contains("TELEGRAM_BOT_TOKEN is not set"));
        assert!(server.finish().is_empty());
        assert!(!root.0.join("logos").exists());
        assert_eq!(
            doc(&root.0.join("ticker_to_id.json")),
            serde_json::json!({})
        );
    }
}
// tests.test_coin_cli_args.AnUnrecognisedArgumentCannotSelectTheLiveBranch.test_fetch_logos_page_count_is_not_read_at_import
#[test]
fn test_fetch_logos_page_count_is_not_read_at_import() {
    let root = empty_provider();
    let server = Server::new(vec![]);
    let result = command(
        &root,
        &server.base,
        &server.base,
        &ids(&["coin-fetch-logos", "--help"]),
        &[],
    );
    assert_eq!(result.code, 0);
    assert!(result.stdout.to_lowercase().contains("usage:"));
    assert!(result.stdout.contains("Default: 40 market pages"));
    for bad in ["notanumber", "0", "-3"] {
        let result = command(
            &root,
            &server.base,
            &server.base,
            &ids(&["coin-fetch-logos", bad]),
            &[],
        );
        assert_eq!(result.code, 2, "{bad}: {}{}", result.stdout, result.stderr);
    }
    assert!(server.finish().is_empty());
    assert!(!root.0.join("logos").exists());
}
