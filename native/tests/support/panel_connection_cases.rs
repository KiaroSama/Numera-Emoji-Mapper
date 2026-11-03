//! Original keep-alive contracts use the shared listener and the external stdlib HTTP parser.
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

struct Handle<'py>(Bound<'py, PyAny>);
impl Handle<'_> {
    fn close(&self) -> PyResult<()> {
        self.0.call_method0("close")?;
        Ok(())
    }
}
impl Drop for Handle<'_> {
    fn drop(&mut self) {
        if self.close().is_err() {
            eprintln!("ERROR: panel HTTP fixture could not close its handle; evidence retained");
        }
    }
}

fn connection<'py>(py: Python<'py>, port: u16, deadline: Instant) -> PyResult<Handle<'py>> {
    assert!(Instant::now() < deadline);
    let options = PyDict::new(py);
    options.set_item(
        "timeout",
        deadline
            .saturating_duration_since(Instant::now())
            .min(Duration::from_secs(5))
            .as_secs_f64(),
    )?;
    Ok(Handle(
        py.import("http.client")?
            .getattr("HTTPConnection")?
            .call(("127.0.0.1", port), Some(&options))?,
    ))
}

fn read<'py>(connection: &Handle<'py>, code: u16) -> PyResult<(Handle<'py>, Vec<u8>)> {
    let response = Handle(connection.0.call_method0("getresponse")?);
    let bytes = response
        .0
        .call_method1("read", (65537,))?
        .extract::<Vec<u8>>()?;
    assert!(
        bytes.len() <= 65536,
        "panel fixture response exceeded its bound"
    );
    response.close()?;
    assert_eq!(response.0.getattr("status")?.extract::<u16>()?, code);
    assert_eq!(response.0.getattr("version")?.extract::<u16>()?, 11);
    Ok((response, bytes))
}

pub fn check(port: u16, accepted: &AtomicUsize, db: &Path, deadline: Instant) {
    let before = _native::migration_signature::signature(db, &[]).unwrap();
    assert!(!db.with_file_name("pack_plan.json").exists());
    Python::attach(|py| -> PyResult<()> {
        let connected = accepted.load(Ordering::SeqCst);
        let client = connection(py, port, deadline)?;
        for _ in 0..2 {
            client.0.call_method1("request", ("GET", "/api/ping"))?;
            let (_, body) = read(&client, 200)?;
            assert_eq!(body, br#"{"ok":true}"#);
            assert_eq!(accepted.load(Ordering::SeqCst), connected + 1);
        }
        let socket = client.0.getattr("sock")?;
        assert!(!socket.is_none());
        let headers = PyDict::new(py);
        headers.set_item("X-Panel-Token", "persistence-fixture")?;
        headers.set_item("Content-Type", "application/json")?;
        let options = PyDict::new(py);
        options.set_item("headers", &headers)?;
        client.0.call_method(
            "request",
            (
                "POST",
                "/api/client-log",
                PyBytes::new(py, br#"{"events":[{"event":"ready"}]}"#),
            ),
            Some(&options),
        )?;
        let (response, body) = read(&client, 204)?;
        assert!(body.is_empty());
        assert!(
            response
                .0
                .call_method1("getheader", ("Content-Length",))?
                .is_none()
        );
        drop(response);
        client.0.call_method1("request", ("GET", "/api/ping"))?;
        let (_, body) = read(&client, 200)?;
        assert_eq!(body, br#"{"ok":true}"#);
        assert!(client.0.getattr("sock")?.is(&socket));
        assert_eq!(accepted.load(Ordering::SeqCst), connected + 1);
        client.close()?;
        drop(client);

        let client = connection(py, port, deadline)?;
        headers.set_item("Content-Length", "99999999")?;
        client.0.call_method(
            "request",
            ("POST", "/api/save", PyBytes::new(py, b"")),
            Some(&options),
        )?;
        let (response, body) = read(&client, 413)?;
        assert_eq!(
            response
                .0
                .call_method1("getheader", ("Connection",))?
                .extract::<String>()?,
            "close"
        );
        assert!(client.0.getattr("sock")?.is_none());
        assert!(
            std::str::from_utf8(&body)
                .unwrap()
                .contains("body too large")
        );
        drop(response);
        client.close()?;
        drop(client);

        let client = connection(py, port, deadline)?;
        client.0.call_method1("putrequest", ("POST", "/api/save"))?;
        for (name, value) in [
            ("X-Panel-Token", "persistence-fixture"),
            ("Content-Type", "application/json"),
        ] {
            client.0.call_method1("putheader", (name, value))?;
        }
        client.0.call_method0("endheaders")?;
        let (response, body) = read(&client, 411)?;
        assert_eq!(
            response
                .0
                .call_method1("getheader", ("Connection",))?
                .extract::<String>()?,
            "close"
        );
        assert!(client.0.getattr("sock")?.is_none());
        assert!(
            std::str::from_utf8(&body)
                .unwrap()
                .contains("Content-Length required")
        );
        client.close()?;
        Ok(())
    })
    .unwrap();
    assert_eq!(
        _native::migration_signature::signature(db, &[]).unwrap(),
        before
    );
    assert!(!db.with_file_name("pack_plan.json").exists());
    assert!(Instant::now() < deadline);
}
