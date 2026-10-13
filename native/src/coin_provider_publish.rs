//! Frozen provider top-up decisions; one durable intent precedes every non-idempotent effect.
use crate::coin_provider_proof::{RecoveryIo, record, recover};
use serde_json::{Map, Value, json};

pub enum Mutation {
    Answered,
    Rejected,
    Unknown,
}
pub trait PublishIo: RecoveryIo {
    fn live_count(&mut self, name: &str) -> Result<usize, String>;
    fn source(&mut self, ticker: &str) -> Result<(String, u64), String>;
    fn mutate(&mut self, intent: &Value, keywords: &str) -> Mutation;
    fn pause(&mut self) -> Result<(), String>;
}

#[allow(clippy::too_many_arguments)]
pub fn publish<I: PublishIo>(
    state: &mut Value,
    mapping: &mut Map<String, Value>,
    tickers: &[String],
    base: &str,
    title: &str,
    bot: &str,
    utc: &str,
    keywords: &Map<String, Value>,
    io: &mut I,
) -> Result<(usize, usize), String> {
    let recovered = match recover(state, mapping, io) {
        Ok(value) => value,
        Err(_) => return Ok((0, tickers.len())),
    };
    let sets = state["sets"]
        .as_array()
        .ok_or("provider publication has no sets list")?;
    let Some(mut last) = sets.iter().max_by_key(|set| set["index"].as_u64()).cloned() else {
        return Ok((0, tickers.len()));
    };
    let mut added = 0;
    let mut failed = 0;
    for (position, ticker) in tickers.iter().enumerate() {
        if recovered.as_deref() == Some(ticker.as_str()) {
            added += 1;
            continue;
        }
        if mapping
            .get(ticker)
            .and_then(Value::as_str)
            .is_some_and(|id| !id.is_empty())
        {
            continue;
        }
        let name = last["name"].as_str().ok_or("provider set has no name")?;
        let before = match io.live_count(name) {
            Ok(count) => count,
            Err(_) => {
                failed += 1;
                continue;
            }
        };
        let old_index = last["index"].as_u64().ok_or("provider set has no index")?;
        let creating = before >= 200;
        let index = if creating {
            old_index
                .checked_add(1)
                .ok_or("provider set index overflow")?
        } else {
            old_index
        };
        let name = if creating {
            format!("{base}{index}_by_{bot}")
        } else {
            name.into()
        };
        let title = if creating {
            format!("{title} {index}")
        } else {
            last["title"].as_str().unwrap_or("").into()
        };
        let (source, want) = match io.source(ticker) {
            Ok(value) => value,
            Err(_) => {
                failed += 1;
                continue;
            }
        };
        let intent = json!({"key":ticker,"operation":if creating{"create"}else{"add"},"set_name":name,"set_index":index,
            "expected_before":if creating{0}else{before},"title":title,"format":"static","started_utc":utc,
            "source_dhash":want,"source_path":source});
        state["provider_in_flight"] = intent.clone();
        io.save("state", state)?;
        match io.mutate(
            &intent,
            keywords
                .get(ticker)
                .and_then(Value::as_str)
                .unwrap_or(ticker),
        ) {
            Mutation::Rejected => {
                state["provider_in_flight"] = Value::Null;
                io.save("state", state)?;
                failed += 1;
                continue;
            }
            Mutation::Unknown => return Ok((added, failed + tickers.len() - position)),
            Mutation::Answered => {}
        }
        let cid = match io.landed(&intent, want) {
            Ok(value) => value,
            Err(_) => return Ok((added, failed + tickers.len() - position)),
        };
        let Some(cid) = cid else {
            state["provider_in_flight"] = Value::Null;
            io.save("state", state)?;
            failed += 1;
            continue;
        };
        if creating {
            last = json!({"index":index,"name":name,"title":title,"live":1});
            state["sets"]
                .as_array_mut()
                .ok_or("provider publication has no sets list")?
                .push(last.clone());
        }
        record(state, ticker)?;
        mapping.insert(ticker.clone(), json!(cid));
        io.save("map", &Value::Object(mapping.clone()))?;
        io.promote(&intent, want)?;
        state["provider_in_flight"] = Value::Null;
        io.save("state", state)?;
        added += 1;
        io.pause()?;
    }
    Ok((added, failed))
}
