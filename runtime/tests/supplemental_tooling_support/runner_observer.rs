//! Standalone stdlib fixture for the actual native test runner; no interpreter or copied guard policy.
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream},
    process::{Command, Stdio},
    time::Duration,
};
// Acknowledged descendant is intentionally left to the actual outer guard's
// owner; waiting here would remove the custody regression being exercised.
#[allow(clippy::zombie_processes)]
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let mode = &args[0];
    let root = std::path::Path::new(&args[1]);
    if mode == "descendant" {
        std::fs::write(
            root.join("runner-descendant.pid"),
            std::process::id().to_string(),
        )
        .unwrap();
        std::thread::sleep(Duration::from_secs(20));
        std::process::exit(91);
    }
    assert!(args.iter().any(|value| value == "--test-threads=1"));
    std::fs::write(root.join("runner-observed"), b"real Rust fixture executed").unwrap();
    match mode.as_str() {
        "environment" => {
            for (key, value) in std::env::vars() {
                println!("ENV|{key}|{value}");
            }
            println!("CWD|{}", std::env::current_dir().unwrap().display());
            println!("ARGV|{}", args.join("\n"));
        }
        "failure" => {
            println!("stdout café 😀");
            eprintln!("stderr failure evidence");
            std::process::exit(7);
        }
        "early" => std::process::exit(9),
        "network" => {
            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            let local = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
            let accepted = listener.accept().unwrap();
            drop((local, accepted, listener));
            assert!(
                TcpStream::connect_timeout(
                    &SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 0, 2, 1)), 443),
                    Duration::from_millis(300)
                )
                .is_err()
            );
            println!("private loopback works; outbound refused");
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args(["descendant", root.to_str().unwrap()])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let until = std::time::Instant::now() + Duration::from_secs(3);
            while !root.join("runner-descendant.pid").is_file() {
                if std::time::Instant::now() >= until {
                    child.kill().unwrap();
                    child.wait().unwrap();
                    panic!("descendant never acknowledged");
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            // The process intentionally exits now: the real outer guard must reap
            // this acknowledged detached-stdio child before returning success.
            std::process::exit(0);
        }
        _ => panic!("unknown observer mode"),
    }
}
