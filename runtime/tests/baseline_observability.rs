//! Original logging, branding and test-double contract cases at their Rust boundaries.
use numera_runtime as production;
use production::{config::Config, logging, media_native_adapter::NativeMediaAdapter};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};
#[allow(dead_code)]
#[path = "baseline_catalog_support/mod.rs"]
mod baseline_catalog_support;
#[allow(dead_code, unused_imports)]
#[path = "baseline_publication_support/mod.rs"]
mod publication_support;
use baseline_catalog_support::Scratch;

fn config(root: &Path, values: &[(&str, &str)]) -> Config {
    let mut environment = BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]);
    environment.extend(
        values
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string())),
    );
    Config::load_with_environment(root.into(), &environment).unwrap()
}
fn old_log(root: &Path, name: &str, days: u64) -> PathBuf {
    let directory = root.join("logs");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join(name);
    let file = std::fs::File::create(&path).unwrap();
    file.set_modified(SystemTime::now() - Duration::from_secs(days * 86400))
        .unwrap();
    path
}
fn log_text(log: &logging::RunLog) -> String {
    std::fs::read_to_string(log.path.as_ref().unwrap()).unwrap()
}
const TOKEN: &str = "1234567890:AAxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx";

// tests.test_logsetup.TestRetentionIsReadWhenItIsUsed.test_a_value_set_after_import_is_honoured
#[test]
fn retention_is_resolved_for_each_loaded_invocation() {
    let root = Scratch::new();
    let first = config(&root.0, &[]);
    assert_eq!(
        logging::retention_days(Some(first.value("EMOJI_LOG_RETENTION_DAYS")))
            .unwrap()
            .0,
        30
    );
    let second = config(&root.0, &[("EMOJI_LOG_RETENTION_DAYS", "7")]);
    assert_eq!(
        logging::retention_days(Some(second.value("EMOJI_LOG_RETENTION_DAYS")))
            .unwrap()
            .0,
        7
    );
}
// tests.test_logsetup.TestRetentionIsReadWhenItIsUsed.test_prune_takes_its_default_at_call_time
#[test]
fn retention_application_uses_current_configuration() {
    let root = Scratch::new();
    let old = old_log(&root.0, "old.log", 90);
    let config = config(&root.0, &[("EMOJI_LOG_RETENTION_DAYS", "0")]);
    let log = logging::RunLog::open(&root.0, "retention", vec![]);
    log.apply_retention(&root.0, config.value("EMOJI_LOG_RETENTION_DAYS"));
    assert!(old.is_file());
    assert!(!log_text(&log).contains("pruned_logs="));
}
// tests.test_logsetup.TestRetentionIsReadWhenItIsUsed.test_a_malformed_value_falls_back_instead_of_raising
#[test]
fn malformed_retention_warns_and_logging_remains_available() {
    let root = Scratch::new();
    let (days, warning) = logging::retention_days(Some("not-a-number")).unwrap();
    assert_eq!(days, 30);
    assert!(warning.unwrap().contains("using 30"));
    let log = logging::RunLog::open(&root.0, "malformed", vec![]);
    log.apply_retention(&root.0, "not-a-number");
    log.event("INFO", "test", "still usable");
    assert!(log_text(&log).contains("still usable"));
}
// tests.test_logsetup.TestRetentionIsReadWhenItIsUsed.test_a_negative_value_cannot_mean_prune_everything
#[test]
fn negative_retention_disables_pruning() {
    assert_eq!(logging::retention_days(Some("-1")).unwrap().0, 0);
}
// tests.test_logsetup.TestRedaction.test_token_shaped_is_masked
#[test]
fn token_shape_is_masked() {
    let text = logging::redact(&format!("using {TOKEN} now"), &[]);
    assert!(!text.contains(TOKEN));
    assert!(text.contains("[REDACTED]"));
}
// tests.test_logsetup.TestRedaction.test_bot_url_is_masked
#[test]
fn bot_url_is_masked() {
    let text = logging::redact(&format!("GET /bot{TOKEN}/getMe"), &[]);
    assert!(!text.contains(TOKEN));
    assert!(text.contains("/bot[REDACTED]/"));
}
// tests.test_logsetup.TestRedaction.test_registered_secret_value_is_masked
#[test]
fn configured_literal_secret_is_masked() {
    let root = Scratch::new();
    let secret = "00000000000000000000000000000000";
    let config = config(&root.0, &[("CMC_API_KEY", secret)]);
    assert!(!logging::redact(&format!("key={secret}"), &config.secret_values()).contains(secret));
}
// tests.test_logsetup.TestRedaction.test_short_value_not_registered
#[test]
fn explicitly_supplied_short_secrets_follow_native_redaction_policy() {
    // The reference Rust API deliberately masks every explicitly supplied secret,
    // including short values; it has no process-global eight-character registry.
    assert_eq!(logging::redact("abc", &["abc".into()]), "[REDACTED]");
    assert_eq!(logging::redact("abc", &[]), "abc");
}
// tests.test_logsetup.TestRedaction.test_content_key_and_ids_preserved
#[test]
fn content_keys_and_numeric_ids_are_preserved() {
    let text = "key s:01f6c1284261975d0000934b7fe6f3c3 cid 111111111";
    assert_eq!(logging::redact(text, &[]), text);
}
// tests.test_logsetup.TestRedaction.test_a_token_glued_to_bot_is_masked
#[test]
fn bot_prefix_does_not_hide_a_token() {
    let token = "123456789:ABCDEFGHIJKLMNOPQRSTUVWXYZ012";
    assert!(
        !logging::redact(&format!("POST https://api.telegram.org/bot{token}"), &[]).contains(token)
    );
}
// tests.test_logsetup.TestRedaction.test_the_worker_secrets_are_masked_by_name
#[test]
fn worker_secret_names_are_recognized_by_actual_configuration() {
    let root = Scratch::new();
    for key in [
        "WORKER_PUBLISH_SECRET",
        "GENERAL_WEBHOOK_SECRET",
        "COIN_WEBHOOK_SECRET",
    ] {
        let secret = format!("synthetic-never-issued-{key}");
        let config = config(&root.0, &[(key, &secret)]);
        assert_eq!(config.secret_values(), vec![secret.clone()]);
        assert_eq!(
            logging::redact(&secret, &config.secret_values()),
            "[REDACTED]"
        );
    }
}
// tests.test_logsetup.TestRedaction.test_formatter_redacts_record
#[test]
fn formatter_redacts_and_retains_actual_run_identity() {
    let root = Scratch::new();
    let log = logging::RunLog::open(&root.0, "formatter", vec![]);
    log.event("INFO", "test", &format!("token {TOKEN}"));
    let text = log_text(&log);
    assert!(!text.contains(TOKEN));
    assert!(text.contains("token [REDACTED]"));
    let ids = text
        .lines()
        .map(|line| line.rsplit_once(" run=").unwrap().1)
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), 2);
    assert!(!ids[0].is_empty());
    assert_eq!(ids[0], ids[1]);
}
// tests.test_logsetup.TestRunOutcome.test_exit_code_is_recorded_and_returned
#[test]
fn outcome_is_recorded_and_returned() {
    let root = Scratch::new();
    let log = logging::RunLog::open(&root.0, "outcome", vec![]);
    assert_eq!(log.finish(3), 3);
    let text = log_text(&log);
    assert!(text.contains("exit=3"));
    assert!(text.contains("outcome=FAILED(3)"));
}
// tests.test_logsetup.TestLogRetention.test_old_logs_are_pruned_and_recent_ones_kept
#[test]
fn old_logs_are_pruned_and_recent_logs_remain() {
    let root = Scratch::new();
    let old = old_log(&root.0, "old.log", 90);
    let recent = old_log(&root.0, "new.log", 1);
    assert_eq!(logging::prune_old_logs(&root.0, 30).unwrap(), 1);
    assert!(!old.exists());
    assert!(recent.is_file());
}
// tests.test_logsetup.TestLogRetention.test_retention_can_be_disabled
#[test]
fn pruning_can_be_disabled() {
    let root = Scratch::new();
    let old = old_log(&root.0, "old.log", 900);
    assert_eq!(logging::prune_old_logs(&root.0, 0).unwrap(), 0);
    assert!(old.is_file());
}
// tests.test_logsetup.TestNoCommittedSecrets.test_no_real_secret_is_committed
#[test]
fn tracked_file_secret_guard_uses_registered_values_and_redacted_locations() {
    // Exercise the original Git-tracked-file algorithm with never-issued local
    // values. The outer test guard intentionally withholds operator credentials.
    let root = Scratch::new();
    let secret = "synthetic-never-issued-local-sentinel";
    std::fs::write(
        root.0.join(".env"),
        format!("GENERAL_BOT_TOKEN={secret}\nPUBLIC_LABEL=public\n"),
    )
    .unwrap();
    let config = Config::load_with_environment(root.0.clone(), &BTreeMap::new()).unwrap();
    let secrets = config.secret_values();
    assert_eq!(secrets, vec![secret]);
    let git = |args: &[&str]| {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(&root.0)
            .output()
            .unwrap();
        assert!(output.status.success());
        output.stdout
    };
    git(&["init", "-q"]);
    std::fs::write(root.0.join("tracked.txt"), "safe public content").unwrap();
    git(&["add", "tracked.txt"]);
    let scan = || {
        let files = git(&["ls-files", "-z"]);
        let mut leaks = vec![];
        for file in files
            .split(|byte| *byte == 0)
            .filter(|name| !name.is_empty())
        {
            let name = std::str::from_utf8(file).unwrap();
            let path = root.0.join(name);
            let metadata = path.metadata().unwrap();
            if metadata.is_file() && metadata.len() <= 2 * 1024 * 1024 {
                let bytes = std::fs::read(path).unwrap();
                if secrets.iter().any(|value| {
                    bytes
                        .windows(value.len())
                        .any(|window| window == value.as_bytes())
                }) {
                    leaks.push(format!("registered secret -> {name}"));
                }
            }
        }
        leaks
    };
    assert!(scan().is_empty());
    std::fs::write(root.0.join("tracked.txt"), secret).unwrap();
    let leaks = scan();
    assert_eq!(leaks, vec!["registered secret -> tracked.txt"]);
    assert!(!leaks.join("\n").contains(secret));
}

