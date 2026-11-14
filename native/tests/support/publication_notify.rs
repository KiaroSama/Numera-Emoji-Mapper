//! Original independent announcement milestones and Worker routing, without any media execution.
use super::{Commands, UTC};
use crate::bot_api::{Reply, Server, TOKEN};
use _native::{
    atomic, catalog::Catalog, config::Config, media_adapter::MediaAdapter, publication::Publisher,
    telegram::Telegram,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const NAME: &str = "clos1_by_bot";
fn direct(title: &str, success: bool) -> Reply {
    Reply::new(
        "sendMessage",
        &[
            ("chat_id", "111111111"),
            ("text", &format!("✅ {title}\nhttps://t.me/addemoji/{NAME}")),
            ("disable_web_page_preview", "true"),
        ],
        if success {
            json!({"ok":true,"result":{"message_id":1}})
        } else {
            json!({"ok":false,"description":"Bad Request: fixture send refused"})
        },
    )
}
fn worker(body: &Value, success: bool) -> Reply {
    let mut reply = Reply::new("POST /publish", &[], json!({"ok":success}));
    reply.fields.insert("json".into(), body.to_string());
    reply.status = if success {
        axum::http::StatusCode::OK
    } else {
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    };
    reply
}

pub(super) fn run(commands: &Commands<'_>) {
    for variant in [
        "milestones",
        "failed-direct",
        "worker",
        "failed-worker",
        "half-worker",
    ] {
        let data = commands.root.join(format!("publication-notify-{variant}"));
        let path = data.join("publish_clos.json");
        let mut state =
            json!({"base":"clos","sets":[],"sent":[],"sent_full":[],"skipped":[],"in_flight":null});
        atomic::write_json(&path, &state).unwrap();
        let mut replies = if variant == "milestones" {
            state["sent"] = json!([NAME]);
            atomic::write_json(&path, &state).unwrap();
            vec![direct("Recorded title", true)]
        } else if variant == "failed-direct" {
            vec![
                direct("Recorded title", false),
                direct("Recorded title", true),
                direct("Recorded title", true),
            ]
        } else if variant == "half-worker" {
            vec![direct("Recorded title", true)]
        } else {
            let body = json!({"bot":"general","packs":[{"name":NAME,"title":"Recorded title"}],"style":"cards"});
            if variant == "failed-worker" {
                vec![worker(&body, false), worker(&body, true)]
            } else {
                vec![worker(&body, true)]
            }
        };
        for reply in &mut replies {
            reply.observation = Some((
                path.clone(),
                if variant == "milestones" || variant == "failed-direct" {
                    "/sent_full"
                } else {
                    "/sent"
                }
                .into(),
                json!([]),
            ));
        }
        // The third request is the separate initial milestone, after the full one succeeded.
        if variant == "failed-direct" {
            replies[2].observation = Some((path.clone(), "/sent".into(), json!([])));
        }
        let expected = replies
            .iter()
            .map(|r| (r.method.clone(), r.fields.clone()))
            .collect::<Vec<_>>();
        let mut server = Server::new(replies);
        let mut environment =
            BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]);
        if variant.contains("worker") {
            environment.insert(
                "WORKER_PUBLISH_URL".into(),
                format!("{}/publish", server.base),
            );
            environment.insert(
                "WORKER_PUBLISH_SECRET".into(),
                if variant == "half-worker" {
                    ""
                } else {
                    "test-only-worker-secret"
                }
                .into(),
            );
        }
        let config = Config::load_with_environment(commands.root.into(), &environment).unwrap();
        let tg = Telegram::new(TOKEN.into(), server.base.clone()).unwrap();
        let media = MediaAdapter {
            python: commands.root.join("must-not-execute"),
            root: commands.root.into(),
            scratch: data.join("unused-codec"),
        };
        let mut cat = Catalog::open(&data.join("catalog.db"), UTC, commands.root).unwrap();
        {
            let publisher = Publisher {
                tg: &tg,
                cat: &mut cat,
                media: &media,
                data: data.clone(),
                runtime: commands.runtime,
                config: Some(&config),
            };
            let set =
                json!({"name":NAME,"title":"Recorded title","fmt":"mixed","keys":[],"live":200});
            println!("native collection notification seam: {variant}");
            {
                let mut notify = |full| {
                    _native::collection_notify::notify(
                        &publisher, 111111111, &mut state, &path, &set, full,
                    )
                    .unwrap()
                };
                if variant == "milestones" {
                    notify(true);
                    notify(true);
                    notify(false);
                } else if variant == "failed-direct" {
                    notify(true);
                    let after: Value =
                        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
                    assert_eq!(after["sent_full"], json!([]));
                    notify(true);
                    notify(true);
                    notify(false);
                    notify(false);
                } else {
                    notify(false);
                    if variant == "failed-worker" {
                        let after: Value =
                            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
                        assert_eq!(after["sent"], json!([]));
                        notify(false);
                    }
                    notify(false);
                }
            }
        }
        let saved: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved, state);
        assert_eq!(state["sent"], json!([NAME]));
        assert_eq!(
            state["sent_full"],
            if variant == "milestones" || variant == "failed-direct" {
                json!([NAME])
            } else {
                json!([])
            }
        );
        assert!(cat.all(None).unwrap().is_empty());
        drop(cat);
        let requests = server.finish().unwrap();
        assert_eq!(requests.len(), expected.len());
        for (request, (method, fields)) in requests.iter().zip(expected) {
            assert_eq!(request.method, method);
            assert_eq!(request.form, fields);
        }
        assert!(!data.join("unused-codec").exists());
        assert!(!_native::process_owner::cleanup_pending(commands.root));
    }
    family(commands);
}

