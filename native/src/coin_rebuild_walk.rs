//! Frozen rebuild cursor walk; source skips and uncertain mutations have distinct durable outcomes.
use crate::coin_rebuild_recovery::RecoveryIo;
use serde_json::{Value, json};
pub struct Options {
    pub owner: u64,
    pub base: String,
    pub title: String,
    pub bot: String,
}
pub enum Mutation {
    Answered,
    Rejected(String),
    Unknown(String),
}
pub trait BuildIo: RecoveryIo {
    fn live_count(&mut self, name: &str) -> Result<u64, String>;
    fn source_status(&mut self, key: &str) -> Result<Option<bool>, String>;
    fn delete_old(&mut self, state: &mut Value) -> Result<bool, String>;
    fn mutate(&mut self, intent: &Value, keywords: &str) -> Mutation;
    fn notify(&mut self, state: &mut Value, name: &str, title: &str) -> Result<(), String>;
    fn wait(&mut self);
}
fn permanent(error: &str) -> bool {
    let error = error.to_uppercase();
    [
        "STICKER_PNG_NOPNG",
        "STICKER_PNG_DIMENSIONS",
        "STICKER_DIMENSIONS_INVALID",
        "STICKER_FILE_INVALID",
        "STICKER_TOO_BIG",
        "STICKER_EMOJI_INVALID",
        "INVALID_STICKER_EMOJIS",
        "IMAGE_PROCESS_FAILED",
        "PHOTO_INVALID_DIMENSIONS",
        "STICKER_TGS_NOTGS",
        "STICKER_VIDEO_NOWEBM",
        "FILE MUST BE NON-EMPTY",
    ]
    .iter()
    .any(|m| error.contains(m))
}
#[allow(clippy::too_many_arguments)]
fn marker<I: BuildIo>(
    state: &mut Value,
    key: &str,
    operation: &str,
    name: &str,
    index: u64,
    before: u64,
    io: &mut I,
) -> Result<(), String> {
    state["in_flight"] = json!({"key":key,"operation":operation,"set_name":name,"set_index":index,"expected_before":before,"phase":"upload"});
    io.save(state)
}
pub fn walk<I: BuildIo>(
    plan: &[Value],
    state: &mut Value,
    options: &Options,
    io: &mut I,
) -> Result<(), String> {
    let problem = crate::coin_state::problem(state, Some(plan));
    if !problem.is_empty() {
        return Err(format!("rebuild resume state is inconsistent: {problem}"));
    }
    let state_object = state
        .as_object_mut()
        .ok_or("rebuild state must be an object")?;
    state_object.entry("order").or_insert_with(|| json!([]));
    state_object.entry("cursor").or_insert_with(|| json!(0));
    state_object.entry("in_flight").or_insert(Value::Null);
    if options.owner == 0 {
        return Err("ERROR: PACK_OWNER_USER_ID is not set (env or .env); refusing to start a rebuild without a pack owner.".into());
    }
    if plan.is_empty() {
        return Err(
            "ERROR: the rebuild plan is empty; refusing to delete the existing packs".into(),
        );
    }
    crate::coin_plan_disk::validate(&Value::Array(plan.to_vec()))?;
    let mut total = 0u64;
    for set in state["sets"]
        .as_array_mut()
        .ok_or("rebuild state has no sets")?
    {
        let name = set["name"].as_str().ok_or("rebuild set has no name")?;
        let count = io
            .live_count(name)
            .map_err(|e| format!("{e}; refusing to resume from a guessed live count"))?;
        total = total.checked_add(count).ok_or("rebuild count overflow")?;
        set["live"] = json!(count);
    }
    if !state["in_flight"].is_null() {
        total = crate::coin_rebuild_recovery::recover(state, total, &options.title, io)?;
    }
    let rebuilt = state["order"]
        .as_array()
        .ok_or("rebuild state has no order")?
        .len();
    let providers = state
        .get("provider_added")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    let recorded = rebuilt + providers;
    if total != recorded as u64 {
        let extra = if providers > 0 {
            format!(" ({rebuilt} by this rebuild + {providers} topped up by the providers)")
        } else {
            String::new()
        };
        return Err(format!(
            "ERROR: {total} stickers are live but {recorded} uploads are recorded{extra}.\n       Refusing to continue: the recorded order is what maps stickers to tickers, and continuing from a disagreeing state is what corrupts ticker_to_id.json.\n       Reconcile with coins/remap_ids.py, or delete the packs and the state file to rebuild cleanly."
        ));
    }
    let cursor = state["cursor"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .ok_or("invalid rebuild cursor")?;
    let sets = state["sets"]
        .as_array()
        .ok_or("rebuild state has no sets")?;
    io.report(&format!(
        "resume: {} sets, {total} live stickers, plan position {cursor}/{}",
        sets.len(),
        plan.len()
    ));
    if state["deleted_old"] != true {
        io.report(&format!(
            "deleting ALL old packs and rebuilding {} images...",
            plan.len()
        ));
        if !io.delete_old(state)? {
            return Err(
                "some old packs still exist; refusing to build a second family beside them".into(),
            );
        }
        state["deleted_old"] = json!(true);
        io.save(state)?;
    }
    let sets = state["sets"]
        .as_array()
        .ok_or("rebuild state has no sets")?;
    let mut index = sets.len() as u64;
    let mut name = String::new();
    let mut count = 0u64;
    if let Some(last) = sets
        .last()
        .filter(|s| s["live"].as_u64().is_some_and(|n| n < 200))
    {
        index = last["index"].as_u64().ok_or("invalid rebuild set index")?;
        name = last["name"]
            .as_str()
            .ok_or("rebuild set has no name")?
            .into();
        count = last["live"]
            .as_u64()
            .ok_or("rebuild set has no live count")?;
    }
    for (position, group) in plan.iter().enumerate().skip(cursor) {
        let key = group["rep"]
            .as_str()
            .ok_or("rebuild plan has no representative")?;
        let keywords = group["kw"].as_str().ok_or("rebuild plan has no keywords")?;
        let status = io.source_status(key)?;
        if status.is_none() || status == Some(true) {
            io.report(&format!(
                "  skip {key}: {}",
                if status.is_none() {
                    "missing/empty"
                } else {
                    "blank image (no blank emoji)"
                }
            ));
            state["cursor"] = json!(position + 1);
            io.save(state)?;
            continue;
        }
        state["cursor"] = json!(position + 1);
        let mut mutation = Mutation::Answered;
        let mut placed = false;
        if count != 0 {
            marker(state, key, "add", &name, index, count, io)?;
            mutation = io.mutate(&state["in_flight"], keywords);
            match &mutation {
                Mutation::Answered => placed = true,
                Mutation::Rejected(error) if error.contains("STICKERS_TOO_MUCH") => count = 0,
                _ => {}
            }
        }
        let should_create = !placed && (count == 0);
        if should_create {
            index = index.checked_add(1).ok_or("rebuild set index overflow")?;
            name = format!("{}{index}_by_{}", options.base, options.bot);
            marker(state, key, "create", &name, index, 0, io)?;
            mutation = io.mutate(&state["in_flight"], keywords);
            if matches!(mutation, Mutation::Answered) {
                state["sets"].as_array_mut().ok_or("rebuild state has no sets")?.push(json!({"index":index,"name":name,"title":format!("{} {index}",options.title)}));
                io.save(state)?;
                io.report(&format!("[set {index}] created {name}"));
            }
        }
        match mutation {
            Mutation::Answered => {
                count += 1;
                state["order"]
                    .as_array_mut()
                    .ok_or("rebuild state has no order")?
                    .push(json!(key));
                let before = state["in_flight"].clone();
                state["in_flight"] = Value::Null;
                if let Err(error) = io.save(state) {
                    state["in_flight"] = before;
                    return Err(error);
                }
                if count >= 200 {
                    io.notify(state, &name, &format!("{} {index}", options.title))?;
                    count = 0;
                }
            }
            Mutation::Unknown(error) => {
                return Err(format!("{key}: {error}; resolved on the next run"));
            }
            Mutation::Rejected(error) => {
                if !placed && count == 0 {
                    index = index.saturating_sub(1);
                }
                let before = state["in_flight"].clone();
                state["in_flight"] = Value::Null;
                if permanent(&error) {
                    io.report(&format!("  skip {key}: {error}"));
                    if let Err(save) = io.save(state) {
                        state["in_flight"] = before;
                        return Err(save);
                    }
                    continue;
                }
                state["cursor"] = json!(position);
                if let Err(save) = io.save(state) {
                    state["in_flight"] = before;
                    return Err(save);
                }
                return Err(format!("{key}: {error}; not applied, retried next run"));
            }
        }
        io.wait();
    }
    io.save(state)?;
    if let Some(last) = state["sets"]
        .as_array()
        .and_then(|sets| sets.last())
        .cloned()
    {
        io.notify(
            state,
            last["name"]
                .as_str()
                .ok_or("rebuild final set has no name")?,
            last["title"]
                .as_str()
                .ok_or("rebuild final set has no title")?,
        )?;
    }
    Ok(())
}
