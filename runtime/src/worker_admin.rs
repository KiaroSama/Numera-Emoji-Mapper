//! Developer Worker administration; credential values never enter argv or reports.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
const HELP: &str = "Usage: numera-worker-admin secrets [--env-file PATH] [--dry-run] [--dev-vars]\n       numera-worker-admin webhooks [--base-url URL] [--only general|coin|both] [--env-file PATH] [--status|--delete] [--force]";
const MAX_ADMIN: u64 = 9_007_199_254_740_991;
#[derive(Clone, Copy)]
enum Selection {
    General,
    Coin,
    Both,
}
struct SecretsOptions {
    env_file: Option<PathBuf>,
    dry_run: bool,
    dev_vars: bool,
}
struct WebhookOptions {
    env_file: Option<PathBuf>,
    base_url: Option<String>,
    selection: Selection,
    status: bool,
    delete: bool,
    force: bool,
}
enum Action {
    Help,
    Secrets(SecretsOptions),
    Webhooks(WebhookOptions),
}
fn value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} requires a value\n{HELP}"))
}
fn parse(args: &[String]) -> Result<Action, String> {
    let Some(command) = args.first() else {
        return Ok(Action::Help);
    };
    if matches!(command.as_str(), "--help" | "-h") && args.len() == 1 {
        return Ok(Action::Help);
    }
    let mut secrets = SecretsOptions {
        env_file: None,
        dry_run: false,
        dev_vars: false,
    };
    let mut hooks = WebhookOptions {
        env_file: None,
        base_url: None,
        selection: Selection::Both,
        status: false,
        delete: false,
        force: false,
    };
    let mut help = false;
    let mut index = 1;
    if !matches!(command.as_str(), "secrets" | "webhooks") {
        return Err(format!("expected secrets or webhooks\n{HELP}"));
    }
    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => help = true,
            "--env-file" => {
                let path = PathBuf::from(value(args, &mut index, "--env-file")?);
                if command == "secrets" {
                    secrets.env_file = Some(path)
                } else {
                    hooks.env_file = Some(path)
                }
            }
            "--dry-run" if command == "secrets" => secrets.dry_run = true,
            "--dev-vars" if command == "secrets" => secrets.dev_vars = true,
            "--base-url" if command == "webhooks" => {
                hooks.base_url = Some(value(args, &mut index, "--base-url")?)
            }
            "--only" if command == "webhooks" => {
                hooks.selection = match value(args, &mut index, "--only")?
                    .to_ascii_lowercase()
                    .as_str()
                {
                    "general" => Selection::General,
                    "coin" => Selection::Coin,
                    "both" => Selection::Both,
                    _ => return Err("--only requires general, coin or both".into()),
                }
            }
            "--status" if command == "webhooks" => hooks.status = true,
            "--delete" if command == "webhooks" => hooks.delete = true,
            "--force" if command == "webhooks" => hooks.force = true,
            _ => return Err(format!("unknown Worker administration argument\n{HELP}")),
        }
        index += 1;
    }
    if help {
        Ok(Action::Help)
    } else if command == "secrets" {
        Ok(Action::Secrets(secrets))
    } else {
        Ok(Action::Webhooks(hooks))
    }
}
struct Environment {
    path: PathBuf,
    values: BTreeMap<String, String>,
}
impl Environment {
    fn load(root: &Path, supplied: Option<&Path>) -> Result<Self, String> {
        let path = match supplied {
            Some(path) if path.is_absolute() => path.into(),
            Some(path) => std::env::current_dir()
                .map_err(|_| "cannot resolve --env-file")?
                .join(path),
            None => root.join(".env"),
        };
        let text = std::fs::read_to_string(&path).map_err(
            |_| "cannot read UTF-8 environment file; copy .env.example and fill it in first",
        )?;
        let mut values = BTreeMap::new();
        let input = text
            .strip_prefix('\u{feff}')
            .unwrap_or(&text)
            .replace("\r\n", "\n")
            .replace('\r', "\n");
        for line in input.split('\n') {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                values.insert(
                    key.trim().to_ascii_uppercase(),
                    value.trim().trim_matches('"').trim_matches('\'').into(),
                );
            }
        }
        Ok(Self { path, values })
    }
    fn get(&self, key: &str) -> &str {
        self.values.get(key).map(String::as_str).unwrap_or("")
    }
    fn sensitive(&self) -> Vec<String> {
        [
            "GENERAL_BOT_TOKEN",
            "TELEGRAM_BOT_TOKEN",
            "COIN_BOT_TOKEN",
            "WORKER_PUBLISH_SECRET",
            "PUBLISH_SECRET",
            "GENERAL_WEBHOOK_SECRET",
            "COIN_WEBHOOK_SECRET",
        ]
        .into_iter()
        .map(|key| self.get(key))
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .collect()
    }
}
struct Secret {
    name: &'static str,
    env_key: &'static str,
    value: String,
    source: String,
    generated: bool,
}
fn canonical_admin(raw: &str) -> Option<String> {
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let significant = raw.trim_start_matches('0');
    if significant.is_empty() {
        return None;
    }
    let number = significant.parse::<u64>().ok()?;
    if number == 0 || number > MAX_ADMIN {
        return None;
    }
    Some(number.to_string())
}
fn admin_ids(env: &Environment) -> (String, String, usize, usize) {
    let explicit = !env.get("ADMIN_USER_IDS").is_empty();
    let split = |value: &str| {
        value
            .split(|c: char| c.is_whitespace() || c == ',' || c == ';')
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let raws = if explicit {
        split(env.get("ADMIN_USER_IDS"))
    } else {
        std::iter::once(env.get("PACK_OWNER_USER_ID").to_owned())
            .chain(split(env.get("BOT_ALLOWED_USER_IDS")))
            .collect()
    };
    let mut ids = vec![];
    let mut seen = BTreeSet::new();
    let mut rejected = 0;
    for raw in raws {
        let raw = raw.trim();
        if raw.is_empty() {
            continue;
        }
        match canonical_admin(raw) {
            Some(id) => {
                if seen.insert(id.clone()) {
                    ids.push(id);
                }
            }
            None => rejected += 1,
        }
    }
    let source = if explicit {
        "ADMIN_USER_IDS (explicit)"
    } else {
        "PACK_OWNER_USER_ID + BOT_ALLOWED_USER_IDS"
    };
    let count = ids.len();
    (
        ids.join(","),
        format!("{source} ({count} id(s))"),
        count,
        rejected,
    )
}
fn diagnostic_key_label(name: &str) -> &'static str {
    match name {
        "GENERAL_BOT_TOKEN" => "GENERAL_BOT_TOKEN",
        "COIN_BOT_TOKEN" => "COIN_BOT_TOKEN",
        "PACK_LINKS_CHAT_ID" => "PACK_LINKS_CHAT_ID",
        "LOG_CHAT_ID" => "LOG_CHAT_ID",
        "ADMIN_USER_IDS" => "ADMIN_USER_IDS",
        "PUBLISH_SECRET" => "PUBLISH_SECRET",
        "GENERAL_WEBHOOK_SECRET" => "GENERAL_WEBHOOK_SECRET",
        "COIN_WEBHOOK_SECRET" => "COIN_WEBHOOK_SECRET",
        _ => "[redacted]",
    }
}
fn secret_plan(env: &Environment, report: &mut impl FnMut(&str)) -> Result<Vec<Secret>, String> {
    let (admins, admin_source, _, rejected) = admin_ids(env);
    if rejected > 0 {
        report(&format!(
            "WARNING: {rejected} admin id(s) are not positive safe ASCII decimal integers and were dropped."
        ));
    }
    let mut plan = vec![];
    for (name, key, source, generate) in [
        (
            "GENERAL_BOT_TOKEN",
            "GENERAL_BOT_TOKEN",
            "GENERAL_BOT_TOKEN",
            false,
        ),
        (
            "COIN_BOT_TOKEN",
            "TELEGRAM_BOT_TOKEN",
            "TELEGRAM_BOT_TOKEN (the coin bot)",
            false,
        ),
        (
            "PACK_LINKS_CHAT_ID",
            "PACK_LINKS_CHAT_ID",
            "PACK_LINKS_CHAT_ID",
            false,
        ),
        ("LOG_CHAT_ID", "LOG_CHAT_ID", "LOG_CHAT_ID", false),
        ("ADMIN_USER_IDS", "ADMIN_USER_IDS", "", false),
        (
            "PUBLISH_SECRET",
            "WORKER_PUBLISH_SECRET",
            "WORKER_PUBLISH_SECRET",
            true,
        ),
        (
            "GENERAL_WEBHOOK_SECRET",
            "GENERAL_WEBHOOK_SECRET",
            ".env",
            true,
        ),
        ("COIN_WEBHOOK_SECRET", "COIN_WEBHOOK_SECRET", ".env", true),
    ] {
        let value = if name == "ADMIN_USER_IDS" {
            admins.clone()
        } else {
            env.get(key).to_owned()
        };
        plan.push(Secret {
            name,
            env_key: key,
            value,
            source: if name == "ADMIN_USER_IDS" {
                admin_source.clone()
            } else {
                source.into()
            },
            generated: generate,
        });
    }
    let missing = plan
        .iter()
        .filter(|s| s.value.is_empty() && !s.generated)
        .map(|s| diagnostic_key_label(s.name))
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(format!(
            "cannot continue - no value for: {}",
            missing.join(", ")
        ));
    }
    for secret in &mut plan {
        if secret.value.is_empty() && secret.generated {
            let mut bytes = [0_u8; 32];
            getrandom::fill(&mut bytes)
                .map_err(|_| "cannot generate Worker secrets from OS randomness")?;
            secret.value = URL_SAFE_NO_PAD.encode(bytes);
            secret.source = "generated (32 random bytes)".into();
        } else {
            secret.generated = false;
        }
    }
    Ok(plan)
}

