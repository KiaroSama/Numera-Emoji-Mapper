//! Source block grammar, including unanchored headers and greedy header-token backtracking.
use std::collections::BTreeSet;

fn whitespace(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

fn block(text: &str) -> Option<(usize, String, String)> {
    let after = text.strip_prefix("##")?;
    let start = text.len() - after.trim_start_matches(whitespace).len();
    let header = &text[start..];
    let token_end = header
        .char_indices()
        .find(|(_, c)| whitespace(*c))
        .map(|(i, _)| i)
        .unwrap_or(header.len());
    // The first \S+ is greedy, so search its possible ends in reverse.
    let ends = header[..token_end]
        .char_indices()
        .map(|(index, c)| index + c.len_utf8())
        .collect::<Vec<_>>();
    for end in ends.into_iter().rev() {
        let remaining = header[end..].trim_start_matches(whitespace);
        let separators = remaining
            .char_indices()
            .take_while(|(_, c)| matches!(c, '-' | '—'))
            .map(|(index, c)| index + c.len_utf8())
            .last()
            .unwrap_or(0);
        if separators == 0 {
            continue;
        }
        let rest = &remaining[separators..];
        let stripped = rest.trim_start_matches(whitespace);
        // \s* may backtrack to leave one whitespace character for .+?.
        let mut starts = vec![rest.len() - stripped.len()];
        starts.extend(
            rest[..starts[0]]
                .char_indices()
                .map(|(index, _)| index)
                .rev(),
        );
        for offset in starts {
            let name_text = &rest[offset..];
            let newline = name_text.find('\n')?;
            if newline == 0 {
                continue;
            }
            let suffix = name_text[newline + 1..].trim_start_matches(whitespace);
            let Some(value) = suffix.strip_prefix("ticker:") else {
                continue;
            };
            let value = value.trim_start_matches(whitespace);
            let ticker_end = value
                .char_indices()
                .find(|(_, c)| whitespace(*c))
                .map(|(index, _)| index)
                .unwrap_or(value.len());
            if ticker_end == 0 {
                continue;
            }
            let ticker = value[..ticker_end].to_lowercase();
            let consumed = text.len() - value[ticker_end..].len();
            return Some((
                consumed,
                name_text[..newline].trim_matches(whitespace).to_owned(),
                ticker,
            ));
        }
    }
    None
}

pub fn missing(text: &str, have: &BTreeSet<String>) -> Vec<(String, String)> {
    // read_text() uses universal newlines before the source regex sees the text.
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut remaining = text.as_str();
    let mut output = Vec::new();
    while let Some(start) = remaining.find("##") {
        if let Some((consumed, name, ticker)) = block(&remaining[start..]) {
            if !have.contains(&ticker) {
                output.push((name, ticker));
            }
            remaining = &remaining[start + consumed..];
        } else {
            remaining = &remaining[start + 2..];
        }
    }
    output
}
