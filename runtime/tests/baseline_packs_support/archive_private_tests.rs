use super::*;
use crate::baseline_packs_support::live;
// Original: tests.test_pack_archive.TheNameCarriesTheSlot.test_only_slot_one_can_be_the_logo
#[test]
#[allow(non_snake_case)]
fn pack_archive__TheNameCarriesTheSlot__test_only_slot_one_can_be_the_logo() {
    let key = "s:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let items =
        json!({key:{"path":"x.webp","fmt":"static","cid":"1","set":"set1","keywords":["lock"]}});
    let mut warnings = vec![];
    let rows = rows_with_warning(
        &json!({"name":"set1","title":"Pack One"}),
        &live(&["logo-cid", "1", "x2", "x3", "stray"]),
        &items,
        &json!({"1":key}),
        |message| warnings.push(message.to_owned()),
    )
    .unwrap();
    assert_eq!(
        warnings,
        vec![
            "Pack One: slot 3 is not in the catalog (x2)",
            "Pack One: slot 4 is not in the catalog (x3)",
            "Pack One: slot 5 is not in the catalog (stray)"
        ]
    );
    assert_eq!(rows[0]["file"], LOGO_NAME);
    assert!(rows[2..].iter().all(|r| r["file"].is_null()));
    assert_eq!(rows[4]["note"], "not in the catalog (unidentified)");
    let h = history(&json!({"name":"set1"}), &rows).unwrap();
    assert_eq!(h.matches(LOGO_NAME).count(), 1);
    assert!(h.contains("`unidentified`"));
}
