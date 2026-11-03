//! Original telemetry contracts through the shared panel router and actual UTF-8 run log.
use super::panel_persistence_cases::call;
use serde_json::{Value, json};
use std::path::Path;
use std::time::Instant;

fn entries(path: &Path) -> Vec<(String, Value)> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter_map(|line| {
            let (prefix, message) = line.split_once(" [UI] ")?;
            let (body, _) = message.rsplit_once(" run=").unwrap();
            let level = prefix.rsplit_once(" [").unwrap().1.trim_end_matches(']');
            Some((level.into(), serde_json::from_str(body).unwrap()))
        })
        .collect()
}

pub fn check(
    client: &reqwest::blocking::Client,
    url: &str,
    log: &Path,
    assets: &Path,
    deadline: Instant,
) {
    let good = json!({"events":[{"event":"hold","count":4},
        {"event":"error","name":"TypeError","source":"panel-grid.js","line":12}]});
    assert_eq!(
        call(client, url, "api/client-log", Some(&good), deadline),
        (204, String::new())
    );
    assert_eq!(
        entries(log),
        [
            ("INFO".into(), good["events"][0].clone()),
            ("WARNING".into(), good["events"][1].clone()),
        ]
    );
    let before = std::fs::read(log).unwrap();
    for event in [
        json!({"event":"error","message":"private-test-value"}),
        json!({"event":[]}),
        json!({"event":"error","source":[]}),
        json!({"event":"hold","count":true}),
    ] {
        // A good prefix in the same request must not escape whole-batch validation either.
        let body = json!({"events":[{"event":"hold","count":4}, event]});
        assert_eq!(
            call(client, url, "api/client-log", Some(&body), deadline).0,
            400
        );
        assert_eq!(std::fs::read(log).unwrap(), before);
    }
    assert!(Instant::now() < deadline);
    // JSON cannot represent Python float("nan"); this exercises actual wire parse refusal.
    let response = client
        .post(format!("{url}api/client-log"))
        .header("x-panel-token", "persistence-fixture")
        .header("content-type", "application/json")
        .body(r#"{"events":[{"event":"hold","count":NaN}]}"#)
        .send()
        .unwrap();
    assert_eq!(response.status().as_u16(), 400);
    assert!(response.text().unwrap().contains("malformed JSON"));
    assert_eq!(std::fs::read(log).unwrap(), before);
    let body = json!({"events":vec![json!({"event":"hold"}); 33]});
    assert_eq!(
        call(client, url, "api/client-log", Some(&body), deadline).0,
        400
    );
    assert_eq!(std::fs::read(log).unwrap(), before);

    let holding = std::fs::read_to_string(assets.join("panel-holding.js")).unwrap();
    let sources = [
        "panel-grid.js",
        "panel-motion.js",
        "panel-save.js",
        "panel-drag.js",
        "panel-actions.js",
        "panel-holding.js",
        "panel-draft.js",
    ];
    let body = json!({"events":sources.map(|source| {
        assert!(holding.contains(&format!("'{source}'")), "missing script {source}");
        json!({"event":"error","source":source,"name":"Error"})
    })});
    assert_eq!(
        call(client, url, "api/client-log", Some(&body), deadline),
        (204, String::new())
    );
    let recorded = entries(log);
    assert_eq!(recorded.len(), 9);
    for ((level, event), expected) in recorded[2..].iter().zip(body["events"].as_array().unwrap()) {
        assert_eq!(level, "WARNING");
        assert_eq!(event, expected);
    }
    assert!(
        !std::fs::read_to_string(log)
            .unwrap()
            .contains("private-test-value")
    );
}
