//! Loopback curation HTTP service; Telegram is never called from a save request.
use crate::{catalog::Catalog, panel_save, panel_view};
use axum::{
    Router,
    body::{Body, to_bytes},
    extract::{Request, State},
    http::{HeaderMap, Method, StatusCode},
    response::Response,
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub struct Panel {
    pub root: PathBuf,
    pub db: PathBuf,
    pub token: String,
    pub options: Value,
    pub branding: Option<(String, String, Vec<String>)>,
    pub detected_bot: Mutex<String>,
    pub view: Mutex<Value>,
    pub session: Value,
    pub utc: fn() -> String,
    pub log: Arc<crate::logging::RunLog>,
    pub events: Mutex<crate::panel_events::Events>,
    pub previews: crate::panel_preview::Previews,
}
fn response(code: u16, body: impl Into<Body>, mime: &str, cache: &str) -> Response {
    let mut response = Response::new(body.into());
    *response.status_mut() =
        StatusCode::from_u16(code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    let headers = response.headers_mut();
    for (name, value) in [
        ("content-type", mime),
        ("cache-control", cache),
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
    ] {
        if let Ok(value) = value.parse() {
            headers.insert(name, value);
        }
    }
    if mime.starts_with("text/html") {
        headers.insert("x-frame-options", "DENY".parse().unwrap());
        headers.insert(
            "content-security-policy",
            "frame-ancestors 'none'".parse().unwrap(),
        );
    }
    response
}
fn answer(code: u16, value: Value) -> Response {
    response(code, value.to_string(), "application/json", "no-store")
}
fn loopback(raw: &str) -> bool {
    let host = raw.rsplit("://").next().unwrap_or(raw);
    let host = if host.starts_with('[') {
        host.split(']')
            .next()
            .map(|s| format!("{s}]"))
            .unwrap_or_default()
    } else {
        host.rsplit_once(':').map_or(host, |(h, _)| h).to_owned()
    };
    ["127.0.0.1", "localhost", "::1", "[::1]"].contains(&host.as_str())
}
fn header<'a>(headers: &'a HeaderMap, name: &str) -> &'a str {
    headers
        .get(name)
        .and_then(|h| h.to_str().ok())
        .unwrap_or("")
}
fn same_token(a: &str, b: &str) -> bool {
    let mut difference = a.len() ^ b.len();
    for (x, y) in a.as_bytes().iter().zip(b.as_bytes()) {
        difference |= usize::from(x ^ y)
    }
    difference == 0
}
fn script_json(value: &Value) -> String {
    value
        .to_string()
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026")
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029")
}
fn view_options(panel: &Panel) -> Result<Value, String> {
    let mut options = panel.options.clone();
    if let Some((_, _, bots)) = &panel.branding {
        let bot = panel
            .detected_bot
            .lock()
            .map_err(|_| "branding lock poisoned")?;
        options["branded"] = json!(
            bots.contains(&bot.to_lowercase())
                && options["logo_path"]
                    .as_str()
                    .is_some_and(|p| Path::new(p).is_file())
        );
    }
    Ok(options)
}
fn refresh(panel: &Panel) -> Result<Value, String> {
    let mut cat = Catalog::open(&panel.db, &(panel.utc)(), &panel.root)?;
    let mut options = view_options(panel)?;
    if cat.meta("order_seeded")?.as_deref() != Some("1") {
        let sorted = panel_view::similarity(&cat.all(None)?)?;
        let keys = sorted
            .iter()
            .filter_map(|v| v["content_key"].as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        cat.order(&keys)?;
        cat.dispatch(json!({"operation":"set_meta","key":"order_seeded","value":"1"}))?;
    }
    options["items"] = json!(cat.all(None)?);
    options["published"] = cat.dispatch(json!({"operation":"published_keys"}))?;
    options["published_sets"] = cat.dispatch(json!({"operation":"published_sets"}))?;
    panel_view::build(options)
}
fn get_page(panel: &Panel) -> Result<Response, String> {
    let mut guard = panel.view.lock().map_err(|_| "panel lock poisoned")?;
    let stale = match refresh(panel) {
        Ok(fresh) => {
            *guard = fresh;
            String::new()
        }
        Err(_) => "catalog unavailable".into(),
    };
    let previous = panel_save::read_plan(&panel.db.with_file_name("pack_plan.json"))?;
    let items = crate::intent::execute(
        json!({"operation":"overlay","view":guard["view"],"previous":previous}),
    )?;
    let assets = panel.root.join("assets");
    let mut page = std::fs::read_to_string(assets.join("panel.html")).map_err(|e| e.to_string())?;
    let (version, icon) = crate::asset_version::versions(&panel.root).map_err(|e| e.to_string())?;
    let fps = panel
        .options
        .get("preview_fps")
        .and_then(Value::as_u64)
        .unwrap_or(15);
    for (from, to) in [
        ("__ITEMS__", script_json(&items)),
        ("__TOKEN__", panel.token.clone()),
        (
            "__PREVIEW_TIERS__",
            script_json(
                &json!({"full":{"size":104,"fps":fps.min(15)},"compact":{"size":72,"fps":fps.min(10)}}),
            ),
        ),
        ("__PER_SET__", "200".into()),
        ("__HIDDEN__", guard["hidden"].to_string()),
        ("__STALE__", script_json(&json!(stale))),
        ("__ASSET_VER__", version),
        ("__ICON_VER__", icon),
    ] {
        page = page.replace(from, &to)
    }
    Ok(response(200, page, "text/html; charset=utf-8", "no-store"))
}
fn file_response(path: &Path) -> Response {
    let mime = match path.extension().and_then(|s| s.to_str()).unwrap_or("") {
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "webm" => "video/webm",
        "tgs" => "application/gzip",
        "js" => "application/javascript",
        _ => "application/octet-stream",
    };
    match std::fs::read(path) {
        Ok(bytes) => response(200, bytes, mime, "public, max-age=31536000, immutable"),
        Err(_) => response(404, "not found", "text/plain", "no-store"),
    }
}
fn get(panel: &Panel, path: &str) -> Response {
    match path {
        "/api/ping" => answer(200, json!({"ok":true})),
        "/api/session" => answer(200, panel.session.clone()),
        "/" => get_page(panel)
            .unwrap_or_else(|e| response(409, e, "text/plain; charset=utf-8", "no-store")),
        _ if path.starts_with("/index") => get_page(panel)
            .unwrap_or_else(|e| response(409, e, "text/plain; charset=utf-8", "no-store")),
        _ if path.starts_with("/static/") => {
            let root = panel.root.join("assets");
            let target = root.join(&path[8..]).canonicalize();
            match (target, root.canonicalize()) {
                (Ok(p), Ok(r)) if p.starts_with(&r) && p.is_file() => file_response(&p),
                _ => response(404, "not found", "text/plain", "no-store"),
            }
        }
        _ if path.starts_with("/img/") => {
            let guard = match panel.view.lock() {
                Ok(g) => g,
                Err(_) => return answer(503, json!({"error":"panel unavailable"})),
            };
            match guard["by_key"].get(&path[5..]).and_then(Value::as_str) {
                Some(p) => file_response(Path::new(p)),
                None => response(404, "not found", "text/plain", "no-store"),
            }
        }
        _ => response(404, "not found", "text/plain", "no-store"),
    }
}
fn post(panel: &Panel, path: &str, payload: Value) -> Result<Response, String> {
    if path == "/api/client-log" {
        if payload.to_string().len() > 16384 {
            return Ok(answer(413, json!({})));
        }
        let mut events = panel.events.lock().map_err(|_| "event lock poisoned")?;
        return Ok(match events.record(&payload, &panel.log) {
            Ok(204) => response(204, Body::empty(), "application/json", "no-store"),
            Ok(code) => answer(code, json!({})),
            Err(e) => answer(400, json!({"error":e})),
        });
    }
    let mut guard = panel.view.lock().map_err(|_| "panel lock poisoned")?;
    if path == "/api/save" {
        let mut cat = Catalog::open(&panel.db, &(panel.utc)(), &panel.root)?;
        let out = panel_save::save(
            &mut cat,
            &payload,
            &view_options(panel)?,
            &panel.db.with_file_name("pack_plan.json"),
            &(panel.utc)(),
        )?;
        if out["status"] == 200 {
            *guard = out["view"].clone();
        }
        return Ok(answer(
            out["status"].as_u64().unwrap_or(500) as u16,
            out["body"].clone(),
        ));
    }
    if path == "/api/order" {
        let object = payload.as_object().ok_or("expected object")?;
        if object.keys().any(|k| k != "order") {
            return Ok(answer(400, json!({"error":"unknown keys"})));
        }
        let Some(raw) = payload.get("order").and_then(Value::as_array) else {
            return Ok(answer(400, json!({"error":"order must be a list of keys"})));
        };
        let Some(mut keys) = raw
            .iter()
            .map(|v| v.as_str().map(str::to_owned))
            .collect::<Option<Vec<_>>>()
        else {
            return Ok(answer(400, json!({"error":"order must be a list of keys"})));
        };
        keys.retain(|k| !k.starts_with("__"));
        let view = guard["view"].as_array().ok_or("invalid view")?;
        let mut expected = view
            .iter()
            .filter(|v| v.get("isLogo") != Some(&Value::Bool(true)))
            .filter_map(|v| v["key"].as_str().map(str::to_owned))
            .collect::<Vec<_>>();
        let mut submitted = keys.clone();
        submitted.sort();
        expected.sort();
        if submitted != expected {
            return Ok(answer(
                400,
                json!({"error":"order must be a permutation of current keys"}),
            ));
        }
        let mut cat = Catalog::open(&panel.db, &(panel.utc)(), &panel.root)?;
        let current = cat
            .all(None)?
            .iter()
            .filter_map(|v| v["content_key"].as_str().map(str::to_owned))
            .collect::<std::collections::BTreeSet<_>>();
        if keys.iter().any(|k| !current.contains(k)) {
            return Ok(answer(
                409,
                json!({"error":"catalog identities changed; export the draft and reload"}),
            ));
        }
        cat.order(&keys)?;
        cat.dispatch(json!({"operation":"set_meta","key":"order_seeded","value":"1"}))?;
        drop(cat);
        let positions = keys
            .iter()
            .enumerate()
            .map(|(i, k)| (k.as_str(), i))
            .collect::<std::collections::BTreeMap<_, _>>();
        guard["view"]
            .as_array_mut()
            .ok_or("invalid view")?
            .sort_by_key(|v| {
                (
                    v.get("isLogo") != Some(&Value::Bool(true)),
                    positions
                        .get(v["key"].as_str().unwrap_or(""))
                        .copied()
                        .unwrap_or(1 << 30),
                )
            });
        return Ok(answer(200, json!({"ok":true,"count":keys.len()})));
    }
    Ok(answer(404, json!({})))
}
async fn route(State(panel): State<Arc<Panel>>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    if !loopback(header(&parts.headers, "host")) {
        return response(403, "unexpected Host", "text/plain", "no-store");
    }
    let path = percent_encoding::percent_decode_str(parts.uri.path())
        .decode_utf8_lossy()
        .into_owned();
    if parts.method == Method::GET && path.starts_with("/preview/") {
        let query = parts.uri.query().unwrap_or("");
        let pairs = query
            .split('&')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        if pairs.len() > 4 {
            return response(400, "invalid preview parameters", "text/plain", "no-store");
        }
        let maximum = panel.options["preview_fps"].as_u64().unwrap_or(15);
        let mut fps = maximum;
        let mut size = 104;
        let mut still = false;
        for pair in pairs {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            match k {
                "fps" => match v.parse::<u64>() {
                    Ok(n) => fps = n,
                    Err(_) => {
                        return response(
                            400,
                            "invalid preview parameters",
                            "text/plain",
                            "no-store",
                        );
                    }
                },
                "size" => match v.parse::<u64>() {
                    Ok(n) => size = n,
                    Err(_) => {
                        return response(
                            400,
                            "invalid preview parameters",
                            "text/plain",
                            "no-store",
                        );
                    }
                },
                "still" => still = v == "1",
                _ => {}
            }
        }
        if !(1..=30).contains(&fps) || ![52, 72, 104].contains(&size) {
            return response(400, "invalid preview parameters", "text/plain", "no-store");
        }
        let key = &path[9..];
        let source = {
            match panel.view.lock() {
                Ok(guard) => guard["by_key"]
                    .get(key)
                    .and_then(Value::as_str)
                    .map(PathBuf::from),
                Err(_) => None,
            }
        };
        let Some(source) = source else {
            return response(404, "not found", "text/plain", "no-store");
        };
        return match panel
            .previews
            .bytes(key, &source, fps.min(maximum), still, size)
            .await
        {
            Ok(bytes) => response(
                200,
                bytes,
                "image/webp",
                "public, max-age=31536000, immutable",
            ),
            Err(e) => {
                panel.log.event("WARNING", "preview", &e);
                response(404, "no preview", "text/plain", "no-store")
            }
        };
    }
    if parts.method == Method::GET {
        return tokio::task::spawn_blocking(move || get(&panel, &path))
            .await
            .unwrap_or_else(|_| answer(500, json!({"error":"request failed"})));
    }
    if parts.method != Method::POST {
        return answer(405, json!({"error":"method not allowed"}));
    }
    let length = match header(&parts.headers, "content-length").parse::<usize>() {
        Ok(n) => n,
        Err(_) => return answer(411, json!({"error":"Content-Length required"})),
    };
    if length > 4 * 1024 * 1024 {
        return answer(413, json!({"error":"body too large"}));
    }
    let bytes = match tokio::time::timeout(Duration::from_secs(30), to_bytes(body, 4 * 1024 * 1024))
        .await
    {
        Ok(Ok(b)) => b,
        Ok(Err(_)) => return answer(413, json!({"error":"body too large"})),
        Err(_) => return answer(408, json!({"error":"request body timeout"})),
    };
    if !same_token(header(&parts.headers, "x-panel-token"), &panel.token) {
        return answer(403, json!({"error":"bad or missing panel token"}));
    }
    if parts.headers.contains_key("origin") && !loopback(header(&parts.headers, "origin")) {
        return answer(403, json!({"error":"unexpected Origin"}));
    }
    if header(&parts.headers, "content-type")
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        != "application/json"
    {
        return answer(
            403,
            json!({"error":"Content-Type must be application/json"}),
        );
    }
    let payload =
        match serde_json::from_slice::<Value>(if bytes.is_empty() { b"{}" } else { &bytes }) {
            Ok(v) if v.is_object() => v,
            Ok(_) => return answer(400, json!({"error":"expected a JSON object"})),
            Err(_) => return answer(400, json!({"error":"malformed JSON"})),
        };
    tokio::task::spawn_blocking(move || {
        post(&panel, &path, payload)
            .unwrap_or_else(|e| answer(503, json!({"error":format!("catalog unavailable: {e}")})))
    })
    .await
    .unwrap_or_else(|_| answer(500, json!({"error":"request failed"})))
}
pub fn router(panel: Arc<Panel>) -> Router {
    Router::new().fallback(route).with_state(panel)
}

pub async fn serve(mut panel: Panel, port: u16) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    panel.options["open_port"] = json!(port);
    serve_listener(panel, listener, async {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {},
            _ = crate::panel_session::sandbox_parent_ended() => {},
        }
    })
    .await
}

