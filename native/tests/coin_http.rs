//! Provider retries use virtual time; actual wire behavior belongs to coin_provider_wire.
use _native::coin_http::{Reply, Retry, get_with};
use serde_json::{Value, json};

#[test]
fn source_retry_ladder_status_stops_and_retry_after_are_preserved() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/native/coin-http.json")).unwrap();
    assert_eq!(
        fixture["source_sha256"],
        "c3ca151103971504e4eefdf5c44b7bb50858e0936fa6cd806982c8bede3540db"
    );
    for case in fixture["cases"].as_array().unwrap() {
        let statuses = case["statuses"].as_array().unwrap();
        let mut calls = Vec::new();
        let mut waits = Vec::new();
        let policy = Retry {
            stop_on: serde_json::from_value(case["stop_on"].clone()).unwrap(),
            ..Retry::default()
        };
        let reply = get_with(
            &policy,
            || {
                let value = statuses[calls.len()].clone();
                calls.push(value.clone());
                let status = value
                    .as_u64()
                    .ok_or_else(|| "transport unavailable".to_owned())?
                    as u16;
                Ok(Reply {
                    status,
                    retry_after: Some(case["retry_after"].as_str().unwrap().into()),
                    bytes: Vec::new(),
                })
            },
            |duration| {
                waits.push(duration.as_secs_f64());
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(
            json!(reply.map(|reply| reply.status)),
            case["expected"]["status"]
        );
        assert_eq!(json!(calls), case["expected"]["calls"]);
        assert_eq!(json!(waits), case["expected"]["waits"]);
    }
    original_retry_assertions();
    let oversized = Retry {
        backoff: f64::MAX,
        max_backoff: f64::MAX,
        ..Retry::default()
    };
    let mut requests = 0;
    let error = get_with(
        &oversized,
        || {
            requests += 1;
            Err("transport unavailable".into())
        },
        |_| panic!("an unrepresentable retry delay must not reach the waiter"),
    )
    .err()
    .expect("finite but unrepresentable retry delays must return an error");
    assert_eq!(error, "provider retry wait exceeds native duration range");
    assert_eq!(requests, 1);
    assert_eq!(
        oversized.wait(1, Some("7")).unwrap(),
        std::time::Duration::from_secs(7)
    );
    for (raw, expected) in [("", 2.0), ("15", 15.0), ("12 seconds", 2.0), (" -3 ", 0.0)] {
        let delay = _native::coin_http::page_delay(raw).unwrap();
        assert_eq!(delay.as_secs_f64(), expected);
        if raw.is_empty() {
            assert!(delay.as_secs_f64() <= 3.0);
        }
    }
}

fn original_retry_assertions() {
    for (statuses, header, attempts, expected_status, expected_waits) in [
        (vec![429, 200], "7", 3, Some(200), vec![7.0]),
        (vec![429, 200], "3600", 3, Some(200), vec![60.0]),
        (vec![500, 500, 500, 500], "", 4, None, vec![4.0, 8.0, 12.0]),
        (vec![0, 0], "", 2, None, vec![4.0]),
    ] {
        let mut calls = Vec::new();
        let mut waits = Vec::new();
        let result = get_with(
            &Retry {
                attempts,
                ..Retry::default()
            },
            || {
                let status = statuses[calls.len()];
                calls.push(status);
                if status == 0 {
                    Err("connection reset".into())
                } else {
                    Ok(Reply {
                        status,
                        retry_after: Some(header.into()),
                        bytes: b"[]".to_vec(),
                    })
                }
            },
            |duration| {
                waits.push(duration.as_secs_f64());
                Ok(())
            },
        )
        .unwrap();
        assert_eq!(result.as_ref().map(|reply| reply.status), expected_status);
        if let Some(reply) = result {
            assert_eq!(reply.bytes, b"[]");
        }
        assert_eq!(calls, statuses);
        assert_eq!(waits, expected_waits);
    }
}
