//! Shared source-compatible repaint consent; skipping is reversible, ingesting requires an answer.
use crate::logging::RunLog;
use serde_json::Value;
use std::io::{self, IsTerminal, Write};

pub const MODES: [&str; 3] = ["ask", "skip", "keep"];
const PROMPT: &str = "  Ingest them anyway? [y/N] ";
const NO_ANSWER: &str =
    "  nobody answered, so skipping them. Re-run with --repaintable keep to ingest them.";

pub fn is_repaintable(sticker: &Value) -> bool {
    sticker
        .get("needs_repainting")
        .is_some_and(crate::codec_lottie_validate::truthy)
}

fn sample(labels: &[String]) -> String {
    let mut shown = labels
        .iter()
        .take(8)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if labels.len() > 8 {
        shown.push_str(&format!(", ... (+{} more)", labels.len() - 8));
    }
    shown
}

pub fn decide(
    labels: &[String],
    mode: &str,
    terminal: bool,
    prompt: impl FnOnce(&str) -> io::Result<Option<String>>,
    output: &mut impl Write,
) -> io::Result<bool> {
    if !MODES.contains(&mode) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid repaintable choice",
        ));
    }
    if labels.is_empty() {
        return Ok(true);
    }
    writeln!(
        output,
        "WARNING: {} of these emoji are REPAINTABLE: {}\n  Telegram OVERRIDES their colours with the text/accent colour, so what you see in the source pack is not the stored art.\n  In a pack without the flag they show that stored art instead -- sometimes flat black, sometimes full colour, so LOOK before deciding.\n  The flag is set once per set at creation; it cannot be added later.",
        labels.len(),
        sample(labels)
    )?;
    match mode {
        "keep" => {
            writeln!(output, "  --repaintable keep: ingesting them anyway.")?;
            output.flush()?;
            return Ok(true);
        }
        "skip" => {
            writeln!(output, "  --repaintable skip: leaving them out.")?;
            output.flush()?;
            return Ok(false);
        }
        _ => {}
    }
    if terminal {
        write!(output, "{PROMPT}")?;
        output.flush()?;
        match prompt(PROMPT) {
            Ok(Some(answer)) => {
                return Ok(matches!(
                    answer
                        .trim_matches(crate::emoji_ids::whitespace)
                        .to_lowercase()
                        .as_str(),
                    "y" | "yes"
                ));
            }
            Ok(None) => {}
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::UnexpectedEof | io::ErrorKind::Interrupted
                ) => {}
            Err(error) => return Err(error),
        }
        writeln!(output)?;
    }
    writeln!(output, "{NO_ANSWER}")?;
    output.flush()?;
    Ok(false)
}

pub fn run(labels: &[String], mode: &str, log: Option<&RunLog>) -> Result<bool, String> {
    if !labels.is_empty()
        && let Some(log) = log
    {
        log.event(
            "WARNING",
            "collector",
            &format!(
                "{} repaintable emoji in this batch: {}",
                labels.len(),
                sample(labels)
            ),
        );
    }
    decide(
        labels,
        mode,
        io::stdin().is_terminal(),
        |_| {
            let mut answer = String::new();
            io::stdin()
                .read_line(&mut answer)
                .map(|count| (count != 0).then_some(answer))
        },
        &mut io::stderr().lock(),
    )
    .map_err(|error| format!("repaintable confirmation failed: {error}"))
}
