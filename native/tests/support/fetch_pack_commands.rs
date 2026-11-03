//! Original whole-pack CLI refusals and shared repaint defaults use the same artifact and owner.
use _native::{catalog::Catalog, logging::RunLog, tooling_process};
use serde_json::json;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::{Duration, Instant};

#[allow(dead_code)]
#[path = "bot_api.rs"]
mod bot_api;

pub fn id_arguments(
    root: &Path,
    executable: &Path,
    runtime: &tokio::runtime::Runtime,
    environment: &BTreeMap<String, String>,
    log: &RunLog,
    deadline: Instant,
) {
    let known = [
        ("5000000000000000001", "alpha_by_bot"),
        ("5000000000000000002", "alpha_by_bot"),
        ("5000000000000000003", "beta_by_bot"),
    ];
    let source = root.join("source.png");
    let before = std::fs::read(&source).unwrap();
    for (name, args, ids, packs, missing) in [
        (
            "same-pack",
            vec![known[0].0, "premium-id:5000000000000000002"],
            vec![known[0].0, known[1].0],
            vec!["alpha_by_bot"],
            false,
        ),
        (
            "mixed",
            vec!["https://t.me/addemoji/beta_by_bot", known[2].0, known[0].0],
            vec![known[2].0, known[0].0],
            vec!["beta_by_bot", "alpha_by_bot"],
            false,
        ),
        (
            "unknown",
            vec![known[0].0, "5999999999999999999"],
            vec![known[0].0, "5999999999999999999"],
            vec!["alpha_by_bot"],
            true,
        ),
    ] {
        let data = root.join(format!("pack IDs {name}"));
        let mut catalog = Catalog::open(&data.join("catalog.db"), "fixture UTC", root).unwrap();
        assert!(
            catalog
                .insert(
                    &json!({"content_key":"s:resolver-fixture", "fmt":"static",
            "file_path":source,"emojis":[],"keywords":[],"sources":[]}),
                    "fixture UTC"
                )
                .unwrap()
        );
        for pack in &packs {
            catalog
                .record_seen(&format!("FU-{pack}"), "s:resolver-fixture")
                .unwrap();
        }
        drop(catalog);
        let resolved = ids
            .iter()
            .filter_map(|id| {
                known
                    .iter()
                    .find(|(known, _)| known == id)
                    .map(|(_, pack)| json!({"custom_emoji_id":id,"set_name":pack}))
            })
            .collect::<Vec<_>>();
        let mut replies = vec![bot_api::Reply::new(
            "getCustomEmojiStickers",
            &[("custom_emoji_ids", &json!(ids).to_string())],
            json!({"ok":true,"result":resolved}),
        )];
        for pack in &packs {
            replies.push(bot_api::Reply::new(
                "getStickerSet",
                &[("name", pack)],
                json!({"ok":true,"result":{"name":pack,"stickers":[{
                    "file_id":"must-not-download","file_unique_id":format!("FU-{pack}"),"emoji":"😀"
                }]}}),
            ));
        }
        let mut server = bot_api::Server::new(replies);
        let mut environment = environment.clone();
        environment.insert("GENERAL_BOT_TOKEN".into(), bot_api::TOKEN.into());
        environment.insert("NUMERA_TEST_API_BASE".into(), server.base.clone());
        let mut argv = vec![executable.as_os_str().to_owned(), "fetch-pack".into()];
        argv.extend(args.into_iter().map(std::ffi::OsString::from));
        argv.extend(["--data-dir".into(), data.as_os_str().to_owned()]);
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(!remaining.is_zero(), "pack ID command deadline exhausted");
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let completed = runtime
                .block_on(tooling_process::capture(
                    root,
                    &argv,
                    &environment,
                    remaining.min(Duration::from_secs(10)),
                    remaining.min(Duration::from_secs(5)),
                    log,
                ))
                .unwrap();
            assert_eq!(
                completed.code,
                if missing { 3 } else { 0 },
                "{name}: {} {}",
                completed.stdout,
                completed.stderr
            );
            let extra = if missing { " packs_failed=1" } else { "" };
            assert_eq!(
                completed.stdout,
                format!(
                    "Done. new=0 dedup={} failed=0{extra}\n  catalog static: 1 total (1 pending upload)\n",
                    packs.len()
                )
            );
            assert!(!completed.stderr.contains(bot_api::TOKEN));
            let catalog = Catalog::open(&data.join("catalog.db"), "observe", root).unwrap();
            let rows = catalog.all(None).unwrap();
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0]["sources"], json!(packs));
            assert_eq!(std::fs::read(&source).unwrap(), before);
            assert!(
                !data.join("tmp").exists(),
                "known IDs must not download or run a codec"
            );
            assert!(!_native::process_owner::cleanup_pending(&data));
            if missing {
                let message = "emoji id 5999999999999999999 belongs to no pack Telegram knows (or is not an id)";
                assert!(
                    std::fs::read_dir(root.join("logs"))
                        .unwrap()
                        .map(|entry| entry.unwrap().path())
                        .filter(|path| path
                            .file_name()
                            .unwrap()
                            .to_string_lossy()
                            .starts_with("fetch-pack_"))
                        .any(|path| std::fs::read_to_string(path)
                            .unwrap()
                            .lines()
                            .any(|line| line.contains("[ERROR] [collector]")
                                && line.contains(message))),
                    "unknown ID must be named in the command ERROR file log"
                );
            }
        }));
        let requests = server.finish();
        if let Err(error) = result {
            std::panic::resume_unwind(error);
        }
        let requests = requests.unwrap();
        assert_eq!(requests.len(), packs.len() + 1);
        assert_eq!(requests[0].method, "getCustomEmojiStickers");
        for (request, pack) in requests.iter().skip(1).zip(&packs) {
            assert_eq!(request.method, "getStickerSet");
            assert_eq!(
                request.form,
                BTreeMap::from([("name".into(), (*pack).into())])
            );
            assert!(request.files.is_empty());
        }
        assert!(
            std::fs::read_dir(root.join("logs/tmp/tooling"))
                .unwrap()
                .next()
                .is_none()
        );
    }
}

