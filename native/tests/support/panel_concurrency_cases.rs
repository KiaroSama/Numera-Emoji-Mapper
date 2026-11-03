//! Requests acknowledged while the real view mutex is held must preserve curation after release.
use super::panel_persistence_cases::{call, excluded, key, page};
use _native::{catalog::Catalog, panel_http::Panel};
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use serde_json::json;
use std::collections::BTreeSet;
use std::path::Path;
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

pub type Arrivals = Arc<Mutex<Option<mpsc::Sender<String>>>>;

pub async fn acknowledge(
    State(arrivals): State<Arrivals>,
    request: Request,
    next: Next,
) -> Response {
    if let Some(sender) = arrivals.lock().expect("arrival lock poisoned").as_ref() {
        sender
            .send(request.uri().path().to_owned())
            .expect("request observer disappeared");
    }
    next.run(request).await
}

pub fn check(
    client: &reqwest::blocking::Client,
    url: &str,
    db: &Path,
    root: &Path,
    panel: &Arc<Panel>,
    arrivals: &Arrivals,
    deadline: Instant,
) {
    let keys = (0..5).map(key).collect::<Vec<_>>();
    let reversed = keys.iter().rev().cloned().collect::<Vec<_>>();
    let _ = page(client, url, deadline);
    let before = Catalog::open(db, "fixture", root)
        .unwrap()
        .all(None)
        .unwrap();
    let shared = before[..3]
        .iter()
        .map(|row| {
            (
                row["content_key"].as_str().unwrap().to_owned(),
                std::fs::read(row["file_path"].as_str().unwrap()).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    let (sender, receiver) = mpsc::channel();
    let (order, save, document) = std::thread::scope(|scope| {
        let guard = panel.view.lock().unwrap();
        *arrivals.lock().unwrap() = Some(sender);
        let order = scope.spawn(|| {
            call(
                client,
                url,
                "api/order",
                Some(&json!({"order":reversed})),
                deadline,
            )
        });
        let save = scope.spawn(|| {
            call(
                client,
                url,
                "api/save",
                Some(&json!({"known":keys,"excluded":&keys[..2]})),
                deadline,
            )
        });
        let document = scope.spawn(|| page(client, url, deadline));
        let media = scope.spawn(|| {
            for (key, expected) in &shared {
                assert!(Instant::now() < deadline);
                let response = client.get(format!("{url}img/{key}")).send().unwrap();
                assert_eq!(
                    response.status(),
                    200,
                    "shared media key disappeared: {key}"
                );
                assert_eq!(response.bytes().unwrap().as_ref(), expected.as_slice());
            }
        });
        let acknowledged = (0..4)
            .map(|_| {
                receiver.recv_timeout(
                    deadline
                        .saturating_duration_since(Instant::now())
                        .min(Duration::from_secs(2)),
                )
            })
            .collect::<Result<BTreeSet<_>, _>>();
        // Release before asserting or joining, including a missing-arrival failure.
        *arrivals.lock().unwrap() = None;
        drop(guard);
        let answers = (order.join(), save.join(), document.join(), media.join());
        assert_eq!(
            acknowledged.unwrap(),
            BTreeSet::from([
                "/api/order".into(),
                "/api/save".into(),
                "/".into(),
                format!("/img/{}", shared[0].0),
            ])
        );
        answers.3.unwrap();
        (answers.0.unwrap(), answers.1.unwrap(), answers.2.unwrap())
    });
    assert_eq!(order.0, 200, "{}", order.1);
    assert_eq!(save.0, 200, "{}", save.1);
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&save.1).unwrap()["excluded"],
        2
    );
    assert_eq!(excluded(db), keys[..2].iter().cloned().collect());
    assert_eq!(
        document
            .iter()
            .map(|card| card["key"].as_str().unwrap().to_owned())
            .collect::<BTreeSet<_>>(),
        keys.iter().cloned().collect()
    );
    let mut catalog = Catalog::open(db, "fixture", root).unwrap();
    let actual = catalog.all(None).unwrap();
    assert_eq!(
        actual
            .iter()
            .map(|row| row["content_key"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>(),
        reversed
    );
    for row in &actual {
        let original = before
            .iter()
            .find(|item| item["content_key"] == row["content_key"])
            .unwrap();
        for field in [
            "fmt",
            "file_path",
            "emojis",
            "keywords",
            "sources",
            "phash",
            "custom_emoji_id",
            "uploaded",
        ] {
            assert_eq!(row[field], original[field], "{field}");
        }
    }
    for row in actual.iter().filter(|row| {
        !shared
            .iter()
            .any(|(key, _)| row["content_key"] == key.as_str())
    }) {
        let response = client
            .get(format!("{url}img/{}", row["content_key"].as_str().unwrap()))
            .send()
            .unwrap();
        assert_eq!(response.status(), 200);
        assert_eq!(
            response.bytes().unwrap().as_ref(),
            std::fs::read(row["file_path"].as_str().unwrap()).unwrap()
        );
    }
    catalog.inclusion(&[]).unwrap();
    catalog.order(&keys).unwrap();
    drop(catalog);
    let _ = page(client, url, deadline);
    assert!(Instant::now() < deadline);
}
