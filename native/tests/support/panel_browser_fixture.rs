//! One real Rust router/catalog per browser cohort; no application subprocess or Python harness.
use _native::{
    catalog::Catalog,
    logging::RunLog,
    media_native_adapter::NativeMediaAdapter,
    panel_events::Events,
    panel_http::{self, Panel},
    panel_preview::Previews,
};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

pub struct Server {
    pub root: PathBuf,
    pub url: String,
    database: PathBuf,
    runtime: Option<tokio::runtime::Runtime>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<std::io::Result<()>>>,
    threads: Arc<AtomicUsize>,
    port: u16,
    closed: bool,
}
impl Server {
    pub fn start(py: Python<'_>) -> PyResult<Self> {
        let project = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .ok_or_else(|| PyRuntimeError::new_err("cannot locate browser project"))?;
        let parent = std::env::var_os("NUMERA_TEST_WORKSPACE").ok_or_else(|| {
            PyValueError::new_err("browser acceptance requires the native test-runner")
        })?;
        let parent = _native::tooling_environment::physical_root(Path::new(&parent))
            .map_err(PyValueError::new_err)?;
        let storage =
            _native::tooling_environment::confined_directory(project, "logs/tmp/tooling", false)
                .map_err(PyValueError::new_err)?;
        if parent.parent() != Some(storage.as_path()) {
            return Err(PyValueError::new_err(
                "browser workspace must belong to the project runner",
            ));
        }
        let root = parent.join(format!("native-browser-{}", std::process::id()));
        std::fs::create_dir(&root)?;
        let root =
            _native::tooling_environment::physical_root(&root).map_err(PyValueError::new_err)?;
        std::fs::create_dir(root.join("assets"))?;
        for name in [
            "panel.html",
            "panel-grid.js",
            "panel-motion.js",
            "panel-save.js",
            "panel-drag.js",
            "panel-actions.js",
            "panel-holding.js",
            "panel-draft.js",
            "logo-128.png",
        ] {
            std::fs::copy(
                project.join("assets").join(name),
                root.join("assets").join(name),
            )?;
        }
        let data = root.join("collection");
        std::fs::create_dir_all(data.join("media/static"))?;
        let database = data.join("catalog.db");
        let mut catalog =
            Catalog::open(&database, "browser-fixture", &root).map_err(PyRuntimeError::new_err)?;
        for i in 0..8 {
            let file = data.join(format!("media/static/i{i}.png"));
            let image = py
                .import("PIL.Image")?
                .call_method1("new", ("RGBA", (40, 40), (9 * i, 20, 30, 255)))?;
            let saved = image.call_method1("save", (&file, "PNG"));
            let closed = image.call_method0("close");
            saved?;
            closed?;
            catalog
                .insert(
                    &json!({"content_key":format!("s:item{i:030}"),"fmt":"static",
                "file_path":file,"keywords":[format!("item{i}")]}),
                    "browser-fixture",
                )
                .map_err(PyRuntimeError::new_err)?;
        }
        drop(catalog);
        let log = Arc::new(RunLog::open(&root, "browser-server", vec![]));
        let panel = Panel {
            root: root.clone(),
            db: database.clone(),
            token: "native-browser-test-token".into(),
            options: json!({"keep_sets":{},"show_published":false,"branded":false,
                "logo_path":root.join("assets/logo-128.png"),"no_open":true,"preview_fps":15}),
            branding: None,
            detected_bot: Mutex::new("YourEmojiBot".into()),
            view: Mutex::new(json!({"view":[],"by_key":{},"hidden":0})),
            session: json!({"application":"numera-emoji-mapper-panel","catalog":"native-browser-fixture","all":false,"packs":[]}),
            utc: _native::logging::iso_utc,
            log,
            events: Mutex::new(Events::default()),
            // Initial fixture is static and included: no codec may be required by this cohort.
            previews: Previews::new(
                NativeMediaAdapter {
                    executable: "must-not-run-browser-codec".into(),
                    root: root.clone(),
                    scratch: data.join("codec"),
                },
                &data,
                1,
            ),
        };
        let threads = Arc::new(AtomicUsize::new(0));
        let started = threads.clone();
        let stopped = threads.clone();
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .max_blocking_threads(2)
            .on_thread_start(move || {
                started.fetch_add(1, Ordering::SeqCst);
            })
            .on_thread_stop(move || {
                stopped.fetch_sub(1, Ordering::SeqCst);
            })
            .enable_all()
            .build()
            .map_err(|_| PyRuntimeError::new_err("cannot create browser server runtime"))?;
        let listener = runtime.block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))?;
        let port = listener.local_addr()?.port();
        let (stop, done) = tokio::sync::oneshot::channel();
        let task = runtime.spawn(panel_http::serve_listener(panel, listener, async {
            let _ = done.await;
        }));
        Ok(Self {
            root,
            database,
            url: format!("http://127.0.0.1:{port}/"),
            runtime: Some(runtime),
            stop: Some(stop),
            task: Some(task),
            threads,
            port,
            closed: false,
        })
    }

    pub fn order(&self) -> PyResult<Vec<String>> {
        let db = rusqlite::Connection::open_with_flags(
            &self.database,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(|_| PyRuntimeError::new_err("cannot observe browser catalog"))?;
        db.busy_timeout(Duration::from_secs(1))
            .map_err(|_| PyRuntimeError::new_err("cannot bound browser catalog read"))?;
        let mut query = db
            .prepare("SELECT content_key FROM items ORDER BY position,rowid")
            .map_err(|_| PyRuntimeError::new_err("cannot query browser catalog order"))?;
        let rows = query
            .query_map([], |row| row.get(0))
            .map_err(|_| PyRuntimeError::new_err("cannot read browser catalog order"))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|_| PyRuntimeError::new_err("invalid browser catalog order"))
    }

    pub fn excluded(&self) -> PyResult<Vec<String>> {
        let db = rusqlite::Connection::open_with_flags(
            &self.database,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .map_err(|_| PyRuntimeError::new_err("cannot observe browser inclusion"))?;
        db.busy_timeout(Duration::from_secs(1))
            .map_err(|_| PyRuntimeError::new_err("cannot bound browser inclusion read"))?;
        let mut query = db
            .prepare("SELECT content_key FROM items WHERE included=0 ORDER BY content_key")
            .map_err(|_| PyRuntimeError::new_err("cannot query browser inclusion"))?;
        let rows = query
            .query_map([], |row| row.get(0))
            .map_err(|_| PyRuntimeError::new_err("cannot read browser inclusion"))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|_| PyRuntimeError::new_err("invalid browser inclusion"))
    }

    pub fn reset(&self) -> PyResult<()> {
        let mut catalog = Catalog::open(&self.database, "browser-reset", &self.root)
            .map_err(PyRuntimeError::new_err)?;
        catalog.inclusion(&[]).map_err(PyRuntimeError::new_err)?;
        let mut keys = catalog
            .all(None)
            .map_err(PyRuntimeError::new_err)?
            .iter()
            .filter_map(|item| item["content_key"].as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        keys.sort();
        catalog.order(&keys).map_err(PyRuntimeError::new_err)?;
        drop(catalog);
        // A real Save also persisted intent; resetting only rows leaks its overlay into the next case.
        let plan = self.database.with_file_name("pack_plan.json");
        match std::fs::symlink_metadata(&plan) {
            Ok(_) => {
                _native::tooling_environment::reject_links(
                    &plan,
                    "browser reset cannot follow plan links",
                )
                .map_err(PyValueError::new_err)?;
                if !plan.is_file() {
                    return Err(PyValueError::new_err("browser plan must be a regular file"));
                }
                std::fs::remove_file(plan)?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Ok(())
    }

    pub fn close(&mut self) -> PyResult<()> {
        if self.closed {
            return Ok(());
        }
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        let runtime = self
            .runtime
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("browser server owner missing"))?;
        let result = if let Some(mut task) = self.task.take() {
            match runtime
                .block_on(async { tokio::time::timeout(Duration::from_secs(5), &mut task).await })
            {
                Ok(Ok(Ok(()))) => Ok(()),
                outcome => {
                    task.abort();
                    let _ = runtime.block_on(async {
                        tokio::time::timeout(Duration::from_secs(2), task).await
                    });
                    Err(PyRuntimeError::new_err(format!(
                        "browser server shutdown failed: {outcome:?}; fixture retained"
                    )))
                }
            }
        } else {
            Err(PyRuntimeError::new_err(
                "browser server task missing; fixture retained",
            ))
        };
        // Even failed/aborted shutdown must not drop a runtime with an unbounded thread join.
        runtime.shutdown_timeout(Duration::from_secs(2));
        result?;
        if self.threads.load(Ordering::SeqCst) != 0 {
            return Err(PyRuntimeError::new_err(
                "browser runtime threads did not stop; fixture retained",
            ));
        }
        // A closed HTTP listener can still have TIME_WAIT sockets; rebinding without
        // SO_REUSEADDR would report those as a leaked listener on Unix.
        let address = std::net::SocketAddr::from((std::net::Ipv4Addr::LOCALHOST, self.port));
        match std::net::TcpStream::connect_timeout(&address, Duration::from_millis(250)) {
            Err(error) if error.kind() == std::io::ErrorKind::ConnectionRefused => {}
            _ => {
                return Err(PyRuntimeError::new_err(
                    "browser listener shutdown is unverified; fixture retained",
                ));
            }
        }
        self.closed = true;
        Ok(())
    }

    pub fn clean(&self) -> PyResult<()> {
        if !self.closed || std::thread::panicking() {
            return Err(PyRuntimeError::new_err(
                "browser fixture is not verified stopped; retained",
            ));
        }
        let codecs = self.root.join("collection/codec");
        if codecs.exists() {
            return Err(PyRuntimeError::new_err(
                "unexpected browser codec activity; fixture retained",
            ));
        }
        fn inspect(path: &Path, depth: usize, remaining: &mut usize) -> PyResult<()> {
            if depth > 16 || *remaining == 0 {
                return Err(PyValueError::new_err(
                    "browser cleanup inventory exceeded its bound; fixture retained",
                ));
            }
            *remaining -= 1;
            _native::tooling_environment::reject_links(path, "browser cleanup cannot follow links")
                .map_err(PyValueError::new_err)?;
            let metadata = std::fs::symlink_metadata(path)?;
            if metadata.is_dir() {
                if _native::process_owner::cleanup_pending(path) {
                    return Err(PyValueError::new_err(
                        "browser process custody remains unresolved; fixture retained",
                    ));
                }
                for entry in std::fs::read_dir(path)? {
                    inspect(&entry?.path(), depth + 1, remaining)?;
                }
            } else if !metadata.is_file() {
                return Err(PyValueError::new_err(
                    "browser cleanup found unsupported storage; fixture retained",
                ));
            }
            Ok(())
        }
        inspect(&self.root, 0, &mut 2048)?;
        // Browser.close returned, but only the outer descendant owner can prove no
        // crashed or closed-stdio browser process still uses this directory.
        use std::io::Write;
        let mut ready = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(self.root.join("cleanup-ready"))?;
        ready.write_all(b"native-browser-stopped-v1")?;
        ready.sync_all()?;
        Ok(())
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        if !self.closed
            && let Err(error) = self.close()
        {
            eprintln!("ERROR: {error}");
        }
        // Success cleanup is explicit after every browser/catalog assertion; failure keeps evidence.
    }
}