pub fn defaults(
    root: &Path,
    executable: &Path,
    runtime: &tokio::runtime::Runtime,
    environment: &BTreeMap<String, String>,
    log: &RunLog,
    deadline: Instant,
) {
    for command in ["fetch-pack", "fetch-emoji-ids"] {
        let data = root.join(format!("default repaint {command}"));
        let id = "111111111111111";
        let sticker = json!({"custom_emoji_id":id,"file_id":"must-not-download",
            "file_unique_id":"skipped-fuid","emoji":"😀","needs_repainting":true});
        let replies = if command == "fetch-pack" {
            vec![bot_api::Reply::new(
                "getStickerSet",
                &[("name", "fixture")],
                json!({"ok":true,"result":{"name":"fixture","stickers":[sticker]}}),
            )]
        } else {
            vec![
                bot_api::Reply::new(
                    "getMe",
                    &[],
                    json!({"ok":true,"result":{"username":"YourEmojiBot"}}),
                ),
                bot_api::Reply::new(
                    "getCustomEmojiStickers",
                    &[("custom_emoji_ids", &json!([id]).to_string())],
                    json!({"ok":true,"result":[sticker]}),
                ),
            ]
        };
        let count = replies.len();
        let mut server = bot_api::Server::new(replies);
        let mut environment = environment.clone();
        environment.insert("GENERAL_BOT_TOKEN".into(), bot_api::TOKEN.into());
        environment.insert("NUMERA_TEST_API_BASE".into(), server.base.clone());
        let mut argv = vec![executable.as_os_str().to_owned(), command.into()];
        if command == "fetch-pack" {
            argv.push("fixture".into());
        } else {
            argv.extend(["--id".into(), id.into()]);
        }
        argv.extend(["--data-dir".into(), data.as_os_str().to_owned()]);
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "default repaint command deadline exhausted"
        );
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let completed = runtime
                .block_on(tooling_process::capture(
                    root,
                    &argv,
                    &environment,
                    remaining.min(Duration::from_secs(10)),
                    remaining.min(Duration::from_secs(5)),
                    log,
                ))
                .unwrap();
            assert_eq!(
                completed.code, 0,
                "{command}: {} {}",
                completed.stdout, completed.stderr
            );
            let expected = if command == "fetch-pack" {
                "Done. new=0 dedup=0 failed=0 repaintable_skipped=1\n"
            } else {
                "Done. unique_ids=1 new=0 dedup=0 failed=0 missing=0 repaintable_skipped=1\n"
            };
            assert_eq!(completed.stdout, expected);
            for message in ["REPAINTABLE", "nobody answered", "--repaintable keep"] {
                assert!(
                    completed.stderr.contains(message),
                    "{command}: {}",
                    completed.stderr
                );
            }
            assert!(!completed.stderr.contains(bot_api::TOKEN));
            assert!(
                !completed.stderr.contains("Ingest them anyway?"),
                "closed stdin must not prompt"
            );
            let catalog = Catalog::open(&data.join("catalog.db"), "observe", root).unwrap();
            assert!(catalog.all(None).unwrap().is_empty());
            assert!(
                !data.join("tmp").exists(),
                "a skipped sticker reached download or codec preparation"
            );
            assert!(!_native::process_owner::cleanup_pending(&data));
        }));
        let requests = server.finish();
        if let Err(error) = result {
            std::panic::resume_unwind(error);
        }
        assert_eq!(requests.unwrap().len(), count);
        assert!(
            std::fs::read_dir(root.join("logs/tmp/tooling"))
                .unwrap()
                .next()
                .is_none()
        );
    }
}

