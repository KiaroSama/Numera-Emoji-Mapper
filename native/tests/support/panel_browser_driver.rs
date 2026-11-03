//! Thread-bound typed external Playwright API; browser/page closure is never delegated to Python scripts.
use super::media::{self, Animated};
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

#[pyclass(dict)]
struct RouteGate {
    origin: String,
    png: Vec<u8>,
    orders: Arc<Mutex<Vec<Value>>>,
    model: Option<String>,
    animated: Option<Arc<Animated>>,
}
#[pymethods]
impl RouteGate {
    fn __call__(&self, route: &Bound<'_, PyAny>) -> PyResult<()> {
        let request = route.getattr("request")?;
        let text: String = request.getattr("url")?.extract()?;
        let parsed = reqwest::Url::parse(&text)
            .map_err(|_| PyValueError::new_err("invalid browser request URL"))?;
        if parsed.origin().ascii_serialization() != self.origin {
            route.call_method1("abort", ("blockedbyclient",))?;
            return Ok(());
        }
        if parsed.path() == "/"
            && let Some(model) = &self.model
        {
            let fetch = PyDict::new(route.py());
            fetch.set_item("timeout", 10_000)?;
            fetch.set_item("max_redirects", 0)?;
            let response = route.call_method("fetch", (), Some(&fetch))?;
            if response.getattr("status")?.extract::<u16>()? != 200 {
                return Err(PyRuntimeError::new_err(
                    "model fixture did not fetch the real panel",
                ));
            }
            let html: String = response.call_method0("text")?.extract()?;
            let marker = "<script id=\"items-data\" type=\"application/json\">";
            let start = html
                .find(marker)
                .ok_or_else(|| PyValueError::new_err("panel model marker missing"))?
                + marker.len();
            let end = start
                + html[start..]
                    .find("</script>")
                    .ok_or_else(|| PyValueError::new_err("panel model terminator missing"))?;
            let body = format!("{}{}{}", &html[..start], model, &html[end..]);
            let options = PyDict::new(route.py());
            options.set_item("status", 200)?;
            options.set_item("content_type", "text/html; charset=utf-8")?;
            options.set_item("body", body)?;
            route.call_method("fulfill", (), Some(&options))?;
        } else if parsed.path().starts_with("/img/") || parsed.path().starts_with("/preview/") {
            // Same original media substitution: these cases test queues/gestures, not codecs.
            let options = PyDict::new(route.py());
            options.set_item("status", 200)?;
            let (content_type, bytes) = if let Some(animated) = &self.animated
                && parsed.path().starts_with("/preview/")
            {
                let still = parsed
                    .query_pairs()
                    .any(|(key, value)| key == "still" && value == "1");
                (
                    "image/webp",
                    if still {
                        &animated.still
                    } else {
                        &animated.moving
                    },
                )
            } else {
                ("image/png", &self.png)
            };
            options.set_item("content_type", content_type)?;
            options.set_item("body", PyBytes::new(route.py(), bytes))?;
            route.call_method("fulfill", (), Some(&options))?;
        } else {
            if parsed.path() == "/api/order"
                && request.getattr("method")?.extract::<String>()? == "POST"
            {
                let body: String = request.getattr("post_data")?.extract()?;
                let body = serde_json::from_str(&body)
                    .map_err(|_| PyValueError::new_err("browser order request is not JSON"))?;
                self.orders
                    .lock()
                    .map_err(|_| PyRuntimeError::new_err("browser request accounting poisoned"))?
                    .push(body);
            }
            route.call_method0("continue_")?;
        }
        Ok(())
    }
}

#[pyclass(dict)]
struct PageError(Arc<Mutex<Vec<String>>>);
#[pymethods]
impl PageError {
    fn __call__(&self, error: &Bound<'_, PyAny>) -> PyResult<()> {
        let text = error.str()?.to_str()?.to_owned();
        self.0
            .lock()
            .map_err(|_| PyRuntimeError::new_err("page error accounting poisoned"))?
            .push(text);
        Ok(())
    }
}