fn media(root: &Path) -> NativeMediaAdapter {
    NativeMediaAdapter {
        executable: env!("CARGO_BIN_EXE_numera-emoji").into(),
        root: root.into(),
        scratch: root.join("codec"),
    }
}
fn logo_source(path: &Path, color: [u8; 4]) {
    let mut image = image::RgbaImage::new(120, 120);
    for y in 20..100 {
        for x in 20..100 {
            image.put_pixel(x, y, image::Rgba(color));
        }
    }
    image.save(path).unwrap();
}
fn prepare(root: &Path, source: &Path) -> Result<Value, String> {
    publication_support::runtime().block_on(production::brand_logo::prepare(
        &media(root),
        source,
        root,
        &config(root, &[]),
    ))
}
fn prepared_path(value: &Value) -> &Path {
    Path::new(value["file_path"].as_str().unwrap())
}
// tests.test_brand_logo.BrandLogoPrepare.test_static_logo_is_100x100_png
#[test]
fn static_logo_is_a_100_pixel_png() {
    let root = Scratch::new();
    let source = root.0.join("logo.png");
    logo_source(&source, [200, 30, 30, 255]);
    let logo = prepare(&root.0, &source).unwrap();
    let output = prepared_path(&logo);
    assert!(output.is_file());
    assert_eq!(image::image_dimensions(output).unwrap(), (100, 100));
    assert_eq!(
        image::guess_format(&std::fs::read(output).unwrap()).unwrap(),
        image::ImageFormat::Png
    );
}
// tests.test_brand_logo.BrandLogoPrepare.test_static_png_is_cached
#[test]
fn static_logo_cache_is_reused() {
    let root = Scratch::new();
    let source = root.0.join("logo.png");
    logo_source(&source, [200, 30, 30, 255]);
    let first = prepare(&root.0, &source).unwrap();
    let before = prepared_path(&first)
        .metadata()
        .unwrap()
        .modified()
        .unwrap();
    let second = prepare(&root.0, &source).unwrap();
    assert_eq!(first, second);
    assert_eq!(
        prepared_path(&second)
            .metadata()
            .unwrap()
            .modified()
            .unwrap(),
        before
    );
}
// tests.test_brand_logo.BrandLogoPrepare.test_missing_source_returns_none
#[test]
fn missing_required_logo_is_a_typed_refusal() {
    let root = Scratch::new();
    let source = root.0.join("nope.png");
    assert!(!source.exists());
    assert_eq!(
        prepare(&root.0, &source).unwrap_err(),
        "required brand logo is missing"
    );
    assert!(!root.0.join("brand").exists());
}
// tests.test_brand_logo.BrandLogoPrepare.test_a_corrupt_logo_raises
#[test]
fn corrupt_logo_is_refused() {
    let root = Scratch::new();
    let source = root.0.join("corrupt.png");
    std::fs::write(&source, b"not an image").unwrap();
    assert!(prepare(&root.0, &source).is_err());
}
// tests.test_brand_logo.BrandLogoPrepare.test_a_blank_logo_raises
#[test]
fn blank_logo_is_refused() {
    let root = Scratch::new();
    let source = root.0.join("blank.png");
    image::RgbaImage::new(100, 100).save(&source).unwrap();
    assert!(prepare(&root.0, &source).is_err());
}
// tests.test_brand_logo.BrandLogoPrepare.test_a_truncated_cache_is_rendered_again
#[test]
fn truncated_logo_cache_is_rendered_again() {
    let root = Scratch::new();
    let source = root.0.join("logo.png");
    logo_source(&source, [200, 30, 30, 255]);
    let first = prepare(&root.0, &source).unwrap();
    let output = prepared_path(&first);
    let bytes = std::fs::read(output).unwrap();
    std::fs::write(output, &bytes[..10]).unwrap();
    let next = prepare(&root.0, &source).unwrap();
    assert_eq!(prepared_path(&next), output);
    assert_eq!(
        image::open(output).unwrap().to_rgba8().dimensions(),
        (100, 100)
    );
}

