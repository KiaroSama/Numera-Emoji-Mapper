//! Actual bounded loopback wire checks; no external provider, retry sleep or global mutation.
use _native::coin_http::ProviderHttp;
use serde_json::json;
use std::time::Duration;

#[path = "support/bot_api.rs"]
mod bot_api;

fn exchange<T>(statuses: &[u16], body: &[u8], cmc: bool, call: impl FnOnce(&str) -> T) -> T {
    let fields = if cmc {
        vec![
            ("x-cmc_pro_api_key", "synthetic-wire-key"),
            ("accept", "application/json"),
        ]
    } else {
        vec![("user-agent", "Mozilla/5.0 (logo-fetcher; local)")]
    };
    let replies = statuses
        .iter()
        .map(|status| {
            let mut reply =
                bot_api::Reply::new("GET /provider/fixture", &fields, serde_json::Value::Null);
            reply.status = reqwest::StatusCode::from_u16(*status).unwrap();
            reply.download = Some(body.to_vec());
            if statuses.len() > 1 {
                // Exercise real retries without replacing the production clock or retry policy.
                reply.headers.insert(
                    "retry-after",
                    reqwest::header::HeaderValue::from_static("0.0000000001"),
                );
            }
            reply
        })
        .collect();
    let mut server = bot_api::Server::new(replies);
    let result = call(&format!("{}/provider/fixture", server.base));
    let requests = server.finish().unwrap();
    assert_eq!(
        requests.len(),
        statuses.len(),
        "unexpected provider retry count"
    );
    for request in requests {
        assert_eq!(request.method, "GET /provider/fixture");
        for (name, value) in &fields {
            assert_eq!(request.form.get(*name).map(String::as_str), Some(*value));
        }
    }
    result
}

#[test]
fn real_provider_status_headers_json_and_logo_bytes_keep_distinct_outcomes() {
    use _native::{coin_cmc as cmc, coin_paprika as paprika};
    let client = ProviderHttp::new(Duration::from_secs(3)).unwrap();
    let headers = cmc::headers("synthetic-wire-key").unwrap();
    for status in [400, 404] {
        assert!(matches!(
            exchange(&[status], b"not JSON", true, |url| cmc::http_json(
                &client, url, &headers
            ))
            .unwrap(),
            cmc::Response::Missing
        ));
    }
    for status in [401, 403] {
        assert_eq!(
            exchange(&[status], b"not JSON", true, |url| cmc::http_json(
                &client, url, &headers
            ))
            .err()
            .unwrap(),
            format!("CMC_API_KEY rejected by CoinMarketCap (HTTP {status})")
        );
    }
    assert!(matches!(
        exchange(&[402], b"not JSON", false, |url| paprika::http_json(
            &client, url
        ))
        .unwrap(),
        paprika::Response::Quota
    ));
    assert!(
        exchange(&[200], b"not JSON", false, |url| paprika::http_json(
            &client, url
        ))
        .is_err()
    );
    let cmc::Response::Payload(value) = exchange(&[200], b"{\"data\":{}}", true, |url| {
        cmc::http_json(&client, url, &headers)
    })
    .unwrap() else {
        panic!("answered JSON must remain payload");
    };
    assert_eq!(value, json!({"data":{}}));
    let paprika::Response::Payload(value) = exchange(
        &[200],
        b"{\"currencies\":[{\"id\":\"btc\"}]}",
        false,
        |url| paprika::http_json(&client, url),
    )
    .unwrap() else {
        panic!("a successful Paprika lookup must preserve its payload");
    };
    assert_eq!(value, json!({"currencies":[{"id":"btc"}]}));
    assert_eq!(
        exchange(
            &[200],
            b"\x89PNG\x00\xff",
            false,
            |url| paprika::http_bytes(&client, url)
        )
        .unwrap()
        .unwrap(),
        b"\x89PNG\x00\xff"
    );
    assert_eq!(
        Duration::try_from_secs_f64(0.0000000001).unwrap(),
        Duration::ZERO
    );
    assert!(matches!(
        exchange(&[500; 4], b"not JSON", true, |url| cmc::http_json(
            &client, url, &headers
        ))
        .unwrap(),
        cmc::Response::Transient
    ));
    assert!(matches!(
        exchange(&[500; 4], b"not JSON", false, |url| paprika::http_json(
            &client, url
        ))
        .unwrap(),
        paprika::Response::Transient
    ));
    assert!(paprika::http_json(&client, "http://user:password@127.0.0.1/fixture").is_err());
    assert!(cmc::logo(&client, &headers, "7&api_key=secret").is_err());
}
