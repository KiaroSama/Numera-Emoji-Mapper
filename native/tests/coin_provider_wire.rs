//! Actual bounded loopback wire checks; no external provider, retry sleep or global mutation.
use _native::coin_http::ProviderHttp;
use serde_json::json;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

fn exchange<T>(status: u16, body: &[u8], call: impl FnOnce(&str) -> T) -> T {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/fixture", listener.local_addr().unwrap());
    let body = body.to_vec();
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && Instant::now() < deadline =>
                {
                    std::thread::yield_now()
                }
                Err(error) => panic!("provider fixture accept: {error}"),
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        while !request.windows(4).any(|part| part == b"\r\n\r\n") {
            let count = stream.read(&mut buffer).unwrap();
            assert!(count > 0 && request.len() + count <= 8192);
            request.extend_from_slice(&buffer[..count]);
        }
        assert!(request.starts_with(b"GET /fixture HTTP/1.1\r\n"));
        let header = format!(
            "HTTP/1.1 {status} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        stream.write_all(header.as_bytes()).unwrap();
        stream.write_all(&body).unwrap();
        String::from_utf8(request).unwrap()
    });
    let result = call(&url);
    let request = server.join().unwrap().to_ascii_lowercase();
    assert!(
        request.contains("user-agent: mozilla/5.0 (logo-fetcher; local)")
            || (request.contains("x-cmc_pro_api_key: synthetic-wire-key")
                && request.contains("accept: application/json"))
    );
    result
}

#[test]
fn real_provider_status_headers_json_and_logo_bytes_keep_distinct_outcomes() {
    use _native::{coin_cmc as cmc, coin_paprika as paprika};
    let client = ProviderHttp::new(Duration::from_secs(3)).unwrap();
    let headers = cmc::headers("synthetic-wire-key").unwrap();
    for status in [400, 404] {
        assert!(matches!(
            exchange(status, b"not JSON", |url| cmc::http_json(
                &client, url, &headers
            ))
            .unwrap(),
            cmc::Response::Missing
        ));
    }
    for status in [401, 403] {
        assert_eq!(
            exchange(status, b"not JSON", |url| cmc::http_json(
                &client, url, &headers
            ))
            .err()
            .unwrap(),
            format!("CMC_API_KEY rejected by CoinMarketCap (HTTP {status})")
        );
    }
    assert!(matches!(
        exchange(402, b"not JSON", |url| paprika::http_json(&client, url)).unwrap(),
        paprika::Response::Quota
    ));
    assert!(exchange(200, b"not JSON", |url| paprika::http_json(&client, url)).is_err());
    let cmc::Response::Payload(value) = exchange(200, b"{\"data\":{}}", |url| {
        cmc::http_json(&client, url, &headers)
    })
    .unwrap() else {
        panic!("answered JSON must remain payload");
    };
    assert_eq!(value, json!({"data":{}}));
    assert_eq!(
        exchange(200, b"\x89PNG\x00\xff", |url| paprika::http_bytes(
            &client, url
        ))
        .unwrap()
        .unwrap(),
        b"\x89PNG\x00\xff"
    );
    assert!(paprika::http_json(&client, "http://user:password@127.0.0.1/fixture").is_err());
    assert!(cmc::logo(&client, &headers, "7&api_key=secret").is_err());
}