fn publish_branded(
    animated: bool,
    enabled: bool,
) -> (
    publication_support::Fixture,
    Value,
    publication_support::Bot,
) {
    use publication_support::*;
    let mut fixture = Fixture::empty();
    if animated {
        let mut cat = fixture.catalog();
        let mut keys = vec![];
        // The source case deliberately uses gzip-magic dummy artwork; blankness
        // admission for animated uploads must remain optional evidence.
        for i in 0..2 {
            let path = fixture.data.join(format!("media/animated/a{i}.tgs"));
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let mut bytes = vec![0x1f, 0x8b];
            bytes.extend(vec![b'x'; 50 + i]);
            std::fs::write(&path, bytes).unwrap();
            let key =
                production::codec_lottie_identity::fingerprint_typed(&path).unwrap()["content_key"]
                    .as_str()
                    .unwrap()
                    .to_owned();
            cat.insert(&json!({"content_key":key,"fmt":"animated","file_path":path,"emojis":["😀"],"keywords":[format!("a{i}")]}),UTC).unwrap();
            cat.record_seen(&format!("UP-a{i}"), &key).unwrap();
            keys.push(key);
        }
        fixture.keys = keys;
    } else {
        let mut keys = vec![];
        let mut cat = fixture.catalog();
        for i in 0..2 {
            let path = fixture.data.join(format!("media/static/item{i}.png"));
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            logo_source(&path, [10, 20 * (i as u8 + 1), 200, 255]);
            let key = key(&path);
            cat.insert(&json!({"content_key":key,"fmt":"static","file_path":path,"emojis":["😀"],"keywords":[format!("item{i}")]}),UTC).unwrap();
            cat.record_seen(&format!("UP-item{i}"), &key).unwrap();
            keys.push(key);
        }
        fixture.keys = keys;
    }
    let bot = Bot::new(Model::ordinary());
    let mut state = fixture.state();
    let mut options = fixture.options();
    if animated {
        options.fmt = "animated".into();
    }
    if enabled {
        let path = fixture.data.join("logo.png");
        logo_source(&path, [0, 200, 0, 255]);
        options.logo = Some(prepare(&fixture.root.0, &path).unwrap());
    }
    let mut catalog = fixture.catalog();
    let rt = runtime();
    let telegram = bot.telegram();
    let adapter = media(&fixture.root.0);
    let config = fixture.config(&[]);
    let mut publisher = production::publication::Publisher {
        tg: &telegram,
        cat: &mut catalog,
        media: &adapter,
        data: fixture.data.clone(),
        runtime: &rt,
        config: Some(&config),
    };
    let result = production::publish_collection::publish(
        &mut publisher,
        &options,
        &fixture.keys,
        &mut state,
    )
    .unwrap();
    drop(catalog);
    assert_eq!(result["uploaded"], 2);
    assert_eq!(result["failed"], 0);
    (fixture, state, bot)
}
// tests.test_brand_logo.PublishFormatLogoFirst.test_logo_is_first_and_cids_offset
#[test]
fn logo_leads_and_catalog_ids_have_the_correct_offset() {
    let (fixture, state, bot) = publish_branded(false, true);
    let model = bot.model.lock().unwrap();
    let stickers = &model.sets[publication_support::SET];
    assert_eq!(stickers.len(), 3);
    assert_eq!(state["sets"][0]["logo"], true);
    let first = &model
        .requests
        .iter()
        .find(|request| request.path.ends_with("createNewStickerSet"))
        .unwrap();
    let initial: Value = serde_json::from_str(&first.form["stickers"]).unwrap();
    assert_eq!(initial[0]["emoji_list"], json!(["✅"]));
    assert_eq!(
        fixture.cid(&fixture.keys[0]),
        format!("{}-1", publication_support::SET)
    );
    assert_eq!(
        fixture.cid(&fixture.keys[1]),
        format!("{}-2", publication_support::SET)
    );
}
// tests.test_brand_logo.PublishFormatLogoFirst.test_static_logo_leads_an_animated_set
#[test]
fn static_logo_leads_an_animated_set() {
    let (_fixture, state, bot) = publish_branded(true, true);
    let model = bot.model.lock().unwrap();
    let set = "pka1_by_YourEmojiBot";
    assert_eq!(model.sets[set].len(), 3);
    assert_eq!(state["sets"][0]["logo"], true);
    let mutations = model
        .requests
        .iter()
        .filter(|request| {
            request.path.ends_with("createNewStickerSet")
                || request.path.ends_with("addStickerToSet")
        })
        .collect::<Vec<_>>();
    assert_eq!(mutations.len(), 3);
    let initial: Value = serde_json::from_str(&mutations[0].form["stickers"]).unwrap();
    assert_eq!(initial[0]["emoji_list"], json!(["✅"]));
    let formats = mutations
        .iter()
        .map(|r| {
            if r.path.ends_with("createNewStickerSet") {
                serde_json::from_str::<Value>(&r.form["stickers"]).unwrap()[0]["format"].clone()
            } else {
                serde_json::from_str::<Value>(&r.form["sticker"]).unwrap()["format"].clone()
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(
        formats,
        vec![json!("static"), json!("animated"), json!("animated")]
    );
}
// tests.test_brand_logo.PublishFormatLogoFirst.test_no_logo_when_disabled
#[test]
fn disabling_branding_preserves_the_first_artwork_slot() {
    let (fixture, state, bot) = publish_branded(false, false);
    assert_eq!(
        bot.model.lock().unwrap().sets[publication_support::SET].len(),
        2
    );
    assert_eq!(state["sets"][0]["logo"], false);
    assert_eq!(
        fixture.cid(&fixture.keys[0]),
        format!("{}-0", publication_support::SET)
    );
}
// tests.test_brand_logo.TheRepositoryShipsNoOperatorLogo.test_no_default_logo_or_bot_list_survives_in_source
#[test]
fn configuration_has_no_default_operator_logo_or_bot_allowlist() {
    let root = Scratch::new();
    let config = config(&root.0, &[]);
    assert_eq!(config.logo_path(false).unwrap(), None);
    assert!(config.logo_bots(false).unwrap().is_empty());
    assert!(
        config
            .logo_bots(true)
            .unwrap_err()
            .contains("BRAND_LOGO_BOTS")
    );
    assert!(config.brand_logo(false, None).is_err());
}

struct CodecImplementations(Vec<(PathBuf, Vec<(String, usize)>)>);
impl<'ast> syn::visit::Visit<'ast> for CodecImplementations {
    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        if item
            .trait_
            .as_ref()
            .is_some_and(|(path, _)| path.segments.last().unwrap().ident == "MediaCodec")
        {
            let methods = item
                .items
                .iter()
                .filter_map(|item| {
                    if let syn::ImplItem::Fn(method) = item {
                        Some((method.sig.ident.to_string(), method.sig.inputs.len()))
                    } else {
                        None
                    }
                })
                .collect();
            self.0.push((PathBuf::new(), methods));
        }
        syn::visit::visit_item_impl(self, item);
    }
}
fn rust_sources(root: &Path, result: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(root).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_sources(&path, result);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            result.push(path);
        }
    }
}
fn implementations() -> CodecImplementations {
    use syn::visit::Visit;
    let runtime = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut sources = vec![];
    rust_sources(&runtime.join("tests"), &mut sources);
    rust_sources(&runtime.join("src"), &mut sources);
    let mut out = CodecImplementations(vec![]);
    for path in sources {
        let tree = syn::parse_file(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let start = out.0.len();
        out.visit_file(&tree);
        for implementation in &mut out.0[start..] {
            implementation.0 = path.clone();
        }
    }
    out
}
fn codec_contract() -> Vec<(String, usize, bool)> {
    let source =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/media_adapter.rs"))
            .unwrap();
    let tree = syn::parse_file(&source).unwrap();
    let contract = tree
        .items
        .iter()
        .find_map(|item| {
            if let syn::Item::Trait(item) = item {
                (item.ident == "MediaCodec").then_some(item)
            } else {
                None
            }
        })
        .unwrap();
    contract
        .items
        .iter()
        .filter_map(|item| {
            if let syn::TraitItem::Fn(method) = item {
                Some((
                    method.sig.ident.to_string(),
                    method.sig.inputs.len(),
                    method.default.is_none(),
                ))
            } else {
                None
            }
        })
        .collect()
}
fn missing_methods(methods: &[(String, usize)], contract: &[(String, usize, bool)]) -> Vec<String> {
    contract
        .iter()
        .filter(|(name, _, required)| {
            *required && !methods.iter().any(|(actual, _)| actual == name)
        })
        .map(|(name, _, _)| name.clone())
        .collect()
}
fn wrong_arities(methods: &[(String, usize)], contract: &[(String, usize, bool)]) -> Vec<String> {
    methods
        .iter()
        .filter(|(name, count)| {
            contract
                .iter()
                .any(|(required, wanted, _)| required == name && wanted != count)
        })
        .map(|(name, _)| name.clone())
        .collect()
}
// tests.test_fake_contracts.EveryFakeAcceptsTheRealCall.test_the_scan_finds_something
#[test]
fn actual_codec_double_scan_is_not_vacuous() {
    let found = implementations();
    assert!(
        found.0.len() >= 5,
        "contract scan stopped finding real implementations"
    );
    assert!(
        found
            .0
            .iter()
            .any(|(path, _)| path.ends_with("baseline_publication_support/mod.rs"))
    );
    assert!(
        found
            .0
            .iter()
            .any(|(path, _)| path.ends_with("baseline_media_support/mod.rs"))
    );
}
// tests.test_fake_contracts.EveryFakeAcceptsTheRealCall.test_no_fake_rejects_a_keyword_the_real_client_accepts
#[test]
fn typed_doubles_expose_every_required_production_operation() {
    // Rust has no keyword call syntax. Its actual trait is the mechanical
    // admission boundary; all these implementation bodies are compiler checked.
    let contract = codec_contract();
    assert!(
        contract
            .iter()
            .any(|(name, _, required)| name == "call" && *required)
    );
    let found = implementations();
    assert!(found.0.len() >= 5);
    for (path, methods) in found.0 {
        assert!(
            missing_methods(&methods, &contract).is_empty(),
            "{}",
            path.display()
        );
    }
    use syn::visit::Visit;
    let mut control = CodecImplementations(vec![]);
    control.visit_file(&syn::parse_file("impl MediaCodec for MissingOperation { fn with_scratch(&self, scratch: PathBuf) -> Self { todo!() } }").unwrap());
    assert_eq!(missing_methods(&control.0[0].1, &contract), vec!["call"]);
}
// tests.test_fake_contracts.EveryFakeAcceptsTheRealCall.test_no_fake_takes_fewer_positionals_than_the_real_client
#[test]
fn typed_doubles_accept_the_actual_production_argument_arity() {
    let contract = codec_contract();
    let found = implementations();
    assert!(found.0.len() >= 5);
    for (path, methods) in found.0 {
        assert!(
            wrong_arities(&methods, &contract).is_empty(),
            "{}",
            path.display()
        );
    }
    use syn::visit::Visit;
    let mut control = CodecImplementations(vec![]);
    control.visit_file(
        &syn::parse_file("impl MediaCodec for WrongArity { fn call(&self) {} }").unwrap(),
    );
    assert_eq!(wrong_arities(&control.0[0].1, &contract), vec!["call"]);
}

#[cfg(test)]
#[path = "support/test_admission.rs"]
mod test_admission;
