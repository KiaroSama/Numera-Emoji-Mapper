//! Original private copy-keyboard cases inside the real production payload module.
use serde_json::Value;
fn covers(ids: &[String]) -> Value {
    let kb = super::keyboard(ids);
    let mut copied = vec![];
    for row in kb["inline_keyboard"].as_array().unwrap() {
        let text = row[0]["copy_text"]["text"].as_str().unwrap();
        assert!(text.chars().count() <= 256, "button text exceeds COPY_MAX");
        copied.extend(text.trim().split('\n').map(str::to_owned));
    }
    assert_eq!(copied, ids, "every id exactly once, in order");
    kb
}
// tests.test_emoji_bot.TestCopyButtonLimit.test_short_ids_fit_one_button
#[test]
fn test_short_ids_fit_one_button() {
    let kb = covers(&vec!["111111111".into(); 3]);
    assert_eq!(kb["inline_keyboard"].as_array().unwrap().len(), 1);
}
// tests.test_emoji_bot.TestCopyButtonLimit.test_nineteen_digit_ids
#[test]
fn test_nineteen_digit_ids() {
    covers(
        &(0..12)
            .map(|i| (10_u64.pow(18) + i).to_string())
            .collect::<Vec<_>>(),
    );
}
// tests.test_emoji_bot.TestCopyButtonLimit.test_max_length_ids_are_split
#[test]
fn test_max_length_ids_are_split() {
    let kb = covers(&vec!["9".repeat(25); 12]);
    assert!(
        kb["inline_keyboard"].as_array().unwrap().len() > 1,
        "must split rather than overflow"
    );
}
// tests.test_emoji_bot.TestCopyButtonLimit.test_many_max_length_ids
#[test]
fn test_many_max_length_ids() {
    covers(&vec!["9".repeat(25); 50]);
}
// tests.test_emoji_bot.TestCopyButtonLimit.test_single_id
#[test]
fn test_single_id() {
    covers(&["111111111".into()]);
}