fn signature(callback: &Bound<'_, PyAny>, parameter: &str) -> PyResult<()> {
    // Playwright inspects and caches attributes on callbacks; bare PyCFunction cannot carry them.
    let inspect = callback.py().import("inspect")?;
    let class = inspect.getattr("Parameter")?;
    let argument = class.call1((parameter, class.getattr("POSITIONAL_OR_KEYWORD")?))?;
    callback.setattr(
        "__signature__",
        inspect.getattr("Signature")?.call1((vec![argument],))?,
    )
}

const STUB: &str = include_str!("../fixtures/panel-browser-stub.js");

#[derive(Default)]
pub struct Case<'a> {
    pub clock: bool,
    pub items: Option<&'a Value>,
    pub init: Option<&'a str>,
    pub reduced_motion: bool,
    pub animated: bool,
}
impl From<bool> for Case<'_> {
    fn from(clock: bool) -> Self {
        Self {
            clock,
            ..Self::default()
        }
    }
}

pub struct Browser<'py> {
    playwright: Bound<'py, PyAny>,
    browser: Bound<'py, PyAny>,
    root: PathBuf,
    animated: OnceLock<Arc<Animated>>,
    completed: RefCell<BTreeSet<String>>,
    closed: bool,
}
impl<'py> Browser<'py> {
    pub fn start(py: Python<'py>, root: &Path) -> PyResult<Self> {
        let version: String = py
            .import("importlib.metadata")?
            .call_method1("version", ("playwright",))?
            .extract()?;
        if version != "1.63.0" {
            return Err(PyRuntimeError::new_err(
                "native browser acceptance requires playwright==1.63.0",
            ));
        }
        let playwright = py
            .import("playwright.sync_api")?
            .call_method0("sync_playwright")?
            .call_method0("start")?;
        let result = (|| -> PyResult<Bound<'py, PyAny>> {
            let options = PyDict::new(py);
            options.set_item("headless", true)?;
            options.set_item("timeout", 30_000)?;
            options.set_item("downloads_path", root.join("browser-downloads"))?;
            if let Ok(channel) = std::env::var("NUMERA_EMOJI_MAPPER_BROWSER_CHANNEL") {
                if !channel.is_empty() {
                    options.set_item("channel", channel)?;
                }
            }
            playwright
                .getattr("chromium")?
                .call_method("launch", (), Some(&options))
        })();
        let browser = match result {
            Ok(browser) => browser,
            Err(error) => {
                if let Err(cleanup) = playwright.call_method0("stop") {
                    eprintln!("ERROR: browser startup cleanup failed: {cleanup}");
                }
                return Err(error);
            }
        };
        Ok(Self {
            playwright,
            browser,
            root: root.into(),
            animated: OnceLock::new(),
            completed: RefCell::new(BTreeSet::new()),
            closed: false,
        })
    }

    pub fn case<'a>(
        &self,
        name: &str,
        url: &str,
        case: impl Into<Case<'a>>,
        execute: impl FnOnce(&Page<'py>) -> PyResult<()>,
    ) -> PyResult<()> {
        if self.completed.borrow().contains(name) {
            return Err(PyValueError::new_err(
                "browser scenario was already completed",
            ));
        }
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(PyValueError::new_err("invalid browser evidence name"));
        }
        let case = case.into();
        let py = self.browser.py();
        let animated = if case.animated {
            if self.animated.get().is_none() {
                let _ = self.animated.set(Arc::new(media::animated(py)?));
            }
            self.animated.get().cloned()
        } else {
            None
        };
        let options = PyDict::new(py);
        if case.reduced_motion {
            options.set_item("reduced_motion", "reduce")?;
        }
        let viewport = PyDict::new(py);
        viewport.set_item("width", 1200)?;
        viewport.set_item("height", 900)?;
        options.set_item("viewport", viewport)?;
        options.set_item("service_workers", "block")?;
        let context = self
            .browser
            .call_method("new_context", (), Some(&options))?;
        let mut page = Page {
            context,
            page: None,
            errors: Arc::new(Mutex::new(Vec::new())),
            orders: Arc::new(Mutex::new(Vec::new())),
            evidence: self.root.join(name),
            animated,
            closed: false,
        };
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> PyResult<()> {
            page.open(url, &case)?;
            execute(&page)?;
            // Drain queued browser events before inspecting the synchronous error ledger.
            page.eval("0")?;
            let errors = page
                .errors
                .lock()
                .map_err(|_| PyRuntimeError::new_err("page error accounting poisoned"))?;
            if !errors.is_empty() {
                return Err(PyRuntimeError::new_err(format!(
                    "browser page errors: {errors:?}"
                )));
            }
            eprintln!("browser case completed: {name}");
            Ok(())
        }));
        let failed = !matches!(&result, Ok(Ok(())));
        let closed = page.close(failed);
        match result {
            Ok(Err(error)) => {
                if let Err(cleanup) = closed {
                    eprintln!("ERROR: browser failure cleanup: {cleanup}");
                }
                Err(error)
            }
            Ok(Ok(())) => {
                closed?;
                if !self.completed.borrow_mut().insert(name.into()) {
                    return Err(PyValueError::new_err(
                        "browser scenario executed more than once",
                    ));
                }
                Ok(())
            }
            Err(panic) => {
                if let Err(error) = closed {
                    eprintln!("ERROR: browser failure cleanup: {error}");
                }
                std::panic::resume_unwind(panic)
            }
        }
    }

    pub fn completed(&self) -> BTreeSet<String> {
        self.completed.borrow().clone()
    }

    pub fn close(&mut self) -> PyResult<()> {
        if self.closed {
            return Ok(());
        }
        let browser = self.browser.call_method0("close");
        let driver = self.playwright.call_method0("stop");
        browser?;
        driver?;
        self.closed = true;
        Ok(())
    }
}
impl Drop for Browser<'_> {
    fn drop(&mut self) {
        if let Err(error) = self.close() {
            eprintln!("ERROR: native browser close failed: {error}");
        }
    }
}

