//! Direct baseline unit tests for private HTTP values and deterministic lock/sort interleavings.
use super::*;
use crate as runtime;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};
#[allow(dead_code)]
#[path = "mod.rs"]
pub(crate) mod support;
// OriginalID: test_panel.InertItemJson.test_script_close_is_escaped
#[test]
fn panel_script_close_is_escaped() {
    let out = script_json(&json!([{"label":"</script><script>alert(1)</script>"}]));
    assert!(!out.contains("</script>"));
    assert!(!out.contains('<'));
    assert!(out.contains("\\u003c"));
}
// OriginalID: test_panel.InertItemJson.test_js_line_separators_are_escaped
#[test]
fn panel_js_line_separators_are_escaped() {
    let out = script_json(&json!([{"label":"a\u{2028}b\u{2029}c"}]));
    assert!(!out.contains('\u{2028}'));
    assert!(!out.contains('\u{2029}'));
}
// OriginalID: test_panel.InertItemJson.test_roundtrips
#[test]
fn panel_roundtrips() {
    let items = json!([{"key":"s:1","label":"ok <b>","included":true}]);
    assert_eq!(
        serde_json::from_str::<Value>(&script_json(&items)).unwrap(),
        items
    );
}
// OriginalID: test_panel_server.LoopbackCheck.test_accepts_loopback_forms
#[test]
fn server_accepts_loopback_forms() {
    for host in [
        "127.0.0.1:8765",
        "localhost:8765",
        "127.0.0.1",
        "http://127.0.0.1:8765",
        "[::1]:8765",
    ] {
        assert!(loopback(host), "{host}");
    }
}
// OriginalID: test_panel_server.LoopbackCheck.test_rejects_foreign_hosts
#[test]
fn server_rejects_foreign_hosts() {
    for host in [
        "evil.com",
        "evil.com:8765",
        "http://evil.com",
        "127.0.0.1.evil.com",
        "",
    ] {
        assert!(!loopback(host), "{host}");
    }
}
fn brand_view(data: &support::Data, bot: &str, logo: &Path) -> Value {
    let env = BTreeMap::from([
        ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
        ("BRAND_LOGO_BOTS".into(), "YourEmojiBot".into()),
        ("BRAND_LOGO_PATH".into(), logo.to_str().unwrap().into()),
    ]);
    let config = runtime::config::Config::load_with_environment(data.root.clone(), &env).unwrap();
    let mut p = data.panel(support::options());
    p.options["logo_path"] = json!(config.logo_path(false).unwrap());
    p.branding = Some((
        "must-not-contact-api".into(),
        "http://127.0.0.1:9".into(),
        config.logo_bots(false).unwrap(),
    ));
    *p.detected_bot.lock().unwrap() = bot.into();
    let options = view_options(&p).unwrap();
    let mut request = data.request(false, json!({}), false, None);
    request["branded"] = options["branded"].clone();
    request["logo_path"] = options["logo_path"].clone();
    runtime::panel_view::build(request).unwrap()
}
// OriginalID: test_panel_view.BrandLogoPreview.test_logo_shown_first_for_a_listed_bot
#[test]
fn view_logo_shown_first_for_a_listed_bot() {
    let d = support::Data::new(3);
    let logo = d.root.join("logo.png");
    support::png(&logo, [10, 20, 30, 255], 40);
    let v = brand_view(&d, "YourEmojiBot", &logo);
    assert_eq!(v["view"][0]["isLogo"], true);
    assert_eq!(v["view"][0]["key"], "__brand_logo__");
    assert_eq!(v["by_key"]["__brand_logo__"], json!(logo));
    let rows = v["view"].as_array().unwrap();
    assert_eq!(rows.len(), 4);
    assert!(rows[1..].iter().all(|x| x["isLogo"] != true));
}
// OriginalID: test_panel_view.BrandLogoPreview.test_logo_hidden_for_coin_bot
#[test]
fn view_logo_hidden_for_coin_bot() {
    let d = support::Data::new(3);
    let logo = d.root.join("logo.png");
    support::png(&logo, [10, 20, 30, 255], 40);
    let v = brand_view(&d, "YourCoinEmojiBot", &logo);
    let rows = v["view"].as_array().unwrap();
    assert_eq!(rows.len(), 3);
    assert!(rows.iter().all(|x| x["isLogo"] != true));
}
// OriginalID: test_panel_view.BrandLogoPreview.test_logo_hidden_when_file_missing
#[test]
fn view_logo_hidden_when_file_missing() {
    let d = support::Data::new(3);
    let v = brand_view(&d, "YourEmojiBot", &d.root.join("does_not_exist.png"));
    assert_eq!(v["view"].as_array().unwrap().len(), 3);
}
// OriginalID: test_panel_view.BrandLogoPreview.test_logo_hidden_when_bot_unknown
#[test]
fn view_logo_hidden_when_bot_unknown() {
    let d = support::Data::new(3);
    let logo = d.root.join("logo.png");
    support::png(&logo, [10, 20, 30, 255], 40);
    let v = brand_view(&d, "", &logo);
    assert_eq!(v["view"].as_array().unwrap().len(), 3);
}

