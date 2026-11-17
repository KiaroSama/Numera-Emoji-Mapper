use super::*;
use crate::baseline_packs_support::*;
fn selected(extra: &[&str]) -> (Result<u8, String>, Vec<u64>) {
    let r = Root::new();
    let data = r.0.join("collection");
    std::fs::create_dir_all(&data).unwrap();
    let mut cat = crate::catalog::Catalog::open(&data.join("catalog.db"), UTC, &r.0).unwrap();
    for i in [1, 2, 5] {
        cat.insert(&json!({"content_key":format!("s:{i}"),"fmt":"static","file_path":r.0.join(format!("{i}.png")),"emojis":[],"keywords":[]}),UTC).unwrap();
        cat.uploaded(&json!({"content_key":format!("s:{i}"),"custom_emoji_id":i.to_string(),"base":"pk","set_name":format!("pks{i}_by_bot")}),UTC).unwrap();
    }
    drop(cat);
    r.write("collection/publish_pk.json",json!({"base":"pk","sent":[],"skipped":[],"sets":([1,2,5].iter().map(|i|json!({"index":i,"name":format!("pks{i}_by_bot"),"fmt":"static","title":format!("Pack {i}"),"live":2,"logo":true,"keys":[format!("s:{i}")]})).collect::<Vec<_>>())} ).to_string());
    let bot = http::Bot::new(|req| {
        assert_eq!(req.method, "getStickerSet");
        let n = req.form["name"]
            .strip_prefix("pks")
            .unwrap()
            .split('_')
            .next()
            .unwrap();
        http::ok(json!({"stickers":live(&["logo",n])}))
    });
    let mut args = vec![
        "--base".into(),
        "pk".into(),
        "--data-dir".into(),
        data.to_str().unwrap().into(),
    ];
    args.extend(extra.iter().map(|s| s.to_string()));
    let log = crate::logging::RunLog::open(&r.0, "order-cli", vec![]);
    let result = run_with(
        &r.config(&[("GENERAL_BOT_TOKEN", http::TOKEN)]),
        &args,
        &log,
        |token| {
            assert_eq!(token, http::TOKEN);
            Ok(bot.telegram())
        },
    );
    let seen = bot
        .requests
        .lock()
        .unwrap()
        .iter()
        .map(|req| {
            req.form["name"]
                .strip_prefix("pks")
                .unwrap()
                .split('_')
                .next()
                .unwrap()
                .parse()
                .unwrap()
        })
        .collect();
    (result, seen)
}

// Original: tests.test_sync_order.OnePackAtATime.test_without_the_flag_every_pack_is_visited
#[test]
#[allow(non_snake_case)]
fn sync_order__OnePackAtATime__test_without_the_flag_every_pack_is_visited() {
    let (rc, s) = selected(&[]);
    assert_eq!(rc.unwrap(), 0);
    assert_eq!(s, vec![1, 2, 5]);
}

// Original: tests.test_sync_order.OnePackAtATime.test_pack_limits_the_run_to_that_set
#[test]
#[allow(non_snake_case)]
fn sync_order__OnePackAtATime__test_pack_limits_the_run_to_that_set() {
    let (rc, s) = selected(&["--pack", "2"]);
    assert_eq!(rc.unwrap(), 0);
    assert_eq!(s, vec![2]);
}

// Original: tests.test_sync_order.OnePackAtATime.test_pack_is_repeatable
#[test]
#[allow(non_snake_case)]
fn sync_order__OnePackAtATime__test_pack_is_repeatable() {
    let (rc, s) = selected(&["--pack", "2", "--pack", "5"]);
    assert_eq!(rc.unwrap(), 0);
    assert_eq!(s, vec![2, 5]);
}

// Original: tests.test_sync_order.OnePackAtATime.test_an_unknown_pack_is_a_usage_error_not_a_silent_no_op
#[test]
#[allow(non_snake_case)]
fn sync_order__OnePackAtATime__test_an_unknown_pack_is_a_usage_error_not_a_silent_no_op() {
    let (rc, s) = selected(&["--pack", "9"]);
    assert!(rc.unwrap_err().contains("not recorded"));
    assert!(s.is_empty());
}
