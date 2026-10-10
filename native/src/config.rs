//! Operator configuration has no identity defaults and is read before mutations.
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub struct Config {
    pub root: PathBuf,
    values: BTreeMap<String, String>,
}
impl Config {
    pub fn load(root: PathBuf) -> Result<Self, String> {
        let mut values = std::env::vars().collect::<BTreeMap<_, _>>();
        if values
            .get("NUMERA_EMOJI_MAPPER_NO_DOTENV")
            .map(String::as_str)
            != Some("1")
        {
            let path = root.join(".env");
            match std::fs::read_to_string(path) {
                Ok(text) => {
                    for line in text.trim_start_matches('\u{feff}').lines() {
                        let line = line.trim();
                        if line.is_empty() || line.starts_with('#') {
                            continue;
                        }
                        if let Some((key, value)) = line.split_once('=') {
                            values.entry(key.trim().into()).or_insert_with(|| {
                                value.trim().trim_matches('"').trim_matches('\'').into()
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
        self.values.get(key).map(|v| v.trim()).unwrap_or("")
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
            .split_whitespace()
            .map(|s| s.trim_start_matches('@').to_lowercase())
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
        let raw = self.value(key);
        if raw.is_empty() {
            return default;
        }
        match raw.parse::<i64>() {
            Ok(n) => n.clamp(minimum, maximum),
            Err(_) => {
                eprintln!("WARNING: {key} is not a whole number; using {default}.");
                default
            }
        }
    }
}
