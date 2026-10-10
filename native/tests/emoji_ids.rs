//! Python regex accepts Unicode decimal IDs; prose and overlong numbers remain excluded.
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
