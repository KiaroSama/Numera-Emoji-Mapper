//! Inventory ID extraction; prose mentions are not download requests.
pub(crate) fn decimal(ch: char) -> bool {
    decimal_value(ch).is_some()
}
pub(crate) fn decimal_value(ch: char) -> Option<u32> {
    static RANGES: std::sync::OnceLock<Vec<[u32; 2]>> = std::sync::OnceLock::new();
    let ranges = RANGES.get_or_init(|| {
        let doc: serde_json::Value =
            serde_json::from_str(include_str!("../data/python311-decimal.json"))
                .expect("checked Unicode table");
        serde_json::from_value(doc["decimal"].clone()).expect("checked decimal ranges")
    });
    let code = ch as u32;
    let index = ranges.partition_point(|range| range[1] < code);
    ranges
        .get(index)
        .filter(|range| range[0] <= code)
        .map(|range| (code - range[0]) % 10)
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
pub(crate) fn whitespace(ch: char) -> bool {
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
pub(crate) fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.split([
        '\n', '\r', '\u{b}', '\u{c}', '\u{1c}', '\u{1d}', '\u{1e}', '\u{85}', '\u{2028}',
        '\u{2029}',
    ])
}
fn mention(raw: &str) -> Option<(&str, &str)> {
    let mut prefix = raw.chars();
    if !"premium-id".chars().all(|expected| {
        prefix.next().is_some_and(|ch| {
            ch.eq_ignore_ascii_case(&expected)
                || (expected == 'i' && matches!(ch, '\u{130}' | '\u{131}'))
        })
    }) {
        return None;
    }
    let raw = prefix
        .as_str()
        .trim_start_matches(whitespace)
        .strip_prefix(':')?
        .trim_start_matches(whitespace);
    let id = id_prefix(raw)?;
    Some((id, &raw[id.len()..]))
}

pub fn entries(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in lines(text) {
        let line = line.trim_start_matches(whitespace);
        let line = line
            .strip_prefix(['-', '*'])
            .unwrap_or(line)
            .trim_start_matches(whitespace);
        if let Some((id, _)) = mention(line) {
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

fn occurrences(ids: impl IntoIterator<Item = String>) -> Vec<(String, usize)> {
    let mut positions = std::collections::BTreeMap::<String, usize>::new();
    let mut counts = Vec::<(String, usize)>::new();
    for id in ids {
        if let Some(&index) = positions.get(&id) {
            counts[index].1 += 1;
        } else {
            positions.insert(id.clone(), counts.len());
            counts.push((id, 1));
        }
    }
    counts
}

pub fn within_file_duplicates(text: &str) -> Vec<(String, usize)> {
    occurrences(entries(text))
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .collect()
}

fn duplicate_summary(mut counts: Vec<(String, usize)>, limit: usize) -> String {
    counts.retain(|(_, count)| *count > 1);
    // Python's count-only sort preserves first-seen order for tied counts.
    counts.sort_by(|a, b| b.1.cmp(&a.1));
    counts
        .into_iter()
        .take(limit)
        .map(|(id, count)| format!("{id}x{count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn mention_count(mut text: &str) -> usize {
    let mut count = 0;
    while let Some(start) = text.find(['p', 'P']) {
        text = &text[start..];
        if let Some((_, rest)) = mention(text) {
            count += 1;
            text = rest;
        } else {
            text = &text[1..];
        }
    }
    count
}

pub fn collect_with_report(
    files: &[String],
    inline: &[String],
    mut report: impl FnMut(&str),
) -> Result<Vec<String>, String> {
    let mut all = Vec::new();
    for file in files {
        let text = std::fs::read_to_string(file).map_err(|e| e.to_string())?;
        let found = entries(&text);
        let counts = occurrences(found.iter().cloned());
        let skipped = mention_count(&text).saturating_sub(found.len());
        let name = std::path::Path::new(file)
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("inventory filename is not UTF-8")?;
        let mut message = format!(
            "{name}: {} real entries ({} unique)",
            found.len(),
            counts.len()
        );
        if skipped > 0 {
            message.push_str(&format!(", {skipped} example/prose mention(s) skipped"));
        }
        let duplicates = duplicate_summary(counts, usize::MAX);
        if !duplicates.is_empty() {
            message.push_str(&format!(", within-file duplicate(s): {duplicates}"));
        }
        report(&message);
        all.extend(found);
    }
    for id in inline {
        let id = id.trim_matches(whitespace);
        if !id.is_empty() && id.chars().all(digit) {
            all.push(id.into());
        }
    }
    let total = all.len();
    let counts = occurrences(all);
    let duplicated = counts.iter().filter(|(_, count)| *count > 1).count();
    report(&format!(
        "collected {total} real id occurrences -> {} unique ({duplicated} ids duplicated across files)",
        counts.len()
    ));
    let duplicates = duplicate_summary(counts.clone(), 10);
    if !duplicates.is_empty() {
        report(&format!("duplicate ids (fetched once each): {duplicates}"));
    }
    Ok(counts.into_iter().map(|(id, _)| id).collect())
}

pub fn collect(files: &[String], inline: &[String]) -> Result<Vec<String>, String> {
    collect_with_report(files, inline, |_| {})
}
