use super::*;
fn check(after: Value, comparison: BTreeMap<String, Option<bool>>, retry: bool) -> Mutation {
    mutation(
        false,
        Some(members(&["a", "b"])),
        Some(listed(after)),
        None,
        comparison,
        retry,
        false,
    )
}
// tests.test_telegram_client.AddedCheckUsesIdentity.test_our_own_new_sticker_is_recognised
#[test]
fn test_our_own_new_sticker_is_recognised() {
    let m = mutation(
        false,
        Some(members(&["a", "b"])),
        Some(listing(&["a", "b", "MINE"])),
        Some(listing(&["a", "b", "MINE"])),
        matches(&[("MINE", Some(true))]),
        false,
        false,
    );
    assert_eq!(m.result.unwrap(), json!(3));
    assert_eq!(mutation_count_ref(&m.requests), 1);
    assert_eq!(m.compared, vec!["MINE", "MINE"]);
}
fn mutation_count_ref(r: &[Request]) -> usize {
    r.iter().filter(|r| r.method == "addStickerToSet").count()
}
// tests.test_telegram_client.AddedCheckUsesIdentity.test_nothing_new_means_not_applied
#[test]
fn test_nothing_new_means_not_applied() {
    let m = check(members(&["a", "b"]), matches(&[]), true);
    assert!(matches!(m.result, Err(ApiError::Rejected(_))));
    assert_eq!(mutation_count(&m), 2, "a definite no permits the retry");
    assert!(m.compared.is_empty());
}
// tests.test_telegram_client.AddedCheckUsesIdentity.test_one_new_but_FOREIGN_sticker_is_not_our_upload
#[test]
#[allow(non_snake_case)]
fn test_one_new_but_FOREIGN_sticker_is_not_our_upload() {
    let m = check(
        members(&["a", "b", "THEIRS"]),
        matches(&[("THEIRS", Some(false))]),
        true,
    );
    assert!(matches!(m.result, Err(ApiError::Rejected(_))));
    assert_eq!(
        mutation_count(&m),
        2,
        "a stranger is a definite no, not upload success"
    );
    assert_eq!(m.compared, vec!["THEIRS"]);
}
// tests.test_telegram_client.AddedCheckUsesIdentity.test_one_new_sticker_we_cannot_verify_is_unknown
#[test]
fn test_one_new_sticker_we_cannot_verify_is_unknown() {
    let m = check(members(&["a", "b", "?"]), matches(&[("?", None)]), false);
    assert!(matches!(m.result, Err(ApiError::Ambiguous(_))));
    assert_eq!(mutation_count(&m), 1);
    assert_eq!(m.compared, vec!["?"]);
}
// tests.test_telegram_client.AddedCheckUsesIdentity.test_two_new_identities_are_unattributable
#[test]
fn test_two_new_identities_are_unattributable() {
    let m = check(
        members(&["a", "b", "THEIRS", "MINE"]),
        matches(&[("THEIRS", Some(true)), ("MINE", Some(true))]),
        false,
    );
    assert!(matches!(m.result, Err(ApiError::Ambiguous(_))));
    assert_eq!(mutation_count(&m), 1);
    assert!(m.compared.is_empty());
}
// tests.test_telegram_client.AddedCheckUsesIdentity.test_a_replacement_is_not_read_as_our_add
#[test]
fn test_a_replacement_is_not_read_as_our_add() {
    let m = check(
        members(&["a", "THEIRS", "MINE"]),
        matches(&[("THEIRS", Some(true)), ("MINE", Some(true))]),
        false,
    );
    assert!(matches!(m.result, Err(ApiError::Ambiguous(_))));
    assert_eq!(mutation_count(&m), 1);
    assert!(m.compared.is_empty());
}
// tests.test_telegram_client.AddedCheckUsesIdentity.test_without_a_snapshot_nothing_is_claimed
#[test]
fn test_without_a_snapshot_nothing_is_claimed() {
    let m = mutation(
        false,
        None,
        Some(listing(&["a", "b", "x"])),
        None,
        matches(&[("x", Some(true))]),
        false,
        false,
    );
    assert!(matches!(m.result, Err(ApiError::Ambiguous(_))));
    assert_eq!(mutation_count(&m), 1);
    assert!(m.compared.is_empty());
}
// tests.test_telegram_client.AddedCheckUsesIdentity.test_stickers_without_identity_are_unknown_not_applied
#[test]
fn test_stickers_without_identity_are_unknown_not_applied() {
    let m = mutation(
        false,
        Some(members(&["None"])),
        Some(listed(json!([{"i":0},{"i":1}]))),
        None,
        matches(&[]),
        false,
        false,
    );
    assert!(matches!(m.result, Err(ApiError::Ambiguous(_))));
    assert_eq!(mutation_count(&m), 1);
    assert!(m.compared.is_empty());
}
// tests.test_telegram_client.AddedCheckUsesIdentity.test_duplicate_identities_are_not_trusted
#[test]
fn test_duplicate_identities_are_not_trusted() {
    let after = members(&["same", "same"]);
    assert!(rt::telegram::usable_fuids(after.as_array().unwrap()).is_none());
    let m = mutation(
        false,
        Some(members(&["x"])),
        Some(listed(after)),
        None,
        matches(&[]),
        false,
        false,
    );
    assert!(matches!(m.result, Err(ApiError::Ambiguous(_))));
    assert_eq!(mutation_count(&m), 1);
}
// tests.test_telegram_client.AddedCheckUsesIdentity.test_usable_fuids_accepts_a_well_formed_set
#[test]
fn test_usable_fuids_accepts_a_well_formed_set() {
    assert_eq!(
        rt::telegram::usable_fuids(members(&["a", "b"]).as_array().unwrap()),
        Some(BTreeSet::from(["a".into(), "b".into()]))
    );
}
// tests.test_telegram_client.AddedCheckUsesIdentity.test_usable_fuids_rejects_a_missing_id
#[test]
fn test_usable_fuids_rejects_a_missing_id() {
    assert!(rt::telegram::usable_fuids(members(&["a", ""]).as_array().unwrap()).is_none());
}
