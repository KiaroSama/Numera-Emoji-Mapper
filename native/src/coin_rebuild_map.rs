//! Rebuild-map admission composes proven content attribution and existing alias replay.
const REMAP_COMMAND: &str = if cfg!(numera_native_runtime) {
    "numera-emoji coin-remap-ids"
} else {
    "coins/remap_ids.py"
};

use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

pub trait MapIo {
    fn list(&mut self, name: &str) -> Result<Vec<Value>, String>;
    fn identity(
        &mut self,
        live: &[(String, String)],
        order: &[String],
    ) -> Result<Map<String, Value>, String>;
    fn current(&mut self) -> Result<Map<String, Value>, String>;
    fn save(&mut self, candidate: bool, value: &Value) -> Result<(), String>;
    fn fill(&mut self, mapping: &Map<String, Value>) -> Result<(), String>;
    fn report(&mut self, text: &str);
}
fn names(values: &[String]) -> String {
    format!(
        "[{}]",
        values
            .iter()
            .map(|v| crate::python_repr::quoted(v))
            .collect::<Vec<_>>()
            .join(", ")
    )
}
fn strings(value: Option<&Value>) -> Result<Vec<String>, String> {
    value.map_or(Ok(Vec::new()), |value| {
        value
            .as_array()
            .ok_or("rebuild names must be a list")?
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "rebuild name must be a string".into())
            })
            .collect()
    })
}
pub fn run<I: MapIo>(
    plan: &[Value],
    state: &Value,
    approved: &[String],
    io: &mut I,
) -> Result<(), String> {
    if plan.is_empty()
        || plan.iter().any(|g| {
            g["rep"].as_str().is_none_or(str::is_empty)
                || g["kw"].as_str().is_none()
                || g["tickers"].as_array().is_none_or(Vec::is_empty)
        })
    {
        return Err("rebuild plan entries are missing rep/tickers/kw".into());
    }
    let mut sets = state["sets"]
        .as_array()
        .ok_or("rebuild state has no sets")?
        .iter()
        .collect::<Vec<_>>();
    if sets
        .iter()
        .any(|s| s["index"].as_u64().is_none() || s["name"].as_str().is_none())
    {
        return Err("rebuild state has invalid set entries".into());
    }
    sets.sort_by_key(|s| s["index"].as_u64());
    let mut live = Vec::new();
    for set in &sets {
        for sticker in io.list(set["name"].as_str().ok_or("rebuild set name is missing")?)? {
            live.push((
                sticker["custom_emoji_id"]
                    .as_str()
                    .ok_or("rebuild live CID must be a string")?
                    .into(),
                sticker["file_id"]
                    .as_str()
                    .ok_or("rebuild live file ID must be a string")?
                    .into(),
            ));
        }
    }
    let order = strings(state.get("order"))?;
    if order.is_empty() {
        io.save(true, &json!({"error":"no uploads are recorded","live_stickers":live.len(),"plan_entries":plan.len()}))?;
        return Err(format!(
            "ERROR: no uploads are recorded but {} stickers are live.\n       Refusing to write ticker_to_id.json from positional guesswork -- that is what corrupts the map.\n       Rebuild identities from image content with {REMAP_COMMAND}. Details: ticker_to_id.candidate.json",
            live.len()
        ));
    }
    let provider = strings(state.get("provider_added"))?;
    let mut seen = BTreeSet::new();
    let recorded = order
        .iter()
        .chain(&provider)
        .filter(|name| seen.insert((*name).clone()))
        .cloned()
        .collect::<Vec<_>>();
    let identity = match io.identity(&live, &recorded) {
        Ok(mapping) => mapping,
        Err(error) => {
            io.save(true, &json!({"error":error,"recorded_uploads":order.len(),"live_stickers":live.len(),"plan_entries":plan.len()}))?;
            return Err(format!(
                "ERROR: {error}.\n       Refusing to write ticker_to_id.json: equal counts are not identity, and mapping by position is what put an unrelated logo on a real ticker.\n       Rebuild identities from image content with {REMAP_COMMAND}. Details: ticker_to_id.candidate.json"
            ));
        }
    };
    let mut mapping = Map::new();
    for (rep, cid) in &identity {
        // Python's by_rep dictionary keeps the last duplicate representative.
        if let Some(group) = plan.iter().rev().find(|g| g["rep"].as_str() == Some(rep)) {
            for ticker in strings(group.get("tickers"))? {
                mapping.insert(ticker, cid.clone());
            }
        }
    }
    for ticker in provider {
        let cid = identity
            .get(&ticker)
            .ok_or("provider upload has no proven identity")?;
        mapping.entry(ticker).or_insert_with(|| cid.clone());
    }
    io.report(&format!(
        "mapped by verified image identity ({} stickers)",
        live.len()
    ));
    let replay = crate::coin_plan::replay(&mapping, &io.current()?, approved)?;
    let result = replay["mapping"]
        .as_object()
        .ok_or("rebuild alias replay has no map")?;
    io.report(&format!(
        "  re-applied {} aliases from the current map.",
        result.len() - mapping.len()
    ));
    let lost = strings(replay.get("missing"))?;
    if !lost.is_empty() {
        io.save(true, &Value::Object(result.clone()))?;
        return Err(format!(
            "ERROR: {} alias(es) in ticker_to_id.json have no sibling in the rebuilt map (e.g. {}).\n       Refusing to write a map that drops them; review ticker_to_id.candidate.json and the plan.",
            lost.len(),
            names(&lost[..lost.len().min(6)])
        ));
    }
    let bad = replay["unapproved"]
        .as_object()
        .ok_or("rebuild alias replay has no shared groups")?;
    let mut biggest: Vec<String> = Vec::new();
    for group in bad.values() {
        let group = strings(Some(group))?;
        if group.len() > biggest.len() {
            biggest = group;
        }
    }
    if biggest.len() > 20 {
        io.save(true, &Value::Object(result.clone()))?;
        biggest.sort();
        return Err(format!(
            "ERROR: one emoji id would be shared by {} unreviewed tickers (e.g. {}).\n       A group that large is the signature of a mis-aligned mapping, not a shared logo. Refusing to overwrite ticker_to_id.json; review ticker_to_id.candidate.json instead.",
            biggest.len(),
            names(&biggest[..biggest.len().min(6)])
        ));
    }
    if !bad.is_empty() {
        io.report(&format!("  note: {} shared-logo group(s) are not listed in shared_logo_groups.json (largest {}). Cross-chain variants of one asset are expected here; add them to that file to silence this.", bad.len(), biggest.len()));
    }
    io.save(false, &Value::Object(result.clone()))?;
    io.report(&format!(
        "mapped {} tickers across {} sets ({} live stickers)",
        result.len(),
        sets.len(),
        live.len()
    ));
    io.fill(result)
}