pub async fn serve_listener(
    panel: Panel,
    listener: tokio::net::TcpListener,
    shutdown: impl std::future::Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let panel = Arc::new(panel);
    let warm_panel = panel.clone();
    let mut warming = tokio::task::JoinSet::new();
    if panel.options["no_open"] != true
        && let Some(port) = panel.options["open_port"]
            .as_u64()
            .and_then(|n| u16::try_from(n).ok())
    {
        let opening = panel.clone();
        warming.spawn(async move {
            if let Err(error) = crate::browser_open::open(port, false).await {
                opening.log.event("WARNING", "panel", &error);
            }
        });
    }
    if let Some((token, base, _)) = panel.branding.clone() {
        let lookup = panel.clone();
        warming.spawn(async move {
            let bot = crate::panel_session::detect_bot(&token, &base).await;
            if let Some(bot) = bot
                && let Ok(mut name) = lookup.detected_bot.lock()
            {
                *name = bot;
            }
        });
    }
    warming.spawn(async move {
        let view = tokio::task::spawn_blocking({
            let panel = warm_panel.clone();
            move || {
                let mut guard = panel.view.lock().map_err(|_| "panel lock poisoned")?;
                let view = refresh(&panel)?;
                *guard = view.clone();
                Ok::<Value, String>(view)
            }
        })
        .await;
        match view {
            Ok(Ok(view)) => {
                let started = std::time::Instant::now();
                let fps = warm_panel.options["preview_fps"].as_u64().unwrap_or(15);
                let counts = warm_panel.previews.warm(&view, fps).await;
                warm_panel.log.event(
                    "INFO",
                    "preview",
                    &format!(
                        "warm-up rendered={} cached={} failed={} duration_ms={}",
                        counts["rendered"],
                        counts["cached"],
                        counts["failed"],
                        started.elapsed().as_millis()
                    ),
                );
            }
            _ => warm_panel.log.event(
                "WARNING",
                "preview",
                "warm-up catalog unavailable; requests will retry",
            ),
        }
    });
    let result = axum::serve(listener, router(panel))
        .with_graceful_shutdown(shutdown)
        .await;
    warming.shutdown().await;
    result
}