struct Gate {
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
    attempted: mpsc::Sender<String>,
    once: AtomicBool,
}
static GATES: OnceLock<Mutex<BTreeMap<PathBuf, Arc<Gate>>>> = OnceLock::new();
fn gates() -> &'static Mutex<BTreeMap<PathBuf, Arc<Gate>>> {
    GATES.get_or_init(Default::default)
}
pub(super) fn before_view_lock(panel: &Panel, path: &str) {
    let gate = gates().lock().unwrap().get(&panel.root).cloned();
    if let Some(gate) = gate {
        let _ = gate.attempted.send(path.into());
    }
}
pub(super) fn during_order_sort(panel: &Panel) {
    let gate = gates().lock().unwrap().get(&panel.root).cloned();
    if let Some(gate) = gate
        && !gate.once.swap(true, Ordering::SeqCst)
    {
        let _ = gate.entered.send(());
        gate.release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(30))
            .expect("sort release missing");
    }
}
struct Suspended {
    root: PathBuf,
    release: mpsc::Sender<()>,
    attempts: mpsc::Receiver<String>,
    started: Result<(), mpsc::RecvTimeoutError>,
}
impl Suspended {
    fn start(s: &support::Server) -> (Self, std::thread::JoinHandle<(u16, Value)>) {
        let (entered, seen) = mpsc::channel();
        let (release, proceed) = mpsc::channel();
        let (attempted, attempts) = mpsc::channel();
        gates().lock().unwrap().insert(
            s.data.root.clone(),
            Arc::new(Gate {
                entered,
                release: Mutex::new(proceed),
                attempted,
                once: AtomicBool::new(false),
            }),
        );
        let client = s.client.clone();
        let url = s.url("/api/order");
        let keys = s.data.keys.iter().rev().cloned().collect::<Vec<_>>();
        let order = std::thread::spawn(move || post(&client, &url, json!({"order":keys})));
        let started = seen.recv_timeout(Duration::from_secs(30));
        while attempts.try_recv().is_ok() {}
        (
            Self {
                root: s.data.root.clone(),
                release,
                attempts,
                started,
            },
            order,
        )
    }
    fn finish(&self) {
        let _ = self.release.send(());
        gates().lock().unwrap().remove(&self.root);
    }
}
impl Drop for Suspended {
    fn drop(&mut self) {
        self.finish();
    }
}
fn post(client: &reqwest::blocking::Client, url: &str, body: Value) -> (u16, Value) {
    let r = client
        .post(url)
        .header("X-Panel-Token", support::TOKEN)
        .json(&body)
        .send()
        .unwrap();
    (r.status().as_u16(), r.json().unwrap())
}
// OriginalID: test_panel.SaveDuringReorder.test_exclusions_survive_a_concurrent_reorder
#[test]
fn panel_exclusions_survive_a_concurrent_reorder() {
    let s = support::Server::new(4);
    let (gate, order) = Suspended::start(&s);
    let client = s.client.clone();
    let url = s.url("/api/save");
    let keys = s.data.keys.clone();
    let excluded = keys[..2].to_vec();
    let sent = excluded.clone();
    let save =
        std::thread::spawn(move || post(&client, &url, json!({"excluded":sent,"known":keys})));
    let attempt = gate.attempts.recv_timeout(Duration::from_secs(30));
    gate.finish();
    let order = order.join().unwrap();
    let saved = save.join().unwrap();
    assert!(gate.started.is_ok(), "reorder never reached sort");
    assert_eq!(attempt.unwrap(), "/api/save", "save never reached lock");
    assert_eq!(order.0, 200);
    assert_eq!(saved.0, 200);
    assert_eq!(saved.1["excluded"], excluded.len());
    assert_eq!(s.data.excluded(), excluded.into_iter().collect());
}
// OriginalID: test_panel.SaveDuringReorder.test_the_page_is_not_served_from_an_emptied_view
#[test]
fn panel_the_page_is_not_served_from_an_emptied_view() {
    let s = support::Server::new(4);
    let (gate, order) = Suspended::start(&s);
    let client = s.client.clone();
    let url = s.url("/");
    let getter = std::thread::spawn(move || {
        let r = client.get(url).send().unwrap();
        assert_eq!(r.status(), 200);
        r.text().unwrap()
    });
    let attempt = gate.attempts.recv_timeout(Duration::from_secs(10));
    gate.finish();
    let result = order.join().unwrap();
    let body = getter.join().unwrap();
    assert!(gate.started.is_ok(), "reorder never reached sort");
    assert_eq!(attempt.unwrap(), "GET /", "page never reached lock");
    assert_eq!(result.0, 200);
    for key in &s.data.keys {
        assert!(body.contains(key), "grid served without its items");
    }
}