struct Settings {
    telegram_base: String,
    environment: BTreeMap<String, String>,
    wall: std::time::Duration,
    idle: std::time::Duration,
}
impl Settings {
    fn native() -> Self {
        Self {
            telegram_base: "https://api.telegram.org".into(),
            environment: std::env::vars().collect(),
            wall: std::time::Duration::from_secs(120),
            idle: std::time::Duration::from_secs(60),
        }
    }
}
// No captured streams and no named input file: Wrangler may echo credentials on either stream.
fn owned_status(
    root: &Path,
    mut command: std::process::Command,
    settings: &Settings,
) -> Result<bool, String> {
    use crate::{process_owner::OwnedProcess, tooling_process};
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_| "cannot initialize Worker child ownership")?;
    let directory = tooling_process::workspace(root)?;
    let owner_directory = directory.join("owner");
    if std::fs::create_dir(&owner_directory).is_err() {
        tooling_process::clean_unspawned(&directory)?;
        return Err("cannot initialize Worker child workspace".into());
    }
    command
        .env_clear()
        .envs(&settings.environment)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let mut owner = match OwnedProcess::spawn_session(&mut command, owner_directory) {
        Ok(owner) => owner,
        Err(_) => {
            drop(command);
            tooling_process::clean_unspawned(&directory)?;
            return Err(
                "cannot start owned Worker prerequisite process; check local installation".into(),
            );
        }
    };
    drop(command);
    let status = runtime.block_on(owner.wait(settings.wall, settings.idle));
    owner
        .finish()
        .map_err(|_| "Worker child cleanup could not be verified; ownership workspace retained")?;
    tooling_process::clean_unspawned(&directory)?;
    status
        .map(|s| s.success())
        .map_err(|_| "Worker child exceeded its execution bounds".to_owned())
}
fn put_secret(root: &Path, secret: &Secret, settings: &Settings) -> Result<(), String> {
    use std::io::{Seek, Write};
    let script = root.join("worker/node_modules/wrangler/bin/wrangler.js");
    if !script.is_file() {
        return Err("local Wrangler is missing; install the Worker dependencies first".into());
    }
    let mut input =
        tempfile::tempfile().map_err(|_| "cannot create anonymous Worker secret input")?;
    input
        .write_all(secret.value.as_bytes())
        .and_then(|_| input.write_all(b"\n"))
        .and_then(|_| input.rewind())
        .map_err(|_| "cannot prepare anonymous Worker secret input")?;
    let mut command = std::process::Command::new("node");
    command
        .current_dir(root.join("worker"))
        .arg(script)
        .args(["secret", "put", secret.name])
        .stdin(std::process::Stdio::from(input));
    // Override inherited flags after env_clear in owned_status through a private copy.
    let mut child = Settings {
        telegram_base: settings.telegram_base.clone(),
        environment: settings.environment.clone(),
        wall: settings.wall,
        idle: settings.idle,
    };
    child
        .environment
        .insert("WRANGLER_WRITE_LOGS".into(), "false".into());
    child
        .environment
        .insert("WRANGLER_SEND_METRICS".into(), "false".into());
    if !owned_status(root, command, &child)? {
        return Err(format!(
            "Wrangler secret put {} failed; generated environment values were not persisted",
            diagnostic_key_label(secret.name)
        ));
    }
    Ok(())
}
fn persist_generated(env: &Environment, plan: &[Secret]) -> Result<(), String> {
    let generated: BTreeMap<_, _> = plan
        .iter()
        .filter(|s| s.generated)
        .map(|s| (s.env_key, s.value.as_str()))
        .collect();
    if generated.is_empty() {
        return Ok(());
    }
    let mut remaining = generated.clone();
    let mut lines = Vec::new();
    let current = std::fs::read_to_string(&env.path).map_err(|_| "Worker secrets were uploaded but environment could not be re-read before generated-key commit")?;
    let current = current
        .strip_prefix('\u{feff}')
        .unwrap_or(&current)
        .replace("\r\n", "\n")
        .replace('\r', "\n");
    for line in current.lines() {
        let replacement = line.trim().split_once('=').and_then(|(key, _)| {
            generated
                .get(key.trim().to_ascii_uppercase().as_str())
                .map(|value| (key.trim().to_ascii_uppercase(), *value))
        });
        if let Some((key, value)) = replacement {
            lines.push(format!("{key}={value}"));
            remaining.remove(key.as_str());
        } else {
            lines.push(line.to_owned());
        }
    }
    for (key, value) in remaining {
        lines.push(format!("{key}={value}"));
    }
    crate::atomic::write_bytes(&env.path, (lines.join("\n") + "\n").as_bytes()).map_err(|_| {
        "Worker secrets were uploaded but generated environment persistence failed".to_owned()
    })
}
fn dev_vars(root: &Path, plan: &[Secret], settings: &Settings) -> Result<(), String> {
    let mut command = std::process::Command::new("git");
    command
        .current_dir(root.join("worker"))
        .args(["check-ignore", "-q", "--", ".dev.vars"])
        .stdin(std::process::Stdio::null());
    if !owned_status(root, command, settings)? {
        return Err(
            "refusing worker/.dev.vars: git check-ignore must confirm it is ignored".into(),
        );
    }
    let mut text = String::new();
    for secret in plan {
        if secret.value.contains(['\r', '\n']) {
            return Err(format!(
                "{} cannot contain line breaks in .dev.vars",
                secret.name
            ));
        }
        text.push_str(&format!("{}={}\n", secret.name, secret.value));
    }
    crate::atomic::write_bytes(&root.join("worker/.dev.vars"), text.as_bytes())
        .map_err(|_| "cannot persist ignored worker/.dev.vars".to_owned())
}
fn bot_call(
    client: &reqwest::blocking::Client,
    settings: &Settings,
    env: &Environment,
    token: &str,
    method: &str,
    body: serde_json::Value,
) -> Result<serde_json::Value, String> {
    use std::io::Read;
    let response = client
        .post(format!(
            "{}/bot{token}/{method}",
            settings.telegram_base.trim_end_matches('/')
        ))
        .json(&body)
        .send()
        .map_err(|_| format!("{method}: Bot API transport failed"))?;
    let success = response.status().is_success();
    let mut bytes = Vec::new();
    response
        .take(1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| format!("{method}: cannot read Bot API response"))?;
    if bytes.len() > 1024 * 1024 {
        return Err(format!("{method}: Bot API response exceeded limit"));
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|_| format!("{method}: malformed Bot API response"))?;
    if !success || value.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
        let description = value
            .get("description")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("request rejected");
        return Err(crate::logging::redact(
            &format!("{method}: {description}"),
            &env.sensitive(),
        ));
    }
    value
        .get("result")
        .cloned()
        .ok_or_else(|| format!("{method}: missing Bot API result"))
}
fn webhooks(
    root: &Path,
    options: WebhookOptions,
    settings: &Settings,
    report: &mut impl FnMut(&str),
) -> Result<u8, String> {
    use serde_json::{Value, json};
    let env = Environment::load(root, options.env_file.as_deref())?;
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .connect_timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .build()
        .map_err(|_| "cannot initialize Bot API transport")?;
    for (role, token_key, secret_key) in [
        ("general", "GENERAL_BOT_TOKEN", "GENERAL_WEBHOOK_SECRET"),
        ("coin", "TELEGRAM_BOT_TOKEN", "COIN_WEBHOOK_SECRET"),
    ] {
        if matches!(
            (options.selection, role),
            (Selection::General, "coin") | (Selection::Coin, "general")
        ) {
            continue;
        }
        let token = env.get(token_key);
        if token.is_empty() {
            report(&format!("  {role}: no token in environment - skipped"));
            continue;
        }
        let info = bot_call(&client, settings, &env, token, "getWebhookInfo", json!({}))?;
        let current = info
            .as_object()
            .and_then(|v| v.get("url"))
            .and_then(Value::as_str)
            .ok_or("getWebhookInfo: malformed webhook metadata")?;
        if options.status {
            let pending = info
                .get("pending_update_count")
                .and_then(Value::as_u64)
                .map(|count| count.to_string())
                .unwrap_or_else(|| "?".into());
            let error = info
                .get("last_error_message")
                .and_then(Value::as_str)
                .unwrap_or("");
            report(&crate::logging::redact(
                &format!(
                    "  {role}: {} | pending {pending} | last error: {error}",
                    if current.is_empty() {
                        "(none - polling)"
                    } else {
                        current
                    }
                ),
                &env.sensitive(),
            ));
            continue;
        }
        if options.delete {
            if bot_call(&client, settings, &env, token, "deleteWebhook", json!({}))? != json!(true)
            {
                return Err("deleteWebhook: malformed success result".into());
            }
            report(&format!(
                "  {role}: webhook deleted; the native bot can poll this token again"
            ));
            continue;
        }
        let base = options
            .base_url
            .as_deref()
            .filter(|s| !s.is_empty())
            .ok_or("--base-url is required")?;
        let secret = env.get(secret_key);
        if secret.is_empty() {
            return Err(format!("{role}: no webhook secret; run secrets first"));
        }
        let target = format!("{}/tg/{role}", base.trim_end_matches('/'));
        if !current.is_empty() && current != target && !options.force {
            report(&format!(
                "  {role}: existing different webhook - refusing replacement; rerun with --force"
            ));
            continue;
        }
        let result = bot_call(
            &client,
            settings,
            &env,
            token,
            "setWebhook",
            json!({"url":target,"secret_token":secret,"allowed_updates":["message","edited_message","channel_post"],"drop_pending_updates":false}),
        )?;
        if result != json!(true) {
            return Err("setWebhook: malformed success result".into());
        }
        report(&format!("  {role}: webhook registered"));
        if current.is_empty() {
            report("      note: the native poller can no longer poll this token");
        }
    }
    Ok(0)
}
fn execute_with(
    root: &Path,
    action: Action,
    settings: &Settings,
    report: &mut impl FnMut(&str),
) -> Result<u8, String> {
    match action {
        Action::Help => {
            report(HELP);
            Ok(0)
        }
        Action::Secrets(options) => {
            let env = Environment::load(root, options.env_file.as_deref())?;
            let plan = secret_plan(&env, report)?;
            for secret in &plan {
                report(&format!(
                    "  {:<22} <- {}  ({} chars)",
                    secret.name,
                    secret.source,
                    secret.value.encode_utf16().count()
                ));
            }
            if options.dry_run {
                report("dry run - Wrangler was not called.");
                return Ok(0);
            }
            if options.dev_vars {
                dev_vars(root, &plan, settings)?;
                let generated = plan.iter().filter(|secret| secret.generated).count();
                if generated > 0 {
                    report(&format!(
                        "{generated} secret(s) were generated for .dev.vars only; .env is unchanged."
                    ));
                }
                report("wrote ignored worker/.dev.vars; Wrangler was not called");
                return Ok(0);
            }
            for secret in &plan {
                put_secret(root, secret, settings)?;
                report(&format!("  {} uploaded", secret.name));
            }
            persist_generated(&env, &plan)?;
            report("Worker secrets uploaded; generated values persisted to environment");
            Ok(0)
        }
        Action::Webhooks(options) => webhooks(root, options, settings, report),
    }
}
pub fn run(args: &[String], report: &mut impl FnMut(&str)) -> Result<u8, String> {
    let action = parse(args)?;
    if matches!(action, Action::Help) {
        report(HELP);
        return Ok(0);
    }
    let root = crate::tooling_environment::project_root()?;
    execute_with(&root, action, &Settings::native(), report)
}
#[cfg(test)]
#[path = "../tests/worker_admin_support/secrets.rs"]
mod secret_tests;
#[cfg(test)]
#[path = "../tests/worker_admin_support/webhooks.rs"]
mod webhook_tests;