pub struct Page<'py> {
    context: Bound<'py, PyAny>,
    page: Option<Bound<'py, PyAny>>,
    errors: Arc<Mutex<Vec<String>>>,
    orders: Arc<Mutex<Vec<Value>>>,
    evidence: PathBuf,
    animated: Option<Arc<Animated>>,
    closed: bool,
}
impl<'py> Page<'py> {
    fn object(&self) -> PyResult<&Bound<'py, PyAny>> {
        self.page
            .as_ref()
            .ok_or_else(|| PyRuntimeError::new_err("browser page is not open"))
    }

    fn open(&mut self, url: &str, case: &Case<'_>) -> PyResult<()> {
        let py = self.context.py();
        let origin =
            reqwest::Url::parse(url).map_err(|_| PyValueError::new_err("invalid fixture URL"))?;
        if origin.scheme() != "http" || origin.host_str() != Some("127.0.0.1") {
            return Err(PyValueError::new_err(
                "browser fixture requires numeric loopback",
            ));
        }
        self.context
            .call_method1("set_default_timeout", (10_000,))?;
        self.context
            .call_method1("set_default_navigation_timeout", (10_000,))?;
        let trace = PyDict::new(py);
        trace.set_item("screenshots", true)?;
        trace.set_item("snapshots", true)?;
        self.context
            .getattr("tracing")?
            .call_method("start", (), Some(&trace))?;
        let route = Py::new(
            py,
            RouteGate {
                origin: origin.origin().ascii_serialization(),
                png: std::fs::read(
                    self.evidence
                        .parent()
                        .ok_or_else(|| PyRuntimeError::new_err("browser fixture parent missing"))?
                        .join("collection/media/static/i0.png"),
                )?,
                orders: self.orders.clone(),
                animated: self.animated.clone(),
                model: case.items.map(|items| {
                    items
                        .to_string()
                        .replace('<', "\\u003c")
                        .replace('>', "\\u003e")
                        .replace('&', "\\u0026")
                        .replace('\u{2028}', "\\u2028")
                        .replace('\u{2029}', "\\u2029")
                }),
            },
        )?
        .into_bound(py)
        .into_any();
        signature(&route, "route")?;
        self.context.call_method1("route", ("**/*", route))?;
        let page = self.context.call_method0("new_page")?;
        self.page = Some(page.clone());
        let handler = Py::new(py, PageError(self.errors.clone()))?
            .into_bound(py)
            .into_any();
        signature(&handler, "error")?;
        page.call_method1("on", ("pageerror", handler))?;
        if case.clock {
            page.getattr("clock")?.call_method0("install")?;
        }
        page.call_method1("add_init_script", (STUB,))?;
        if let Some(init) = case.init {
            page.call_method1("add_init_script", (init,))?;
        }
        self.navigate(url)?;
        let expected = case.items.and_then(Value::as_array).map_or(8, Vec::len);
        self.wait(&format!("ITEMS.length === {expected}"))
    }

    fn navigate(&self, url: &str) -> PyResult<()> {
        let options = PyDict::new(self.context.py());
        options.set_item("wait_until", "load")?;
        options.set_item("timeout", 10_000)?;
        let response = self.object()?.call_method("goto", (url,), Some(&options))?;
        if response.is_none() || response.getattr("status")?.extract::<u16>()? != 200 {
            return Err(PyRuntimeError::new_err(
                "browser panel navigation did not return200",
            ));
        }
        self.wait("typeof ITEMS !== 'undefined'")
    }

    pub fn wait(&self, expression: &str) -> PyResult<()> {
        self.object()?
            .call_method1("wait_for_function", (expression,))?;
        Ok(())
    }
    pub fn locator(&self, selector: &str) -> PyResult<Bound<'py, PyAny>> {
        self.object()?.call_method1("locator", (selector,))
    }
    pub fn click_at(&self, selector: &str, index: usize, shift: bool) -> PyResult<()> {
        let options = PyDict::new(self.context.py());
        if shift {
            options.set_item("modifiers", vec!["Shift"])?;
        }
        self.locator(selector)?
            .call_method1("nth", (index,))?
            .call_method("click", (), Some(&options))?;
        Ok(())
    }
    pub fn hover(&self, selector: &str, index: usize) -> PyResult<()> {
        self.locator(selector)?
            .call_method1("nth", (index,))?
            .call_method0("hover")?;
        Ok(())
    }
    pub fn pointer_away(&self) -> PyResult<()> {
        self.object()?
            .getattr("mouse")?
            .call_method1("move", (1, 1))?;
        Ok(())
    }
    pub fn key(&self, key: &str) -> PyResult<()> {
        self.object()?
            .getattr("keyboard")?
            .call_method1("press", (key,))?;
        Ok(())
    }
    pub fn double_click(&self, selector: &str) -> PyResult<()> {
        self.locator(selector)?.call_method0("dblclick")?;
        Ok(())
    }
    pub fn viewport(&self, width: u32, height: u32) -> PyResult<()> {
        let dimensions = PyDict::new(self.context.py());
        dimensions.set_item("width", width)?;
        dimensions.set_item("height", height)?;
        self.object()?
            .call_method1("set_viewport_size", (dimensions,))?;
        Ok(())
    }
    pub fn screenshot_pixels(&self, selector: &str, index: usize) -> PyResult<Vec<(u8, u8, u8)>> {
        let bytes: Vec<u8> = self
            .locator(selector)?
            .call_method1("nth", (index,))?
            .call_method0("screenshot")?
            .extract()?;
        media::checker_pixels(self.context.py(), &bytes)
    }
    pub fn import_draft(&self, doc: &Value) -> PyResult<()> {
        let file = PyDict::new(self.context.py());
        file.set_item("name", "numera-emoji-mapper-draft.json")?;
        file.set_item("mimeType", "application/json")?;
        file.set_item(
            "buffer",
            PyBytes::new(self.context.py(), doc.to_string().as_bytes()),
        )?;
        self.locator("#draftFile")?
            .call_method1("set_input_files", (vec![file],))?;
        Ok(())
    }
    pub fn tray_drag(&self, from: usize, target: Option<usize>) -> PyResult<()> {
        let transfer = self
            .object()?
            .call_method1("evaluate_handle", ("() => new DataTransfer()",))?;
        let payload = PyDict::new(self.context.py());
        payload.set_item("dataTransfer", &transfer)?;
        self.locator("#holdCards .hcard")?
            .call_method1("nth", (from,))?
            .call_method1("dispatch_event", ("dragstart", &payload))?;
        if let Some(index) = target {
            let card = self
                .locator("#grid .card:not(.logo)")?
                .call_method1("nth", (index,))?;
            card.call_method1("dispatch_event", ("dragover", &payload))?;
            card.call_method1("dispatch_event", ("drop", &payload))?;
        }
        transfer.call_method0("dispose")?;
        Ok(())
    }
    pub fn eval(&self, expression: &str) -> PyResult<()> {
        self.object()?.call_method1("evaluate", (expression,))?;
        Ok(())
    }
    pub fn value(&self, expression: &str) -> PyResult<Value> {
        let observed = self.object()?.call_method1("evaluate", (expression,))?;
        let encoded: String = self
            .context
            .py()
            .import("json")?
            .call_method1("dumps", (observed,))?
            .extract()?;
        serde_json::from_str(&encoded)
            .map_err(|_| PyValueError::new_err("browser observation is not JSON"))
    }
    pub fn tick(&self, milliseconds: u32) -> PyResult<()> {
        self.object()?
            .getattr("clock")?
            .call_method1("run_for", (milliseconds,))?;
        Ok(())
    }
    pub fn click(&self, selector: &str) -> PyResult<()> {
        self.object()?.call_method1("click", (selector,))?;
        Ok(())
    }
    pub fn click_card(&self, index: usize) -> PyResult<()> {
        // Original tests intentionally address card order, not labels.
        self.click_at("#grid .card", index, false)
    }
    pub fn drag(&self, from: usize, to: usize) -> PyResult<()> {
        let cards = self.object()?.call_method1("locator", ("#grid .card",))?;
        cards
            .call_method1("nth", (from,))?
            .call_method1("drag_to", (cards.call_method1("nth", (to,))?,))?;
        Ok(())
    }
    pub fn posted_orders(&self) -> PyResult<Vec<Value>> {
        self.orders
            .lock()
            .map(|orders| orders.clone())
            .map_err(|_| PyRuntimeError::new_err("browser request accounting poisoned"))
    }
    pub fn wait_saved(&self) -> PyResult<()> {
        let options = PyDict::new(self.context.py());
        options.set_item("timeout", 20_000)?;
        // Waiting inside Playwright services route callbacks; sleeping in Rust does not.
        self.object()?.call_method(
            "wait_for_function",
            ("pendingOrder === null && orderFlight === 0",),
            Some(&options),
        )?;
        Ok(())
    }
    pub fn reload(&self) -> PyResult<()> {
        let options = PyDict::new(self.context.py());
        options.set_item("wait_until", "load")?;
        self.object()?.call_method("reload", (), Some(&options))?;
        Ok(())
    }
    fn close(&mut self, failed: bool) -> PyResult<()> {
        if self.closed {
            return Ok(());
        }
        if failed {
            if let Some(page) = &self.page {
                let options = PyDict::new(self.context.py());
                options.set_item("path", self.evidence.with_extension("png"))?;
                options.set_item("timeout", 2000)?;
                if let Err(error) = page.call_method("screenshot", (), Some(&options)) {
                    eprintln!("ERROR: browser failure screenshot unavailable: {error}");
                }
            }
        }
        let options = PyDict::new(self.context.py());
        if failed {
            options.set_item("path", self.evidence.with_extension("zip"))?;
        }
        let trace = self
            .context
            .getattr("tracing")
            .and_then(|tracing| tracing.call_method("stop", (), Some(&options)));
        let context = self.context.call_method0("close");
        if context.is_ok() {
            self.closed = true;
        }
        if let Err(error) = &context {
            eprintln!("ERROR: browser context closure failed: {error}");
        }
        trace?;
        context?;
        Ok(())
    }
}
impl Drop for Page<'_> {
    fn drop(&mut self) {
        if let Err(error) = self.close(true) {
            eprintln!("ERROR: native browser context close failed: {error}");
        }
    }
}
