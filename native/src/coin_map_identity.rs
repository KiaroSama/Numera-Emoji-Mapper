//! One-to-one content attribution; recorded order is never interpreted as live position.
use serde_json::{Map, Value};
fn names(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|name| crate::python_repr::quoted(name))
            .collect::<Vec<_>>()
            .join(", ")
    )
}
pub fn resolve<E, H>(
    sources: &[(String, u64)],
    live: &[(String, u64)],
    mut exists: E,
    mut hash: H,
) -> Result<Map<String, Value>, String>
where
    E: FnMut(&str) -> bool,
    H: FnMut(&str, u64) -> Result<u64, String>,
{
    let mut want = Map::new();
    for (name, value) in sources {
        if !exists(name) {
            return Err(format!(
                "the source image for {} is gone, so no live sticker can be proven to be it",
                crate::python_repr::quoted(name)
            ));
        }
        want.insert(name.clone(), Value::from(*value));
    }
    let mut result = Map::new();
    for (cid, value) in live {
        let got = hash(cid, *value)
            .map_err(|error| format!("live sticker {cid} could not be read ({error})"))?;
        // ponytail: O(live x plan) Hamming scan; use hash buckets if measured families outgrow it.
        let hits = want
            .iter()
            .filter(|(_, value)| {
                value
                    .as_u64()
                    .is_some_and(|want| (want ^ got).count_ones() <= 8)
            })
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>();
        if hits.len() != 1 {
            let mut ordered = hits.clone();
            ordered.sort();
            return Err(format!(
                "live sticker {cid} matches {} uploaded image(s) {}; it cannot be attributed to a coin",
                hits.len(),
                names(&ordered[..ordered.len().min(4)])
            ));
        }
        let name = &hits[0];
        if let Some(old) = result.get(name).and_then(Value::as_str) {
            return Err(format!(
                "{} is live twice ({old} and {cid}); the pack contains a duplicate",
                crate::python_repr::quoted(name)
            ));
        }
        result.insert(name.clone(), Value::String(cid.clone()));
    }
    let missing = sources
        .iter()
        .filter(|(name, _)| !result.contains_key(name))
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!(
            "{} recorded upload(s) have no live sticker (e.g. {})",
            missing.len(),
            names(&missing[..missing.len().min(5)])
        ));
    }
    Ok(result)
}
