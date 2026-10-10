//! Best-effort bot-name lookup is cancellable, bounded and isolated from panel mutations.
use _native::panel_session;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

#[test]
fn loopback_bot_lookup_accepts_only_verified_username_and_cancels_without_waiting() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        for body in [r#"{"ok":true,"result":{"username":"YourEmojiBot"}}"#, "[]"] {
            let (mut socket, _) = loop {
                match listener.accept() {
                    Ok(value) => break value,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(Instant::now() < deadline);
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(e) => panic!("{e}"),
                }
            };
            socket
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut bytes = [0; 2048];
            let n = socket.read(&mut bytes).unwrap();
            assert!(
                String::from_utf8_lossy(&bytes[..n]).starts_with("POST /bottest-only-token/getMe ")
            );
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
    });
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    assert_eq!(
        runtime.block_on(panel_session::detect_bot("test-only-token", &base)),
        Some("YourEmojiBot".into())
    );
    assert_eq!(
        runtime.block_on(panel_session::detect_bot("test-only-token", &base)),
        None
    );
    server.join().unwrap();
    let unused = TcpListener::bind("127.0.0.1:0").unwrap();
    let blocking = format!("http://{}", unused.local_addr().unwrap());
    runtime.block_on(async {
        let mut tasks = tokio::task::JoinSet::new();
        tasks.spawn(async move {
            panel_session::detect_bot("test-only-token", &blocking).await;
        });
        tokio::task::yield_now().await;
        tokio::time::timeout(Duration::from_secs(2), tasks.shutdown())
            .await
            .unwrap();
    });
}
