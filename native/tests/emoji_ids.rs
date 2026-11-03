//! Python regex accepts Unicode decimal IDs; prose and overlong numbers remain excluded.
use _native::emoji_ids;

const A: &str = "111111111";
const B: &str = "222222222";
const C: &str = "333333333";
const EXAMPLE: &str = "444444444";

#[test]
fn original_entry_cases_exclude_examples_and_preserve_labels_and_digit_bounds() {
    for (text, expected) in [
        (format!("premium-id:{A}\npremium-id:{B}\n"), vec![A, B]),
        (
            format!(
                "- Set a custom_emoji_id (e.g. premium-id: {EXAMPLE}) OR a full\npremium-id:{A}\n"
            ),
            vec![A],
        ),
        (
            format!("premium-id:{A} 👋 waving hand\n- premium-id:{B} | fire\n"),
            vec![A, B],
        ),
        (
            format!("  - premium-id:{A}\n* PREMIUM-ID : {B}\npremium-id:  {C}\n"),
            vec![A, B, C],
        ),
        (format!("{A}\n{B}\n   {C}   \n"), vec![A, B, C]),
        (
            format!("premium-id:123\nrandom prose {A} in text\nhello\n"),
            vec![],
        ),
        (
            format!("premium-id:{}\npremium-id:{A}\n", "9".repeat(30)),
            vec![A],
        ),
    ] {
        assert_eq!(emoji_ids::entries(&text), expected, "{text}");
    }
}

#[test]
fn original_within_file_duplicates_do_not_count_prose() {
    assert_eq!(
        emoji_ids::within_file_duplicates(&format!(
            "premium-id:{A}\npremium-id:{B}\npremium-id:{A}\n"
        )),
        vec![(A.to_owned(), 2)]
    );
    assert!(
        emoji_ids::within_file_duplicates(&format!(
            "- Set a custom_emoji_id (e.g. premium-id: {EXAMPLE}) OR a full\npremium-id:{EXAMPLE}\n"
        ))
        .is_empty()
    );
    assert_eq!(
        emoji_ids::within_file_duplicates(&format!("{B}\n{A}\n{B}\n{A}\n{B}\n")),
        vec![(B.to_owned(), 3), (A.to_owned(), 2)]
    );
}