fn family(commands: &Commands<'_>) {
    let packs = (0..30)
        .map(|i| json!({"name":format!("p{i}_by_bot"),"title":i.to_string()}))
        .collect::<Vec<_>>();
    let body = json!({"bot":"coin","packs":packs,"style":"cards","note":"all packs:"});
    let mut server = Server::new(vec![worker(&body, true)]);
    let config = Config::load_with_environment(
        commands.root.into(),
        &BTreeMap::from([
            ("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into()),
            (
                "WORKER_PUBLISH_URL".into(),
                format!("{}/publish", server.base),
            ),
            (
                "WORKER_PUBLISH_SECRET".into(),
                "test-only-worker-secret".into(),
            ),
        ]),
    )
    .unwrap();
    let tg = Telegram::new(TOKEN.into(), server.base.clone()).unwrap();
    assert_eq!(
        _native::announce::packs(
            &config,
            &tg,
            111111111,
            &packs,
            "coin",
            "all packs:",
            "cards"
        )
        .unwrap(),
        "the worker"
    );
    let requests = server.finish().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "POST /publish");
    assert_eq!(
        serde_json::from_str::<Value>(&requests[0].form["json"]).unwrap(),
        body
    );

    let mut server = Server::new(vec![
        Reply::new(
            "sendMessage",
            &[
                ("chat_id", "111111111"),
                ("text", "header"),
                ("disable_web_page_preview", "true"),
            ],
            json!({"ok":true,"result":true}),
        ),
        direct("A", true),
    ]);
    let config = Config::load_with_environment(
        commands.root.into(),
        &BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]),
    )
    .unwrap();
    let tg = Telegram::new(TOKEN.into(), server.base.clone()).unwrap();
    assert_eq!(
        _native::announce::packs(
            &config,
            &tg,
            111111111,
            &[json!({"name":NAME,"title":"A"})],
            "coin",
            "header",
            "cards"
        )
        .unwrap(),
        "111111111"
    );
    let requests = server.finish().unwrap();
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].form["text"], "header");
    assert_eq!(
        requests[1].form["text"],
        format!("✅ A\nhttps://t.me/addemoji/{NAME}")
    );
    assert!(
        requests
            .iter()
            .all(|r| r.form["disable_web_page_preview"] == "true")
    );
}
