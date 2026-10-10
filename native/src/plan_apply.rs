//! Saved intent execution is a separate native command, dry-run unless explicitly applied.
use crate::{
    atomic, catalog::Catalog, config::Config, logging::RunLog, media_adapter::MediaAdapter,
    publication::Publisher, publish_collection::Options, telegram::Telegram,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
fn load(path: &Path, default: Value) -> Result<Value, String> {
    match std::fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|e| format!("cannot read {}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(default),
        Err(e) => Err(e.to_string()),
    }
}
fn steps(db: &Path, base: &str, plan: &Value, state: &Value) -> Result<Value, String> {
    let (items, ids) = crate::catalog_read::plan_rows(db, base)?;
    crate::plan_steps::compute(&json!({"plan":plan,"state":state,"items":items,"ids":ids}))
}
pub fn run(config: &Config, args: &[String], log: &RunLog) -> Result<u8, String> {
    if args.iter().any(|a| a == "-h" || a == "--help") {
        println!(
            "Usage: numera-emoji plan-apply [--base NAME] [--data-dir DIR] [--token-env KEY] [--user-id N] [--apply] [--max-changes 1..20] [--max-wait SECONDS]"
        );
        return Ok(0);
    }
    let (mut base, mut data, mut token_key, mut user, mut apply, mut max_changes, mut max_wait) = (
        config.value("COLLECTION_PACK_BASE").to_owned(),
        "collection".to_owned(),
        "GENERAL_BOT_TOKEN".to_owned(),
        config.integer("PACK_OWNER_USER_ID", 0, 0, i64::MAX) as u64,
        false,
        20u64,
        300u64,
    );
    let mut i = 0;
    while i < args.len() {
        let take = |i: &mut usize| -> Result<String, String> {
            *i += 1;
            args.get(*i)
                .cloned()
                .ok_or("option requires a value".into())
        };
        match args[i].as_str() {
            "--base" => base = take(&mut i)?,
            "--data-dir" => data = take(&mut i)?,
            "--token-env" => token_key = take(&mut i)?,
            "--user-id" => user = take(&mut i)?.parse().map_err(|_| "invalid user ID")?,
            "--max-changes" => {
                max_changes = take(&mut i)?.parse().map_err(|_| "invalid --max-changes")?
            }
            "--max-wait" => {
                max_wait = take(&mut i)?
                    .parse::<i64>()
                    .map_err(|_| "invalid --max-wait")?
                    .max(0) as u64
            }
            "--apply" => apply = true,
            value => return Err(format!("unknown plan-apply option: {value}")),
        }
        i += 1;
    }
    if !(1..=20).contains(&max_changes) {
        return Err("--max-changes must be 1..20".into());
    }
    crate::collection_names::valid_base(&base)?;
    let data = config.path(&data);
    let db = data.join("catalog.db");
    if !db.is_file() {
        return Err("data directory has no catalog.db".into());
    }
    let plan = crate::panel_save::read_plan(&data.join("pack_plan.json"))?;
    if plan.is_null() {
        println!("the panel has saved no plan");
        return Ok(0);
    }
    let state_path = data.join(format!("publish_{base}.json"));
    if !apply {
        let state = crate::resume::collection(load(&state_path, json!({}))?, &base)?;
        let steps = steps(&db, &base, &plan, &state)?;
        let (_, ids) = crate::catalog_read::plan_rows(&db, &base)?;
        println!("{}", crate::plan_steps::render(&steps, &ids, max_changes)?);
        return Ok(if steps["pending"] == true { 3 } else { 0 });
    }
    if user == 0 {
        return Err("--apply needs a valid user ID".into());
    }
    let token = config.require(&token_key)?;
    let logo_bots = config.logo_bots(true)?;
    let logo_path = if logo_bots.is_empty() {
        None
    } else {
        config.logo_path(true)?
    };
    if !logo_bots.is_empty() && logo_path.as_ref().is_none_or(|p| !p.is_file()) {
        return Err("required brand logo is missing".into());
    }
    let mut cat = Catalog::open(&db, &crate::logging::utc(), &config.root)?;
    let _family = crate::locks::Lease::acquire(
        &data.join(format!("publish_{base}.lock")),
        &crate::logging::utc(),
    )
    .map_err(|e| e.to_string())?;
    let plan = crate::panel_save::read_plan(&data.join("pack_plan.json"))?;
    if plan.is_null() {
        return Err("saved plan disappeared before apply ownership".into());
    }
    let mut state = crate::resume::collection(load(&state_path, json!({}))?, &base)?;
    let journal_path = data.join(format!("plan_apply_{base}.json"));
    let mut journal = load(
        &journal_path,
        json!({"version":1,"base":base,"intent":null,"retired":[]}),
    )?;
    if journal["base"] != base || !journal["retired"].is_array() {
        return Err("invalid apply journal".into());
    }
    let api = if std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref() == Ok("1") {
        std::env::var("NUMERA_TEST_API_BASE").unwrap_or_else(|_| "https://api.telegram.org".into())
    } else {
        "https://api.telegram.org".into()
    };
    let mut tg = Telegram::new(token.into(), api).map_err(|e| e.to_string())?;
    tg.max_flood_wait = Some(max_wait);
    let bot = tg
        .read_call("getMe", &BTreeMap::new())
        .map_err(|e| e.to_string())?["username"]
        .as_str()
        .ok_or("getMe has no username")?
        .to_owned();
    crate::plan_remove::settle(&tg, &mut cat, &mut state, &mut journal, &data, &base)?;
    let initial = steps(&db, &base, &plan, &state)?;
    let (_, ids) = crate::catalog_read::plan_rows(&db, &base)?;
    println!(
        "{}",
        crate::plan_steps::render(&initial, &ids, max_changes)?
    );
    let mut budget = max_changes;
    for row in initial["removals"]
        .as_array()
        .ok_or("invalid removal steps")?
        .iter()
        .take(budget as usize)
    {
        crate::plan_remove::remove(
            &tg,
            &mut cat,
            &mut state,
            &mut journal,
            &data,
            &base,
            (
                row[0].as_str().ok_or("invalid removal key")?,
                row[1].as_u64().ok_or("invalid removal pack")?,
            ),
        )?;
        budget -= 1;
    }
    let python = std::env::var_os("PYO3_PYTHON")
        .filter(|_| std::env::var("NUMERA_EMOJI_MAPPER_NO_DOTENV").as_deref() == Ok("1"))
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            config.root.join(if cfg!(windows) {
                ".venv/Scripts/python.exe"
            } else {
                ".venv/bin/python"
            })
        });
    let media = MediaAdapter {
        python,
        root: config.root.clone(),
        scratch: data.join("tmp/native-codec"),
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    if budget > 0 {
        for (pack, keys) in initial["adds"].as_object().ok_or("invalid add steps")? {
            let pack = pack.parse::<u64>().map_err(|_| "invalid target pack")?;
            let creating = initial["create"].as_u64() == Some(pack);
            let logo = if creating && logo_bots.contains(&bot.to_lowercase()) {
                Some(runtime.block_on(crate::brand_logo::prepare(
                    &media,
                    logo_path.as_ref().ok_or("missing brand logo")?,
                    &data,
                    config,
                ))?)
            } else {
                None
            };
            let logo_cost = u64::from(creating && logo.is_some());
            if budget <= logo_cost {
                break;
            }
            let keys = keys
                .as_array()
                .ok_or("invalid add keys")?
                .iter()
                .take((budget - logo_cost) as usize)
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .ok_or("invalid add key".into())
                })
                .collect::<Result<Vec<_>, String>>()?;
            let title = state["sets"]
                .as_array()
                .and_then(|v| v.first())
                .and_then(|v| v["title"].as_str())
                .ok_or("family has no recorded title")?;
            let title = title
                .rsplit_once(' ')
                .filter(|(_, tail)| !tail.is_empty() && tail.bytes().all(|b| b.is_ascii_digit()))
                .map_or(title, |(head, _)| head)
                .to_owned();
            let options = Options {
                base: base.clone(),
                title,
                bot: bot.clone(),
                user_id: user,
                fmt: "mixed".into(),
                per_set: plan["per_set"].as_u64().unwrap_or(200).min(200),
                new_set: creating,
                into_pack: if creating { None } else { Some(pack) },
                repaint: false,
                default_emoji: "😀".into(),
                logo,
            };
            let mut publisher = Publisher {
                tg: &tg,
                cat: &mut cat,
                media: &media,
                data: data.clone(),
                runtime: &runtime,
                config: Some(config),
            };
            let result = crate::publish_collection::publish_locked(
                &mut publisher,
                &options,
                &keys,
                &mut state,
            )?;
            budget = budget.saturating_sub(result["uploaded"].as_u64().unwrap_or(0) + logo_cost);
            if result["failed"].as_u64().unwrap_or(0) > 0 {
                return Ok(4);
            }
            if budget == 0 {
                break;
            }
        }
    }
    let left = steps(&db, &base, &plan, &state)?;
    if left["counted"].as_u64().unwrap_or(0) > 0 {
        println!("cap reached; run again to finish counted changes");
        return Ok(3);
    }
    let set_count = state["sets"].as_array().ok_or("invalid sets")?.len();
    for i in 0..set_count {
        if left["reorder"]
            .as_array()
            .ok_or("invalid reorder steps")?
            .contains(&state["sets"][i]["index"])
        {
            let result = crate::sync_order::sync(&tg, &cat, &mut state["sets"][i], true);
            // Earlier successful sets remain durable even if a later set refuses.
            atomic::write_json(&state_path, &state).map_err(|e| e.to_string())?;
            result?;
        }
    }
    atomic::write_json(&state_path, &state).map_err(|e| e.to_string())?;
    log.event("INFO", "plan", "saved plan execution finished");
    Ok(0)
}