pub fn check(
    root: &Path,
    executable: &Path,
    runtime: &tokio::runtime::Runtime,
    environment: &BTreeMap<String, String>,
    log: &RunLog,
    deadline: Instant,
) {
    for (name, options, packs, code, diagnostic) in [
        (
            "negative-limit",
            vec!["somepack", "--limit", "-1"],
            vec![],
            2,
            "--limit",
        ),
        (
            "threshold-range",
            vec!["somepack", "--phash-threshold", "64"],
            vec![],
            2,
            "out of range",
        ),
        (
            "accepted-threshold",
            vec!["somepack", "--phash-threshold", "4"],
            vec!["somepack"],
            4,
            "",
        ),
        (
            "all-packs-failed",
            vec!["gone_by_bot", "https://t.me/addemoji/also_gone"],
            vec!["gone_by_bot", "also_gone"],
            4,
            "",
        ),
    ] {
        let data = root.join(format!("pack contract {name}"));
        let mut server = bot_api::Server::new(packs.iter().map(|name| {
            bot_api::Reply::new("getStickerSet", &[("name", *name)],
                json!({"ok":false,"error_code":400,"description":"Bad Request: STICKERSET_INVALID"}))
        }).collect());
        let mut environment = environment.clone();
        environment.insert("GENERAL_BOT_TOKEN".into(), bot_api::TOKEN.into());
        environment.insert("NUMERA_TEST_API_BASE".into(), server.base.clone());
        let mut argv = vec![executable.as_os_str().to_owned(), "fetch-pack".into()];
        argv.extend(options.into_iter().map(std::ffi::OsString::from));
        argv.extend(["--data-dir".into(), data.as_os_str().to_owned()]);
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "fetch-pack contract deadline exhausted"
        );
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let completed = runtime
                .block_on(tooling_process::capture(
                    root,
                    &argv,
                    &environment,
                    remaining.min(Duration::from_secs(10)),
                    remaining.min(Duration::from_secs(5)),
                    log,
                ))
                .unwrap();
            assert_eq!(
                completed.code, code,
                "{name}: {} {}",
                completed.stdout, completed.stderr
            );
            assert!(!completed.stderr.contains(bot_api::TOKEN));
            if code == 2 {
                assert!(
                    completed.stderr.contains(diagnostic),
                    "{name}: {}",
                    completed.stderr
                );
                assert!(completed.stdout.is_empty());
                assert!(!data.exists(), "validation created a catalog");
            } else {
                assert_eq!(
                    completed.stdout,
                    format!(
                        "Done. new=0 dedup=0 failed=0 packs_failed={}\n",
                        packs.len()
                    )
                );
                let catalog = Catalog::open(&data.join("catalog.db"), "observe", root).unwrap();
                assert!(catalog.all(None).unwrap().is_empty());
                assert!(!data.join("tmp/native-codec").exists());
            }
            assert!(!_native::process_owner::cleanup_pending(&data));
        }));
        let requests = server.finish();
        if let Err(error) = result {
            std::panic::resume_unwind(error);
        }
        let requests = requests.unwrap();
        assert_eq!(requests.len(), packs.len(), "{name}");
        for (request, pack) in requests.iter().zip(packs) {
            assert_eq!(request.method, "getStickerSet");
            assert_eq!(request.form, BTreeMap::from([("name".into(), pack.into())]));
            assert!(request.files.is_empty());
        }
        assert!(
            std::fs::read_dir(root.join("logs/tmp/tooling"))
                .unwrap()
                .next()
                .is_none()
        );
    }
}
