//! A replacement requires a new CID and a complete otherwise-unchanged before/after manifest.
pub fn new_id(before: &[String], after: &[String], position: i64) -> Option<String> {
    let position = usize::try_from(position).ok()?;
    if before.len() != after.len() || position >= after.len() {
        return None;
    }
    if before
        .iter()
        .zip(after)
        .enumerate()
        .any(|(index, (left, right))| index != position && left != right)
    {
        return None;
    }
    let new = &after[position];
    if new.is_empty() || new == "None" || before.contains(new) {
        return None;
    }
    Some(new.clone())
}
