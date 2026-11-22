//! Retry proof must preserve every identity observed before an append.
#[allow(dead_code)]
#[path = "baseline_packs_support/http.rs"]
mod http;
#[path = "support/test_admission.rs"]
mod test_admission;

use numera_runtime::telegram::{ApiError, Telegram};
use serde_json::{Value, json};
use std::time::Duration;

fn members(ids: &[&str]) -> Value {
    json!({"stickers":ids.iter().map(|id| json!({
        "file_unique_id":id,"file_id":id,"custom_emoji_id":format!("cid-{id}")
    })).collect::<Vec<_>>()})
}

fn interrupted_add(
    after: &[&str],
    readback: &[&str],
) -> (Result<Value, ApiError>, Vec<String>, Vec<Duration>) {
    interrupted_add_rows(members(after), members(readback))
}

fn interrupted_add_rows(
    after: Value,
    readback: Value,
) -> (Result<Value, ApiError>, Vec<String>, Vec<Duration>) {
    let mut probes = 0;
    let mut appends = 0;
    let bot = http::Bot::new(move |request| match request.method.as_str() {
        "getStickerSet" => {
            probes += 1;
            http::ok(match probes {
                1 => members(&["old-a", "old-b"]),
                2 => after.clone(),
                _ => readback.clone(),
            })
        }
        "addStickerToSet" => {
            appends += 1;
            if appends == 1 {
                // A malformed response leaves the real HTTP mutation outcome unknown.
                http::Response(b"interrupted Bot API response".to_vec())
            } else {
                http::ok(json!(true))
            }
        }
        method => panic!("unexpected fixture method {method}"),
    });
    let scratch = tempfile::tempdir().unwrap();
    let source = scratch.path().join("ours.png");
    image::RgbaImage::from_pixel(8, 8, image::Rgba([200, 30, 30, 255]))
        .save(&source)
        .unwrap();
    let telegram = Telegram::new(http::TOKEN.into(), bot.base.clone()).unwrap();
    let mut waits = Vec::new();
    let result = numera_runtime::telegram_mutation::mutate_with_wait(
        &telegram,
        &json!({"name":"owned_fixture","source":source,"format":"static",
            "create":false,"user_id":1,"emojis":["😀"],"keywords":[],"expected_before":2}),
        |_, sticker, _, _| Ok(Some(sticker["file_unique_id"] == "ours")),
        |delay| waits.push(delay),
    );
    (result, bot.methods(), waits)
}

#[test]
fn deleted_member_does_not_authorize_resending_an_uncertain_append() {
    let (result, methods, waits) = interrupted_add(&["old-a"], &["old-a", "ours"]);
    assert!(matches!(result, Err(ApiError::Ambiguous(_))), "{result:?}");
    assert_eq!(
        methods
            .iter()
            .filter(|method| *method == "addStickerToSet")
            .count(),
        1,
        "a missing old member makes the snapshot inconclusive"
    );
    assert_eq!(waits, [Duration::from_secs(2)]);
}

#[test]
fn same_image_replacement_is_not_proof_of_an_append() {
    let (result, methods, waits) = interrupted_add(&["old-a", "ours"], &["old-a", "ours"]);
    assert!(matches!(result, Err(ApiError::Ambiguous(_))), "{result:?}");
    assert_eq!(
        methods,
        ["getStickerSet", "addStickerToSet", "getStickerSet"]
    );
    assert_eq!(waits, [Duration::from_secs(2)]);
}

#[test]
fn readback_rejects_a_member_removed_after_successful_append_proof() {
    let (result, methods, waits) = interrupted_add(&["old-a", "old-b", "ours"], &["old-a", "ours"]);
    assert!(matches!(result, Err(ApiError::Ambiguous(_))), "{result:?}");
    assert_eq!(
        methods,
        [
            "getStickerSet",
            "addStickerToSet",
            "getStickerSet",
            "getStickerSet"
        ]
    );
    assert_eq!(waits, [Duration::from_secs(2)]);
}

#[test]
fn readback_requires_complete_unique_identities_before_booking_the_count() {
    for sticker in [
        json!({"file_id":"ours"}),
        json!({"file_unique_id":""}),
        json!({"file_unique_id":123}),
        json!({"file_unique_id":"old-a"}),
    ] {
        let mut readback = members(&["old-a", "old-b"]);
        readback["stickers"].as_array_mut().unwrap().push(sticker);
        let (result, methods, waits) =
            interrupted_add_rows(members(&["old-a", "old-b", "ours"]), readback);
        assert!(matches!(result, Err(ApiError::Ambiguous(_))), "{result:?}");
        assert_eq!(
            methods,
            [
                "getStickerSet",
                "addStickerToSet",
                "getStickerSet",
                "getStickerSet"
            ]
        );
        assert_eq!(waits, [Duration::from_secs(2)]);
    }
}

#[test]
fn stable_membership_still_permits_a_verified_append() {
    let (result, methods, waits) =
        interrupted_add(&["old-a", "old-b", "ours"], &["old-a", "old-b", "ours"]);
    assert_eq!(result.unwrap(), json!(3));
    assert_eq!(
        methods
            .iter()
            .filter(|method| *method == "addStickerToSet")
            .count(),
        1
    );
    assert_eq!(waits, [Duration::from_secs(2)]);
}
