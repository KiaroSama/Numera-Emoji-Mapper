//! Pure inventory text policies. Header parsing, file I/O and canonical map locks are separate.
use std::collections::BTreeMap;

pub fn norm(text: &str) -> String {
    crate::coin_aliases::strip_parentheses_lower(text)
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        .collect()
}

pub fn base_ticker(ticker: &str) -> &str {
    match ticker {
        "avaxc" => return "avax",
        "bttc" => return "btt",
        _ => {}
    }
    for suffix in [
        "mainnet", "erc20", "bep20", "trc20", "polygon", "base", "matic", "avax", "bsc", "arb",
        "ton", "sol", "trx", "eth", "op",
    ] {
        if ticker.ends_with(suffix) && ticker.chars().count() > suffix.len() + 1 {
            return &ticker[..ticker.len() - suffix.len()];
        }
    }
    ticker
}

fn whitespace(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

#[derive(Debug, PartialEq, Eq)]
pub struct Refilled {
    pub text: String,
    pub filled: usize,
    pub total: usize,
}

/// String IDs are the verified map contract; malformed JSON primitive IDs remain unmapped scope.
pub fn refill(mapping: &BTreeMap<String, String>, text: &str) -> Refilled {
    // split(), not lines(): preserve final empty lines, CR and all other Unicode separators.
    let mut lines: Vec<String> = text.split('\n').map(str::to_owned).collect();
    let mut current = None;
    let mut filled = 0;
    let mut total = 0;
    for line in &mut lines {
        let body = line.trim_start_matches(whitespace);
        if let Some(value) = body.strip_prefix("ticker:") {
            let value = value.trim_start_matches(whitespace);
            // The source's .+? can backtrack one whitespace char from \s* into its value.
            if !value.is_empty() || body.len() > "ticker:".len() {
                current = Some(value.trim_end_matches(whitespace).to_lowercase());
                total += 1;
                continue;
            }
        }
        if body.starts_with("premium-id:") {
            if let Some(ticker) = current.take() {
                let id = mapping.get(&ticker).map(String::as_str).unwrap_or("");
                let prefix = &line[..line.len() - body.len()];
                *line = format!("{prefix}premium-id: {id}")
                    .trim_end_matches(whitespace)
                    .to_owned();
                if !id.is_empty() {
                    filled += 1;
                }
            }
        }
    }
    Refilled {
        text: lines.join("\n"),
        filled,
        total,
    }
}
