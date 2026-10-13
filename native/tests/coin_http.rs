//! Provider retries are observed with virtual time; one bounded loopback request covers wire transport.
use _native::coin_http::{ProviderHttp, Reply, Retry, get_with};
use serde_json::{Value, json};
use std::time::Duration;

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
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        use std::io::{Read, Write};
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && std::time::Instant::now() < deadline =>
                {
                    std::thread::yield_now()
                }
                Err(error) => panic!("loopback accept: {error}"),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut bytes = [0; 4096];
        let count = stream.read(&mut bytes).unwrap();
        assert!(String::from_utf8_lossy(&bytes[..count]).starts_with("GET /fixture "));
        stream.write_all(b"HTTP/1.1 402 Payment Required\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").unwrap();
    });
    let client = ProviderHttp::new(Duration::from_secs(3)).unwrap();
    let response = client
        .get(
            &format!("http://127.0.0.1:{port}/fixture"),
            &reqwest::header::HeaderMap::new(),
            &Retry {
                stop_on: vec![402],
                ..Retry::default()
            },
        )
        .unwrap()
        .unwrap();
    assert_eq!(response.status, 402);
    assert_eq!(response.bytes, b"{}");
    server.join().unwrap();
}
