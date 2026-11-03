//! Original verified add retry with actual downloaded content rather than injected match answers.
use super::{Commands, FIRST, download, listing, member};
use crate::bot_api::{Reply, Server, TOKEN};
use crate::bounded_media::BoundedCodec;
use _native::{
    media_native_adapter::NativeMediaAdapter,
    telegram::{ApiError, Telegram},
    upload_effect::UploadEffect,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn attempt(bytes: &[u8], answer: Value) -> Reply {
    let mut reply = Reply::new("addStickerToSet", &[], answer);
    reply.fields = BTreeMap::from([
        ("user_id".into(),"111111111".into()), ("name".into(),FIRST.into()),
        ("sticker".into(),json!({"sticker":"attach://file0","format":"static","emoji_list":["😀"],"keywords":["kw"]}).to_string()),
    ]);
    reply.files.insert("file0".into(), bytes.to_vec());
    reply
}

pub(super) fn run(commands: &Commands<'_>, images: &[PathBuf; 3], bytes: &[Vec<u8>; 3]) {
    for variant in ["applied", "foreign", "unreadable", "not-applied", "unknown"] {
        let data = commands
            .root
            .join(format!("publication-real-retry-{variant}"));
        std::fs::create_dir(&data).unwrap();
        let old = member("retry-seed", "retry-seed-unique", "1");
        let ours = member("retry-ours", "retry-ours-unique", "2");
        let foreign = member("retry-foreign", "retry-foreign-unique", "3");
        let mut replies = vec![
            listing(FIRST, std::slice::from_ref(&old)),
            attempt(&bytes[0], json!([])),
        ];
        let live = if variant == "foreign" {
            vec![old.clone(), foreign.clone()]
        } else if variant == "not-applied" {
            vec![old.clone()]
        } else {
            vec![old.clone(), ours.clone()]
        };
        let probe = if variant == "unknown" {
            Reply::new("getStickerSet", &[("name", FIRST)], json!({"ok":true}))
        } else {
            listing(FIRST, &live)
        };
        let (probe, mut reached, release) = probe.hold();
        replies.push(probe);
        if variant == "foreign" {
            download(&mut replies, "retry-foreign", &bytes[2]);
        } else if variant == "applied" {
            download(&mut replies, "retry-ours", &bytes[0]);
        } else if variant == "unreadable" {
            download(&mut replies, "retry-ours", b"not an image");
        }
        let retry = variant == "foreign" || variant == "not-applied";
        if retry {
            replies.push(attempt(&bytes[0], json!({"ok":true,"result":true})));
        }
        if variant == "applied" || retry {
            let final_live = if variant == "foreign" {
                vec![old.clone(), foreign.clone(), ours.clone()]
            } else {
                vec![old.clone(), ours.clone()]
            };
            replies.push(listing(FIRST, &final_live));
            if variant == "foreign" {
                download(&mut replies, "retry-foreign", &bytes[2]);
            }
            download(&mut replies, "retry-ours", &bytes[0]);
        }
        let expected = replies
            .iter()
            .map(|r| (r.method.clone(), r.fields.clone()))
            .collect::<Vec<_>>();
        let mut server = Server::new(replies);
        let tg = Telegram::new(TOKEN.into(), server.base.clone()).unwrap();
        let media = BoundedCodec {
            native: NativeMediaAdapter {
                executable: commands.executable.into(),
                root: commands.root.into(),
                scratch: data.join("unused-codec"),
            },
            deadline: std::time::Instant::now() + std::time::Duration::from_secs(60),
        };
        let effect = UploadEffect {
            tg: &tg,
            media: &media,
            data: &data,
            runtime: commands.runtime,
        };
        let request = json!({"name":FIRST,"source":images[0],"format":"static","create":false,
            "user_id":111111111,"emojis":["😀"],"keywords":["kw"],"expected_before":1,"needs_repainting":false});
        println!("native verified add real-content retry: {variant}");
        let result = std::thread::scope(|scope| {
            let observer = scope.spawn(|| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                runtime
                    .block_on(async {
                        tokio::time::timeout(std::time::Duration::from_secs(4), &mut reached).await
                    })
                    .unwrap()
                    .unwrap();
                println!("native verified add proof read: {variant}");
                release.send(()).unwrap();
                runtime.shutdown_timeout(std::time::Duration::from_secs(1));
            });
            let result =
                _native::telegram_mutation::mutate(&tg, &request, |_, sticker, source, fmt| {
                    println!("native verified add content comparison: {variant}");
                    effect
                        .same(sticker, std::path::Path::new(source), fmt)
                        .map_err(ApiError::Invalid)
                });
            observer.join().unwrap();
            result
        });
        if variant == "unreadable" || variant == "unknown" {
            assert!(
                matches!(result, Err(ApiError::Ambiguous(_))),
                "{variant}: {result:?}"
            );
        } else {
            assert_eq!(
                result.unwrap(),
                if variant == "foreign" {
                    json!(3)
                } else {
                    json!(2)
                }
            );
        }
        let requests = server.finish().unwrap();
        assert_eq!(requests.len(), expected.len(), "{variant}");
        for (actual, (method, fields)) in requests.iter().zip(expected) {
            assert_eq!(actual.method, method);
            assert_eq!(actual.form, fields);
        }
        assert_eq!(
            requests
                .iter()
                .filter(|r| r.method == "addStickerToSet")
                .count(),
            if retry { 2 } else { 1 }
        );
        assert!(!data.join("unused-codec").exists());
        if data.join("tmp").exists() {
            assert!(
                std::fs::read_dir(data.join("tmp"))
                    .unwrap()
                    .next()
                    .is_none()
            );
        }
        assert!(!_native::process_owner::cleanup_pending(commands.root));
    }
}
