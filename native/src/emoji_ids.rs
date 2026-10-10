//! Inventory ID extraction; prose mentions are not download requests.
fn decimal(ch: char) -> bool {
    static RANGES: std::sync::OnceLock<Vec<[u32; 2]>> = std::sync::OnceLock::new();
    let ranges = RANGES.get_or_init(|| {
        let doc: serde_json::Value =
            serde_json::from_str(include_str!("../data/python311-decimal.json"))
                .expect("checked Unicode table");
        serde_json::from_value(doc["decimal"].clone()).expect("checked decimal ranges")
    });
    let code = ch as u32;
    let index = ranges.partition_point(|range| range[1] < code);
    ranges.get(index).is_some_and(|range| range[0] <= code)
}
fn digit(ch: char) -> bool {
    static EXTRA: std::sync::OnceLock<Vec<[u32; 2]>> = std::sync::OnceLock::new();
    let ranges = EXTRA.get_or_init(|| {
        let doc: serde_json::Value =
            serde_json::from_str(include_str!("../data/python311-decimal.json"))
                .expect("checked Unicode table");
        serde_json::from_value(doc["digit_extra"].clone()).expect("checked extra digit ranges")
    });
    let code = ch as u32;
    let index = ranges.partition_point(|range| range[1] < code);
    decimal(ch) || ranges.get(index).is_some_and(|range| range[0] <= code)
}
fn whitespace(ch: char) -> bool {
    ch.is_whitespace() || matches!(ch, '\u{1c}'..='\u{1f}')
}
fn id_prefix(raw: &str) -> Option<&str> {
    let mut count = 0;
    let mut end = 0;
    for (i, ch) in raw.char_indices() {
        if !decimal(ch) {
            break;
        }
        count += 1;
        end = i + ch.len_utf8();
    }
    (5..=25).contains(&count).then(|| &raw[..end])
}
fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.split([
        '\n', '\r', '\u{b}', '\u{c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}',
        '\u{2029}',
    ])
}
pub fn entries(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in lines(text) {
        let line = line.trim_start_matches(whitespace);
        let line = line
            .strip_prefix(['-', '*'])
            .unwrap_or(line)
            .trim_start_matches(whitespace);
        let mut prefix = line.chars();
        if !"premium-id".chars().all(|expected| {
            prefix.next().is_some_and(|ch| {
                ch.eq_ignore_ascii_case(&expected)
                    || (expected == 'i' && matches!(ch, '\u{130}' | '\u{131}'))
            })
        }) {
            continue;
        }
        let Some(raw) = prefix
            .as_str()
            .trim_start_matches(whitespace)
            .strip_prefix(':')
        else {
            continue;
        };
        if let Some(id) = id_prefix(raw.trim_start_matches(whitespace)) {
            out.push(id.into());
        }
    }
    if out.is_empty() {
        for line in lines(text) {
            let line = line.trim_matches(whitespace);
            if id_prefix(line) == Some(line) {
                out.push(line.into());
            }
        }
    }
    out
}
pub fn collect(files: &[String], inline: &[String]) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for file in files {
        let text = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
        for id in entries(&text) {
            if seen.insert(id.clone()) {
                out.push(id);
            }
        }
    }
    for id in inline {
        let id = id.trim_matches(whitespace);
        if !id.is_empty() && id.chars().all(digit) && seen.insert(id.into()) {
            out.push(id.into());
        }
    }
    Ok(out)
}
