//! Private configuration and cursor methods retain original IDs without public test APIs.
use crate::{config::Config, logging::RunLog};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Root(PathBuf);
impl Root {
    fn new() -> Self {
        loop {
            let p = std::env::temp_dir().join(format!(
                "numera-baseline-bot-unit-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            match std::fs::create_dir(&p) {
                Ok(()) => return Self(p),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => panic!("{e}"),
            }
        }
    }
    fn config(&self, values: &[(&str, &str)]) -> Config {
        let mut env = BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]);
        env.extend(values.iter().map(|(k, v)| ((*k).into(), (*v).into())));
        Config::load_with_environment(self.0.clone(), &env).unwrap()
    }
    fn log(&self) -> RunLog {
        RunLog::open(&self.0, "bot-unit", vec![])
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
// tests.test_emoji_bot.TestAccessControl.test_allowlist_defaults_to_the_owner
#[test]
fn test_allowlist_defaults_to_the_owner() {
    let t = Root::new();
    assert_eq!(
        super::allowed_users(
            &t.config(&[("PACK_OWNER_USER_ID", "42"), ("BOT_ALLOWED_USER_IDS", "")]),
            &t.log()
        )
        .unwrap(),
        BTreeSet::from([42])
    );
}
// tests.test_emoji_bot.TestAccessControl.test_allowlist_parses_separators_and_ignores_junk
#[test]
fn test_allowlist_parses_separators_and_ignores_junk() {
    let t = Root::new();
    assert_eq!(
        super::allowed_users(
            &t.config(&[
                ("PACK_OWNER_USER_ID", "42"),
                ("BOT_ALLOWED_USER_IDS", "1, 2 ;3, oops,")
            ]),
            &t.log()
        )
        .unwrap(),
        BTreeSet::from([1, 2, 3])
    );
}
// tests.test_emoji_bot.TestAccessControl.test_empty_configuration_authorizes_nobody
#[test]
fn test_empty_configuration_authorizes_nobody() {
    let t = Root::new();
    assert!(
        super::allowed_users(
            &t.config(&[("PACK_OWNER_USER_ID", "0"), ("BOT_ALLOWED_USER_IDS", "")]),
            &t.log()
        )
        .is_err(),
        "missing owner must refuse, never authorize anybody"
    );
}
// tests.test_emoji_bot.TestAccessControl.test_channel_list_parses_negative_ids_and_skips_junk
#[test]
fn test_channel_list_parses_negative_ids_and_skips_junk() {
    let t = Root::new();
    assert_eq!(
        super::allowlist("-1001234567890; abc,,-5", true, &t.log()),
        BTreeSet::from([-1001234567890, -5])
    );
}
// tests.test_emoji_bot.TestOffsetPersistence.test_missing_file_starts_at_zero
#[test]
fn test_missing_file_starts_at_zero() {
    let t = Root::new();
    assert_eq!(super::load_offset(&t.0.join("state_emoji_bot.json")), 0);
}
// tests.test_emoji_bot.TestOffsetPersistence.test_roundtrip
#[test]
fn test_roundtrip() {
    let t = Root::new();
    let path = t.0.join("state_emoji_bot.json");
    super::save_offset(&path, 4242).unwrap();
    assert_eq!(super::load_offset(&path), 4242);
}
// tests.test_emoji_bot.TestOffsetPersistence.test_corrupt_file_falls_back_to_zero
#[test]
fn test_corrupt_file_falls_back_to_zero() {
    let t = Root::new();
    let path = t.0.join("state_emoji_bot.json");
    std::fs::write(&path, "{not json").unwrap();
    assert_eq!(super::load_offset(&path), 0);
}
