use super::*;
fn fixture() -> Verify {
    let f = Verify::new();
    write(&f.files.map, &json!({"sol":OLD}));
    f
}
const MARKETS: &str = "/api/v3/coins/markets?vs_currency=usd&order=market_cap_desc&per_page=250&page=1&sparkline=false";
fn invoke(
    f: &Verify,
    server: &Server,
    values: &[(&str, &str)],
    only: &str,
) -> rt::tooling_process::Completed {
    command(&f.root, &server.base, &server.base, &f.args(only), values)
}
// tests.test_verify_logos.VerifyLogosMainContracts.test_a_requested_fix_that_failed_exits_nonzero
#[test]
fn test_a_requested_fix_that_failed_exits_nonzero() {
    let f = fixture();
    let server = Server::new(vec![json_reply(MARKETS, 200, json!([]))]);
    let result = invoke(
        &f,
        &server,
        &[("TELEGRAM_BOT_TOKEN", TOKEN), ("PACK_OWNER_USER_ID", "7")],
        "sol",
    );
    assert_eq!(result.code, 4, "{}{}", result.stdout, result.stderr);
    assert_eq!(server.finish().len(), 1);
    f.unchanged();
}
// tests.test_verify_logos.VerifyLogosMainContracts.test_missing_owner_id_is_a_usage_error_not_a_traceback
#[test]
fn test_missing_owner_id_is_a_usage_error_not_a_traceback() {
    let f = fixture();
    let server = Server::new(vec![]);
    let result = invoke(&f, &server, &[("TELEGRAM_BOT_TOKEN", TOKEN)], "sol");
    assert_eq!(result.code, 2);
    assert!(!result.stderr.contains("panicked"));
    assert!(server.finish().is_empty());
}
// tests.test_verify_logos.VerifyLogosMainContracts.test_non_numeric_owner_id_is_a_usage_error
#[test]
fn test_non_numeric_owner_id_is_a_usage_error() {
    let f = fixture();
    let server = Server::new(vec![]);
    let result = invoke(
        &f,
        &server,
        &[
            ("TELEGRAM_BOT_TOKEN", TOKEN),
            ("PACK_OWNER_USER_ID", "not-a-number"),
        ],
        "sol",
    );
    assert_eq!(result.code, 2);
    assert!(server.finish().is_empty());
}
// tests.test_verify_logos.VerifyLogosMainContracts.test_missing_token_is_a_usage_error
#[test]
fn test_missing_token_is_a_usage_error() {
    let f = fixture();
    let server = Server::new(vec![]);
    assert_eq!(
        invoke(&f, &server, &[("PACK_OWNER_USER_ID", "7")], "sol").code,
        2
    );
    assert!(server.finish().is_empty());
}
// tests.test_verify_logos.VerifyLogosMainContracts.test_unreadable_state_file_is_a_usage_error
#[test]
fn test_unreadable_state_file_is_a_usage_error() {
    let f = fixture();
    std::fs::write(&f.files.state, b"{not json").unwrap();
    let server = Server::new(vec![]);
    assert_eq!(
        invoke(
            &f,
            &server,
            &[("TELEGRAM_BOT_TOKEN", TOKEN), ("PACK_OWNER_USER_ID", "7")],
            "sol"
        )
        .code,
        2
    );
    assert!(server.finish().is_empty());
}
// tests.test_verify_logos.VerifyLogosMainContracts.test_an_unresolved_intent_blocks_every_replacement
#[test]
fn test_an_unresolved_intent_blocks_every_replacement() {
    let f = Verify::new();
    f.pending();
    let server = Server::new(vec![unavailable()]);
    let result = invoke(
        &f,
        &server,
        &[("TELEGRAM_BOT_TOKEN", TOKEN), ("PACK_OWNER_USER_ID", "7")],
        "sol",
    );
    assert_eq!(result.code, 4, "{}{}", result.stdout, result.stderr);
    let requests = server.finish();
    assert_eq!(requests.len(), 1);
    assert!(
        requests
            .iter()
            .all(|r| r.method != "replaceStickerInSet" && !r.method.starts_with("GET /api"))
    );
    assert!(f.files.intent.exists());
    f.unchanged();
}
// tests.test_verify_logos.VerifyLogosMainContracts.test_a_clear_intent_lets_the_run_proceed
#[test]
fn test_a_clear_intent_lets_the_run_proceed() {
    let f = fixture();
    let mut script = vec![
        json_reply(
            MARKETS,
            200,
            json!([{"symbol":"sol","name":"Solana","image":"https://fixture.invalid/btc.png"}]),
        ),
        provider_reply("/btc.png", 200, &noise("official-btc")),
        listing(&f.before()),
        reply("replaceStickerInSet", ok(json!(true))),
        listing(&f.after()),
    ];
    uploaded_download(&mut script, &format!("fid-{NEW}"));
    let server = Server::new(script);
    let result = invoke(
        &f,
        &server,
        &[("TELEGRAM_BOT_TOKEN", TOKEN), ("PACK_OWNER_USER_ID", "7")],
        "sol",
    );
    assert_eq!(result.code, 0, "{}{}", result.stdout, result.stderr);
    let requests = server.finish();
    assert_eq!(
        requests
            .iter()
            .filter(|r| r.method == "replaceStickerInSet")
            .count(),
        1
    );
    assert_eq!(doc(&f.files.map)["sol"], NEW);
    assert!(!f.files.intent.exists());
}
