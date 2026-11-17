//! Additional Config boundaries beyond the eight separately mapped originals.
use crate::root::Root;
use numera_runtime::config::Config;
use std::{collections::BTreeMap, path::Path};
fn config(root: &Path, pairs: &[(&str, &str)]) -> Config {
    let mut environment = BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]);
    environment.extend(pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())));
    Config::load_with_environment(root.into(), &environment).unwrap()
}
// Native supplemental: native/tests/operator_config.rs::original_operator_settings_preserve_unknown_empty_logo_paths_and_keyword_defaults
#[test]
fn operator_unicode_whitespace_strict_paths_and_override_ownership() {
    let root = Root::new("operator settings café");
    let c = config(
        &root.0,
        &[
            (
                "BRAND_LOGO_BOTS",
                "\u{1c}@@YourEmojiBot, @YOURemojiBOT\u{1f}other_bot\u{1e}",
            ),
            (
                "BRAND_LOGO_KEYWORDS",
                "\u{1c}yourbrand\u{1f}, ,\u{1e}logo\u{1c}",
            ),
            ("COIN_PACK_BASE", "\u{1c} \u{1f}"),
        ],
    );
    assert!(c.require("COIN_PACK_BASE").is_err());
    assert_eq!(
        c.logo_bots(true).unwrap(),
        vec!["other_bot", "youremojibot"]
    );
    assert_eq!(c.logo_keywords(), vec!["yourbrand", "logo"]);
    assert_eq!(
        config(&root.0, &[("BRAND_LOGO_KEYWORDS", ", \u{1c},")]).logo_keywords(),
        vec!["logo"]
    );
    assert_eq!(
        config(&root.0, &[("BRAND_LOGO_PATH", "private/brand-logo.png")])
            .logo_path(true)
            .unwrap(),
        Some(root.0.join("private/brand-logo.png"))
    );
    let logo = root.0.join("logo.png");
    std::fs::write(&logo, b"png").unwrap();
    let configured = config(
        &root.0,
        &[
            ("BRAND_LOGO_BOTS", "YourEmojiBot"),
            ("BRAND_LOGO_PATH", "logo.png"),
        ],
    );
    let replacement = root.0.join("override.png");
    std::fs::write(&replacement, b"override").unwrap();
    assert_eq!(
        configured
            .brand_logo(false, Some("override.png"))
            .unwrap()
            .1,
        Some(replacement)
    );
    let missing = config(
        &root.0,
        &[
            ("BRAND_LOGO_BOTS", "YourEmojiBot"),
            ("BRAND_LOGO_PATH", "private/does-not-exist.png"),
        ],
    );
    assert_eq!(
        missing
            .brand_logo(true, Some("missing-override.png"))
            .unwrap(),
        (vec![], None)
    );
    assert_eq!(std::fs::read(logo).unwrap(), b"png");
}
