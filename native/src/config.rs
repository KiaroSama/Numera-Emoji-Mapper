//! Operator configuration has no identity defaults and is read before mutations.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct Config {
    pub root: PathBuf,
    values: BTreeMap<String, String>,
}
impl Config {
    pub fn load(root: PathBuf) -> Result<Self, String> {
        Self::load_with_environment(root, &std::env::vars().collect())
    }
    pub fn load_with_environment(
        root: PathBuf,
        inherited: &BTreeMap<String, String>,
    ) -> Result<Self, String> {
        let mut values = inherited.clone();
        if values
            .get("NUMERA_EMOJI_MAPPER_NO_DOTENV")
            .map(String::as_str)
            != Some("1")
        {
            let path = root.join(".env");
            match std::fs::read_to_string(path) {
                Ok(text) => {
                    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
                    for line in crate::emoji_ids::lines(text) {
                        let line = line.trim_matches(crate::emoji_ids::whitespace);
                        if line.is_empty() || line.starts_with('#') {
                            continue;
                        }
                        if let Some((key, value)) = line.split_once('=') {
                            values
                                .entry(key.trim_matches(crate::emoji_ids::whitespace).into())
                                .or_insert_with(|| {
                                    value
                                        .trim_matches(crate::emoji_ids::whitespace)
                                        .trim_matches('"')
                                        .trim_matches('\'')
                                        .into()
                                });
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("cannot read configuration: {e}")),
            }
        }
        Ok(Self { root, values })
    }
    pub fn value(&self, key: &str) -> &str {
        self.values
            .get(key)
            .map(|v| v.trim_matches(crate::emoji_ids::whitespace))
            .unwrap_or("")
    }
    pub fn require(&self, key: &str) -> Result<&str, String> {
        let value = self.value(key);
        if value.is_empty() {
            Err(format!(
                "not configured: {key}. Set it in .env; nothing was changed"
            ))
        } else {
            Ok(value)
        }
    }
    pub fn path(&self, raw: &str) -> PathBuf {
        let path = Path::new(raw);
        if path.is_absolute() {
            path.into()
        } else {
            self.root.join(path)
        }
    }
    pub fn logo_bots(&self, strict: bool) -> Result<Vec<String>, String> {
        if !self.values.contains_key("BRAND_LOGO_BOTS") && strict {
            return Err("not configured: BRAND_LOGO_BOTS".into());
        }
        Ok(self
            .value("BRAND_LOGO_BOTS")
            .replace(',', " ")
            .split(crate::emoji_ids::whitespace)
            .filter(|s| !s.is_empty())
            .map(|s| s.trim_start_matches('@').to_lowercase())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect())
    }
    pub fn logo_path(&self, strict: bool) -> Result<Option<PathBuf>, String> {
        let raw = self.value("BRAND_LOGO_PATH");
        if raw.is_empty() {
            if strict {
                self.require("BRAND_LOGO_PATH")?;
            }
            return Ok(None);
        }
        Ok(Some(self.path(raw)))
    }
    pub fn brand_logo(
        &self,
        disabled: bool,
        override_path: Option<&str>,
    ) -> Result<(Vec<String>, Option<PathBuf>), String> {
        if disabled {
            return Ok((vec![], None));
        }
        let bots = self.logo_bots(true)?;
        if bots.is_empty() {
            return Ok((bots, None));
        }
        let path = match override_path.filter(|value| !value.is_empty()) {
            Some(path) => Some(self.path(path)),
            None => self.logo_path(true)?,
        };
        if path.as_ref().is_none_or(|path| !path.is_file()) {
            return Err("brand logo not found; fix BRAND_LOGO_PATH or pass --no-brand-logo; nothing was changed".into());
        }
        Ok((bots, path))
    }
    pub fn logo_keywords(&self) -> Vec<&str> {
        let keywords = self
            .value("BRAND_LOGO_KEYWORDS")
            .split(',')
            .map(|value| value.trim_matches(crate::emoji_ids::whitespace))
            .filter(|value| !value.is_empty())
            .collect::<Vec<_>>();
        if keywords.is_empty() {
            vec!["logo"]
        } else {
            keywords
        }
    }
    pub fn secret_values(&self) -> Vec<String> {
        self.values
            .iter()
            .filter(|(key, _)| {
                ["TOKEN", "SECRET", "PASSWORD", "API_KEY"]
                    .iter()
                    .any(|part| key.contains(part))
            })
            .filter(|(_, value)| !value.is_empty())
            .map(|(_, value)| value.clone())
            .collect()
    }
    pub fn integer(&self, key: &str, default: i64, minimum: i64, maximum: i64) -> i64 {
        let (value, warning) = crate::config_integer::resolve(
            self.values.get(key).map(String::as_str),
            key,
            default,
            Some(minimum),
            Some(maximum),
        );
        if let Some(warning) = warning {
            eprint!("{warning}");
        }
        value
            .parse()
            .expect("integer policy returned a native bound")
    }
}

#[cfg(all(test, not(numera_native_runtime)))]
mod tests {
    use super::*;

    #[test]
    fn original_numeric_configuration_preserves_defaults_unicode_bounds_and_warnings() {
        let root = PathBuf::from("not-read-numeric-configuration");
        for (raw, default, minimum, maximum, expected, warning) in [
            (None, 7, i64::MIN, i64::MAX, 7, None),
            (Some("   "), 7, i64::MIN, i64::MAX, 7, None),
            (Some("42"), 7, i64::MIN, i64::MAX, 42, None),
            (
                Some("not-a-number"),
                7,
                i64::MIN,
                i64::MAX,
                7,
                Some("WARNING: X_TEST_NUM is not a whole number; using 7.\n"),
            ),
            (
                Some("999"),
                1,
                i64::MIN,
                64,
                64,
                Some("WARNING: X_TEST_NUM=999 above 64; using 64.\n"),
            ),
            (
                Some("-5"),
                1,
                0,
                i64::MAX,
                0,
                Some("WARNING: X_TEST_NUM=-5 below 0; using 0.\n"),
            ),
            (Some("\u{1c}+۴_۲\u{1f}"), 7, 0, 64, 42, None),
            (
                Some("-۰۰۹"),
                7,
                0,
                64,
                0,
                Some("WARNING: X_TEST_NUM=-9 below 0; using 0.\n"),
            ),
            (
                Some("9223372036854775808"),
                7,
                i64::MIN,
                i64::MAX,
                i64::MAX,
                Some(
                    "WARNING: X_TEST_NUM=9223372036854775808 above 9223372036854775807; using 9223372036854775807.\n",
                ),
            ),
            (
                Some("-9223372036854775809"),
                7,
                i64::MIN,
                i64::MAX,
                i64::MIN,
                Some(
                    "WARNING: X_TEST_NUM=-9223372036854775809 below -9223372036854775808; using -9223372036854775808.\n",
                ),
            ),
            (
                Some("-9223372036854775808"),
                7,
                i64::MIN,
                i64::MAX,
                i64::MIN,
                None,
            ),
            (Some("-۰"), 7, 0, 64, 0, None),
            (None, 7, 10, 64, 7, None),
        ] {
            let mut environment =
                BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]);
            if let Some(raw) = raw {
                environment.insert("X_TEST_NUM".into(), raw.into());
            }
            let config = Config::load_with_environment(root.clone(), &environment).unwrap();
            assert_eq!(
                config.integer("X_TEST_NUM", default, minimum, maximum),
                expected,
                "{raw:?}"
            );
            let actual = crate::config_integer::resolve(
                raw,
                "X_TEST_NUM",
                default,
                Some(minimum),
                Some(maximum),
            );
            assert_eq!(actual.0, expected.to_string(), "{raw:?}");
            assert_eq!(actual.1.as_deref(), warning, "{raw:?}");
        }
        for raw in ["_1", "1_", "1__0", "0x10", "²", "1\0", "+", "- 1"] {
            let actual = crate::config_integer::resolve(Some(raw), "X_TEST_NUM", 7, None, None);
            assert_eq!(
                actual,
                (
                    "7".into(),
                    Some("WARNING: X_TEST_NUM is not a whole number; using 7.\n".into())
                )
            );
        }
        let within = "0".repeat(4300);
        assert_eq!(
            crate::config_integer::resolve(Some(&within), "X_TEST_NUM", 7, None, None),
            ("0".into(), None)
        );
        let oversized = "0".repeat(4301);
        assert_eq!(
            crate::config_integer::resolve(Some(&oversized), "X_TEST_NUM", 7, None, None).0,
            "7"
        );
        let huge = "9".repeat(4300);
        let clamped = crate::config_integer::resolve(Some(&huge), "X_TEST_NUM", 7, None, Some(64));
        assert_eq!(clamped.0, "64");
        assert_eq!(
            clamped.1,
            Some(format!("WARNING: X_TEST_NUM={huge} above 64; using 64.\n"))
        );
    }

