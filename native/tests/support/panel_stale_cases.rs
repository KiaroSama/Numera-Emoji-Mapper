//! Old drafts cannot acknowledge deleted, rekeyed or newly hidden catalog identities.
use super::panel_persistence_cases::{call, excluded, key, page, post, snapshot};
use _native::catalog::Catalog;
use serde_json::json;
use std::path::Path;
use std::time::{Duration, Instant};

pub fn order(
    client: &reqwest::blocking::Client,
    url: &str,
    db: &Path,
    root: &Path,
    deadline: Instant,
) {
    let keys = (0..5).map(key).collect::<Vec<_>>();
    let shown = page(client, url, deadline);
    assert_eq!(
        shown
            .iter()
            .map(|card| card["key"].as_str().unwrap())
            .collect::<Vec<_>>(),
        keys
    );
    {
        let _owner = Catalog::open(db, "fixture rekey", root).unwrap();
        let connection = rusqlite::Connection::open(db).unwrap();
        connection.busy_timeout(Duration::from_secs(1)).unwrap();
        connection
            .execute(
                "UPDATE items SET content_key='s:changed' WHERE content_key=?",
                [key(0)],
            )
            .unwrap();
    }
    let before = snapshot(db, root);
    assert_eq!(
        post(
            client,
            url,
            json!({"known":keys,"excluded":[],"packs":[[key(0),2]]}),
            deadline
        )
        .0,
        409
    );
    assert!(!db.with_file_name("pack_plan.json").exists());
    assert_eq!(
        call(
            client,
            url,
            "api/order",
            Some(&json!({"order":keys})),
            deadline
        )
        .0,
        409
    );
    assert_eq!(snapshot(db, root), before);
    {
        let mut owner = Catalog::open(db, "restore fixture", root).unwrap();
        let connection = rusqlite::Connection::open(db).unwrap();
        connection.busy_timeout(Duration::from_secs(1)).unwrap();
        connection
            .execute(
                "UPDATE items SET content_key=? WHERE content_key='s:changed'",
                [key(0)],
            )
            .unwrap();
        owner.order(&keys).unwrap();
    }
}

pub fn saves(
    client: &reqwest::blocking::Client,
    url: &str,
    db: &Path,
    root: &Path,
    deadline: Instant,
) {
    let path = db.with_file_name("pack_plan.json");
    let keys = (0..5).map(key).collect::<Vec<_>>();
    for (change, refresh) in [
        ("rekey", true),
        ("delete", true),
        ("published", true),
        ("published", false),
    ] {
        let original = Catalog::open(db, "fixture", root)
            .unwrap()
            .get(&key(4))
            .unwrap()
            .unwrap();
        assert_eq!(
            post(
                client,
                url,
                json!({"known":keys,"excluded":[],"packs":[[key(0),2]]}),
                deadline
            )
            .0,
            200
        );
        let old_page = page(client, url, deadline);
        {
            let mut cat = Catalog::open(db, "fixture change", root).unwrap();
            if change == "published" {
                cat.uploaded(&json!({"content_key":key(4),"base":"fixture","set_name":"hidden-finished-pack","custom_emoji_id":"NEW-CID"}), "fixture").unwrap();
            } else {
                let connection = rusqlite::Connection::open(db).unwrap();
                connection.busy_timeout(Duration::from_secs(1)).unwrap();
                let sql = if change == "rekey" {
                    "UPDATE items SET content_key='s:new-identity' WHERE content_key=?"
                } else {
                    "DELETE FROM items WHERE content_key=?"
                };
                connection.execute(sql, [key(4)]).unwrap();
            }
        }
        if refresh {
            assert!(
                page(client, url, deadline)
                    .iter()
                    .all(|card| card["key"] != key(4))
            );
        }
        let before = snapshot(db, root);
        let bytes = std::fs::read(&path).unwrap();
        let known = old_page
            .iter()
            .map(|card| card["key"].clone())
            .collect::<Vec<_>>();
        let (code, body) = post(
            client,
            url,
            json!({"known":known,"excluded":[key(4),key(1)],"packs":[]}),
            deadline,
        );
        assert_eq!(code, 409, "{change} refresh={refresh}: {body}");
        assert!(body["error"].as_str().unwrap().contains("reload"));
        assert_eq!(snapshot(db, root), before);
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        let known = page(client, url, deadline)
            .iter()
            .map(|card| card["key"].clone())
            .collect::<Vec<_>>();
        assert_eq!(
            post(
                client,
                url,
                json!({"known":known,"excluded":[key(1)],"packs":[]}),
                deadline
            )
            .0,
            200
        );
        assert!(excluded(db).contains(&key(1)));
        {
            let mut cat = Catalog::open(db, "restore fixture", root).unwrap();
            let connection = rusqlite::Connection::open(db).unwrap();
            connection.busy_timeout(Duration::from_secs(1)).unwrap();
            connection
                .execute("DELETE FROM publications WHERE content_key=?", [key(4)])
                .unwrap();
            if change == "rekey" {
                connection
                    .execute(
                        "UPDATE items SET content_key=? WHERE content_key='s:new-identity'",
                        [key(4)],
                    )
                    .unwrap();
            } else if change == "delete" {
                cat.insert(&original, "fixture").unwrap();
            }
            connection
                .execute(
                    "UPDATE items SET uploaded=0,custom_emoji_id=NULL WHERE content_key=?",
                    [key(4)],
                )
                .unwrap();
            cat.inclusion(&[]).unwrap();
            cat.order(&keys).unwrap();
        }
    }
}