#[test]
fn original_collection_preserves_first_seen_order_and_reports_duplicates() {
    use std::path::Path;
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let parent =
        _native::tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
    let directory = parent.join(format!("inventory reports {}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let a = directory.join("a.md");
    let b = directory.join("b.md");
    std::fs::write(&a, format!("premium-id:{A}\npremium-id:{B}\n")).unwrap();
    std::fs::write(&b, format!("premium-id:{B}\npremium-id:{C}\n")).unwrap();
    let files = [a.to_str().unwrap().into(), b.to_str().unwrap().into()];
    let mut messages = Vec::new();
    assert_eq!(
        emoji_ids::collect_with_report(&files, &[], |message| messages.push(message.to_owned()))
            .unwrap(),
        vec![A, B, C]
    );
    assert_eq!(
        messages,
        [
            "a.md: 2 real entries (2 unique)".to_owned(),
            "b.md: 2 real entries (2 unique)".to_owned(),
            "collected 4 real id occurrences -> 3 unique (1 ids duplicated across files)"
                .to_owned(),
            format!("duplicate ids (fetched once each): {B}x2"),
        ]
    );
    std::fs::write(&a, format!("premium-id:{A}\npremium-id:{A}\n")).unwrap();
    messages.clear();
    assert_eq!(
        emoji_ids::collect_with_report(&files[..1], &[A.into(), B.into()], |message| messages
            .push(message.to_owned()))
        .unwrap(),
        vec![A, B]
    );
    assert_eq!(
        messages,
        [
            format!("a.md: 2 real entries (1 unique), within-file duplicate(s): {A}x2"),
            "collected 4 real id occurrences -> 2 unique (1 ids duplicated across files)"
                .to_owned(),
            format!("duplicate ids (fetched once each): {A}x3"),
        ]
    );
    std::fs::write(
        &a,
        format!("- e.g. premium-id: {EXAMPLE} in the header\npremium-id:{A}\n"),
    )
    .unwrap();
    messages.clear();
    assert_eq!(
        emoji_ids::collect_with_report(&files[..1], &[], |message| messages
            .push(message.to_owned()))
        .unwrap(),
        vec![A]
    );
    assert_eq!(
        messages,
        [
            "a.md: 1 real entries (1 unique), 1 example/prose mention(s) skipped",
            "collected 1 real id occurrences -> 1 unique (0 ids duplicated across files)",
        ]
    );
    assert_eq!(emoji_ids::collect(&files[..1], &[]).unwrap(), vec![A]);
    for (text, expected, per_file) in [
        (
            "PREMİUM-ıD:\u{1f}۱۲۳۴۵\nexample premium-id: ۱۲۳۴۵\n".to_owned(),
            vec!["۱۲۳۴۵"],
            "a.md: 1 real entries (1 unique), 1 example/prose mention(s) skipped",
        ),
        (
            format!("{A}\n{B}\n"),
            vec![A, B],
            "a.md: 2 real entries (2 unique)",
        ),
        (
            format!("prose premium-id:\n{B}\npremium-id:{A}\n"),
            vec![A],
            "a.md: 1 real entries (1 unique), 1 example/prose mention(s) skipped",
        ),
        (
            format!("premium-id:{}\npremium-id:{A}\n", "9".repeat(26)),
            vec![A],
            "a.md: 1 real entries (1 unique)",
        ),
    ] {
        std::fs::write(&a, text.as_bytes()).unwrap();
        messages.clear();
        assert_eq!(
            emoji_ids::collect_with_report(&files[..1], &[], |message| messages
                .push(message.to_owned()))
            .unwrap(),
            expected
        );
        assert_eq!(messages[0], per_file);
        assert_eq!(messages.len(), 2);
        assert_eq!(std::fs::read(&a).unwrap(), text.as_bytes());
    }
    std::fs::write(&a, b"premium-id:\xff111111111").unwrap();
    messages.clear();
    assert!(
        emoji_ids::collect_with_report(&files[..1], &[], |message| messages
            .push(message.to_owned()))
        .is_err()
    );
    assert!(
        messages.is_empty(),
        "malformed UTF-8 must not be silently replaced"
    );
    assert_eq!(std::fs::read(&a).unwrap(), b"premium-id:\xff111111111");
    std::fs::remove_dir_all(directory).expect("clean verified inventory fixture");
}

#[test]
fn duplicate_reports_keep_first_seen_ties_and_limit_only_the_summary() {
    let ids = (10000..10012)
        .rev()
        .map(|id| id.to_string())
        .collect::<Vec<_>>();
    let mut inline = ids.clone();
    inline.extend(ids.clone());
    let mut messages = Vec::new();
    assert_eq!(
        emoji_ids::collect_with_report(&[], &inline, |message| messages.push(message.to_owned()))
            .unwrap(),
        ids
    );
    assert_eq!(
        messages,
        [
            "collected 24 real id occurrences -> 12 unique (12 ids duplicated across files)",
            "duplicate ids (fetched once each): 10011x2, 10010x2, 10009x2, 10008x2, 10007x2, 10006x2, 10005x2, 10004x2, 10003x2, 10002x2",
        ]
    );
}

#[test]
fn unicode_decimal_inventory_and_line_breaks_match_source() {
    let text = "premium-id: ۱۲۳۴۵ 😀\u{2028}- PREMIUM-ID : 54321\nexample premium-id: 99999\npremium-id: 12345678901234567890123456";
    assert_eq!(_native::emoji_ids::entries(text), vec!["۱۲۳۴۵", "54321"]);
    assert_eq!(
        _native::emoji_ids::entries("１２３４５\n99999"),
        vec!["１２３４５", "99999"]
    );
    assert_eq!(
        _native::emoji_ids::collect(
            &[],
            &[
                " ²①𐩁 ".into(),
                "¼".into(),
                "²①𐩁".into(),
                "۱۲۳۴۵".into(),
                "".into()
            ],
        )
        .unwrap(),
        vec!["²①𐩁", "۱۲۳۴۵"],
        "inline str.isdigit accepts nondecimal digits and keeps first-seen strings"
    );
    assert_eq!(
        _native::emoji_ids::entries("PREMİUM-ıD: 12345"),
        vec!["12345"],
        "source IGNORECASE includes dotted and dotless I"
    );
}
