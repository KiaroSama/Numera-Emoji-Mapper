//! Original request and fresh-read contracts share the persistence server and real catalog.
use super::panel_persistence_cases::{call, excluded, key, page, post, snapshot};
use _native::catalog::Catalog;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::Path;
use std::time::{Duration, Instant};

fn document(text: &str) -> (&str, Vec<Value>) {
    let marker = "<script id=\"items-data\" type=\"application/json\">";
    let start = text.find(marker).unwrap() + marker.len();
    let end = start + text[start..].find("</script>").unwrap();
    (
        &text[start..end],
        serde_json::from_str(&text[start..end]).unwrap(),
    )
}

pub fn check(
    client: &reqwest::blocking::Client,
    url: &str,
    db: &Path,
    root: &Path,
    deadline: Instant,
) {
    let keys = (0..5).map(key).collect::<Vec<_>>();
    let initial = page(client, url, deadline);
    let plan = db.with_file_name("pack_plan.json");
    assert!(!plan.exists());
    let before = snapshot(db, root);
    for (token, content_type, origin, body, expected) in [
        (None, "application/json", None, "{}", 403),
        (Some("nope"), "application/json", None, "{}", 403),
        (Some("persistence-fixture"), "text/plain", None, "{}", 403),
        (
            Some("persistence-fixture"),
            "application/json",
            Some("http://evil.invalid"),
            "{}",
            403,
        ),
        (
            Some("persistence-fixture"),
            "application/json",
            None,
            "{not json",
            400,
        ),
        (
            Some("persistence-fixture"),
            "application/json",
            None,
            "[]",
            400,
        ),
    ] {
        assert!(Instant::now() < deadline);
        let mut request = client
            .post(format!("{url}api/save"))
            .header("content-type", content_type)
            .body(body);
        if let Some(token) = token {
            request = request.header("x-panel-token", token);
        }
        if let Some(origin) = origin {
            request = request.header("origin", origin);
        }
        let response = request.send().unwrap();
        assert_eq!(response.status().as_u16(), expected);
        let _ = response.bytes().unwrap();
        assert_eq!(snapshot(db, root), before);
        assert!(!plan.exists());
    }
    for order in [
        json!([key(0)]),
        json!([key(0), key(1), key(2), key(3), key(4), "s:bogus"]),
        json!([key(0), key(0), key(2), key(3), key(4)]),
    ] {
        assert_eq!(
            call(
                client,
                url,
                "api/order",
                Some(&json!({"order":order})),
                deadline
            )
            .0,
            400
        );
        assert_eq!(snapshot(db, root), before);
    }
    assert_eq!(
        post(
            client,
            url,
            json!({"known":[],"excluded":[],"wat":1}),
            deadline
        )
        .0,
        400
    );
    assert_eq!(snapshot(db, root), before);

    for host in [
        "127.0.0.1:8765",
        "localhost:8765",
        "127.0.0.1",
        "http://127.0.0.1:8765",
        "[::1]:8765",
    ] {
        assert!(Instant::now() < deadline);
        let response = client
            .get(format!("{url}api/ping"))
            .header("host", host)
            .send()
            .unwrap();
        assert_eq!(response.status().as_u16(), 200, "{host}");
        assert_eq!(response.json::<Value>().unwrap(), json!({"ok":true}));
    }
    for host in [
        "evil.invalid",
        "evil.invalid:8765",
        "http://evil.invalid",
        "127.0.0.1.evil.invalid",
        "",
    ] {
        for route in [String::new(), format!("img/{}", key(0))] {
            assert!(Instant::now() < deadline);
            let response = client
                .get(format!("{url}{route}"))
                .header("host", host)
                .send()
                .unwrap();
            assert_eq!(response.status().as_u16(), 403, "{host} {route}");
            assert!(!response.text().unwrap().contains("persistence-fixture"));
        }
    }
    assert_eq!(snapshot(db, root), before);
    for route in ["", "static/panel-grid.js?v=23a5e175b383"] {
        assert!(Instant::now() < deadline);
        let response = client.get(format!("{url}{route}")).send().unwrap();
        assert_eq!(response.status().as_u16(), 200);
        assert_eq!(response.headers()["x-content-type-options"], "nosniff");
        assert_eq!(response.headers()["referrer-policy"], "no-referrer");
        if route.is_empty() {
            assert_eq!(response.headers()["x-frame-options"], "DENY");
            assert_eq!(
                response.headers()["content-security-policy"],
                "frame-ancestors 'none'"
            );
        } else {
            assert!(!response.headers().contains_key("x-frame-options"));
        }
        let _ = response.bytes().unwrap();
    }
    let (status, served) = call(
        client,
        url,
        "static/panel-save.js?v=23a5e175b383",
        None,
        deadline,
    );
    assert_eq!(status, 200);
    assert!(served.contains("/api/ping") && served.contains("setInterval("));
    assert!(!served.contains("setTimeout(()=>a.classList.remove('show')"));
    let beat = &served[served.find("setInterval(async ()=>{").unwrap()..];
    assert!(
        beat.chars()
            .take(600)
            .collect::<String>()
            .contains("flushOrder(pendingOrder)")
    );
    let api = &served[served.find("async function apiPost(").unwrap()..];
    let api = &api[..api.find("async function flushOrder").unwrap()];
    assert!(api.contains("r.status === 403") && api.contains("const TOKEN ="));
    let unload = &served[served.find("addEventListener('beforeunload'").unwrap()..];
    assert!(
        unload
            .chars()
            .take(200)
            .collect::<String>()
            .contains("if(pendingOrder)")
    );
    assert!(
        call(client, url, "", None, deadline)
            .1
            .contains("id=\"alert\"")
    );
    let (status, text) = call(client, url, "", None, deadline);
    assert_eq!(status, 200);
    assert!(text.contains("const PREVIEW_TIERS = {\"full\":{\"size\":104,\"fps\":15},\"compact\":{\"size\":72,\"fps\":10}};"));
    for query in [
        "fps=1000&size=10000",
        "fps=0",
        "fps=not-a-number",
        "size=1",
        "a=1&b=2&c=3&d=4&e=5",
    ] {
        assert_eq!(
            call(
                client,
                url,
                &format!("preview/missing?{query}"),
                None,
                deadline
            )
            .0,
            400
        );
    }
    assert_eq!(
        call(
            client,
            url,
            "preview/missing?fps=30&size=72",
            None,
            deadline
        )
        .0,
        404
    );
    for index in [0, 1] {
        let (code, bytes) = {
            let response = client
                .get(format!("{url}img/{}", key(index)))
                .send()
                .unwrap();
            assert_eq!(response.headers()["content-type"], "image/png");
            assert!(
                response.headers()["cache-control"]
                    .to_str()
                    .unwrap()
                    .contains("immutable")
            );
            (response.status().as_u16(), response.bytes().unwrap())
        };
        assert_eq!(code, 200);
        assert_eq!(
            bytes.as_ref(),
            std::fs::read(root.join(format!("{index}.png"))).unwrap()
        );
    }

    let reversed = keys.iter().rev().cloned().collect::<Vec<_>>();
    let (status, body) = call(
        client,
        url,
        "api/order",
        Some(&json!({"order":reversed})),
        deadline,
    );
    assert_eq!(status, 200);
    assert_eq!(serde_json::from_str::<Value>(&body).unwrap()["ok"], true);
    let (status, saved) = post(
        client,
        url,
        json!({"known":keys,"excluded":[key(0)]}),
        deadline,
    );
    assert_eq!(status, 200);
    assert_eq!(saved["excluded"], 1);
    assert_eq!(excluded(db), BTreeSet::from([key(0)]));
    {
        let mut cat = Catalog::open(db, "restore fixture", root).unwrap();
        cat.inclusion(&[]).unwrap();
        cat.order(&keys).unwrap();
    }
    assert!(!plan.exists());

    let label = "</script><script>alert(1)</script> a\u{2028}b\u{2029}c";
    let original = initial.iter().find(|item| item["key"] == key(0)).unwrap()["label"].clone();
    let update = |keywords: Value| {
        let _owner = Catalog::open(db, "fixture external writer", root).unwrap();
        let connection = rusqlite::Connection::open(db).unwrap();
        connection.busy_timeout(Duration::from_secs(1)).unwrap();
        assert_eq!(
            connection
                .execute(
                    "UPDATE items SET keywords=? WHERE content_key=?",
                    rusqlite::params![keywords.to_string(), key(0)]
                )
                .unwrap(),
            1
        );
    };
    update(json!([label]));
    let (status, text) = call(client, url, "", None, deadline);
    assert_eq!(status, 200);
    let (raw, cards) = document(&text);
    assert!(!raw.contains('<') && !raw.contains('\u{2028}') && !raw.contains('\u{2029}'));
    assert!(raw.contains("\\u003c"));
    assert_eq!(
        cards.iter().find(|item| item["key"] == key(0)).unwrap()["label"],
        label
    );
    update(json!(["premium-id:9999999999999999999"]));
    let cards = page(client, url, deadline);
    let changed = cards.iter().find(|item| item["key"] == key(0)).unwrap();
    assert_ne!(changed["label"], original);
    assert_eq!(changed["copyId"], "9999999999999999999");
    update(
        before
            .0
            .iter()
            .find(|item| item["content_key"] == key(0))
            .unwrap()["keywords"]
            .clone(),
    );
    assert_eq!(page(client, url, deadline), initial);

    let journal = root.join("identity-migration.journal.json");
    assert!(!journal.exists());
    std::fs::write(&journal, b"{}").unwrap();
    for (route, body) in [
        ("api/save", json!({"known":keys,"excluded":[key(0)]})),
        ("api/order", json!({"order":keys})),
    ] {
        let (status, body) = call(client, url, route, Some(&body), deadline);
        assert_eq!(status, 503, "{body}");
        assert_ne!(serde_json::from_str::<Value>(&body).unwrap()["ok"], true);
        assert_eq!(
            _native::migration_signature::signature(db, &[]).unwrap(),
            before.3
        );
        assert_eq!(std::fs::read(&journal).unwrap(), b"{}");
        assert!(!plan.exists());
    }
    std::fs::remove_file(&journal).unwrap();
    assert_eq!(snapshot(db, root), before);
    assert_eq!(page(client, url, deadline), initial);

    // All fixture connections are closed; never move an active WAL away from its database.
    for suffix in ["-wal", "-shm"] {
        assert!(!db.with_file_name(format!("catalog.db{suffix}")).exists());
    }
    let retained = db.with_file_name("retained-catalog.db");
    std::fs::rename(db, &retained).unwrap();
    std::fs::create_dir(db).unwrap();
    std::fs::write(db.join("foreign"), b"retain fixture obstruction").unwrap();
    for (route, body) in [
        ("api/save", json!({"known":keys,"excluded":[key(0)]})),
        ("api/order", json!({"order":keys})),
    ] {
        let (status, body) = call(client, url, route, Some(&body), deadline);
        assert_eq!(status, 503);
        assert!(
            serde_json::from_str::<Value>(&body).unwrap()["error"]
                .as_str()
                .unwrap()
                .contains("catalog unavailable")
        );
        assert!(!plan.exists());
    }
    let (status, text) = call(client, url, "", None, deadline);
    assert_eq!(status, 200);
    assert_eq!(document(&text).1, initial);
    assert!(text.contains("const STALE = \"catalog unavailable\";"));
    assert_eq!(
        std::fs::read(db.join("foreign")).unwrap(),
        b"retain fixture obstruction"
    );
    std::fs::remove_file(db.join("foreign")).unwrap();
    std::fs::remove_dir(db).unwrap();
    std::fs::rename(retained, db).unwrap();
    assert_eq!(page(client, url, deadline), initial);
    assert_eq!(snapshot(db, root), before);
    {
        let mut cat = Catalog::open(db, "fixture hidden selection", root).unwrap();
        cat.uploaded(
            &json!({"content_key":key(0),"base":"hidden-fixture",
            "set_name":"hidden-finished-pack","custom_emoji_id":"HIDDEN-CID"}),
            "fixture",
        )
        .unwrap();
        cat.inclusion(&[key(0)]).unwrap();
    }
    let filtered = page(client, url, deadline);
    assert_eq!(filtered.len(), 4);
    assert!(filtered.iter().all(|item| item["key"] != key(0)));
    let visible = filtered
        .iter()
        .map(|item| item["key"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        post(
            client,
            url,
            json!({"known":visible,"excluded":[]}),
            deadline
        )
        .0,
        200
    );
    assert_eq!(excluded(db), BTreeSet::from([key(0)]));
    {
        let mut cat = Catalog::open(db, "restore fixture publication", root).unwrap();
        cat.dispatch(json!({"operation":"forget","base":"hidden-fixture"}))
            .unwrap();
        let connection = rusqlite::Connection::open(db).unwrap();
        connection.busy_timeout(Duration::from_secs(1)).unwrap();
        connection
            .execute(
                "UPDATE items SET uploaded=0,custom_emoji_id=NULL WHERE content_key=?",
                [key(0)],
            )
            .unwrap();
        cat.inclusion(&[]).unwrap();
    }
    assert_eq!(page(client, url, deadline), initial);
    assert_eq!(snapshot(db, root), before);
}
