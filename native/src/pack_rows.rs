//! One shared Markdown renderer for pack lists, preserving every keyword.
use serde_json::Value;
fn cell(value: &str) -> String {
    let clean = value
        .replace(
            [
                '\r', '\n', '\u{000b}', '\u{000c}', '\u{0085}', '\u{2028}', '\u{2029}',
            ],
            " ",
        )
        .replace('|', "\\|");
    let clean = clean.trim();
    if clean.is_empty() {
        "—".into()
    } else {
        clean.into()
    }
}
pub fn markdown(rows: &[Value], columns: &[(&str, &str)]) -> String {
    aligned(
        rows,
        &columns
            .iter()
            .map(|(key, title)| (*key, *title, false))
            .collect::<Vec<_>>(),
    )
}

pub fn aligned(rows: &[Value], columns: &[(&str, &str, bool)]) -> String {
    let head = format!(
        "| {} |",
        columns
            .iter()
            .map(|(_, name, _)| *name)
            .collect::<Vec<_>>()
            .join(" | ")
    );
    let rule = format!(
        "|{}|",
        columns
            .iter()
            .map(|(_, name, right)| if *right {
                format!("{}:", "-".repeat(name.chars().count().max(1)))
            } else {
                "-".repeat(name.chars().count() + 2)
            })
            .collect::<Vec<_>>()
            .join("|")
    );
    let mut lines = vec![head, rule];
    for row in rows {
        let values = columns
            .iter()
            .map(|(key, _, _)| match row.get(*key) {
                None | Some(Value::Null) => cell(""),
                Some(Value::String(value)) => cell(value),
                Some(value) => cell(&value.to_string()),
            })
            .collect::<Vec<_>>();
        lines.push(format!("| {} |", values.join(" | ")));
    }
    lines.join("\n")
}
pub fn keywords(item: &Value) -> String {
    item["keywords"]
        .as_array()
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .filter(|v| !v.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}