    #[test]
    fn original_dotenv_bom_names_lines_and_environment_precedence_are_preserved() {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let parent =
            crate::tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
        let root = parent.join(format!(
            "dotenv original é {} {}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        let file = root.join(".env");
        let inherited = BTreeMap::new();
        let empty = Config::load_with_environment(root.clone(), &inherited).unwrap();
        assert_eq!(empty.value("FIRST_KEY"), "");
        // Original tests/test_cli_env.py's literal input and assertions.
        std::fs::write(&file, "\u{feff}FIRST_KEY=one\n# note\nSECOND='two'\n").unwrap();
        let original = Config::load_with_environment(root.clone(), &inherited).unwrap();
        assert_eq!(original.value("FIRST_KEY"), "one");
        assert_eq!(original.value("SECOND"), "two");
        assert_eq!(original.values.len(), 2);
        std::fs::write(&file, "\u{feff}\u{feff}FIRST=one\rSECOND=two\u{85}THIRD='three'\u{2028}FOURTH=4\u{2029}FIFTH=5\nSECOND=ignored\n\u{1c}SIXTH = \"six\"\u{1f}\n").unwrap();
        let mut inherited = BTreeMap::from([("THIRD".into(), "environment".into())]);
        let before = inherited.clone();
        let parsed = Config::load_with_environment(root.clone(), &inherited).unwrap();
        assert_eq!(inherited, before);
        assert_eq!(parsed.value("FIRST"), "");
        assert_eq!(parsed.value("\u{feff}FIRST"), "one");
        assert_eq!(parsed.value("SECOND"), "two");
        assert_eq!(parsed.value("THIRD"), "environment");
        assert_eq!(parsed.value("FOURTH"), "4");
        assert_eq!(parsed.value("FIFTH"), "5");
        assert_eq!(parsed.value("SIXTH"), "six");
        std::fs::write(&file, b"FIRST=\xff").unwrap();
        assert!(
            Config::load_with_environment(root.clone(), &inherited)
                .err()
                .unwrap()
                .contains("cannot read configuration")
        );
        assert_eq!(std::fs::read(&file).unwrap(), b"FIRST=\xff");
        inherited.insert("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into());
        let disabled = Config::load_with_environment(root.clone(), &inherited).unwrap();
        assert_eq!(disabled.value("THIRD"), "environment");
        assert_eq!(disabled.values, inherited);
        std::fs::remove_file(&file).unwrap();
        std::fs::create_dir(&file).unwrap();
        assert!(Config::load_with_environment(root.clone(), &before).is_err());
        assert!(Config::load_with_environment(root.clone(), &inherited).is_ok());
        std::fs::remove_dir(file).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
