//! Launcher workflow, separate from process execution and the retained bootstrap.
use crate::{logging, tooling_environment};
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Request {
    Command(Vec<String>),
    Checks,
}

pub(crate) enum Answer {
    Value(String),
    Back,
    Quit,
}

pub(crate) struct Menu<'a, R, W, F> {
    pub root: PathBuf,
    input: &'a mut R,
    output: &'a mut W,
    execute: F,
    color: bool,
    execution_failed: bool,
}

pub(crate) enum Field<'a> {
    Folder(&'a str),
    Required(&'a str),
    Optional(&'a str),
}

impl<R: BufRead, W: Write, F: FnMut(&Request) -> Result<i32, String>> Menu<'_, R, W, F> {
    pub fn line(&mut self, message: &str) -> Result<(), String> {
        writeln!(self.output, "{message}").map_err(|_| "cannot write launcher output".into())
    }

    pub fn title(&mut self, message: &str) -> Result<(), String> {
        self.line("")?;
        let text = if self.color {
            format!("\x1b[38;5;123m{message}\x1b[0m")
        } else {
            message.into()
        };
        self.line(&text)
    }

    pub fn ask(&mut self, label: &str, navigation: bool) -> Result<Answer, String> {
        write!(
            self.output,
            "{label} {{{}}}: ",
            if navigation {
                "back=0, quit=exit"
            } else {
                "quit=exit"
            }
        )
        .and_then(|_| self.output.flush())
        .map_err(|_| "cannot write launcher prompt")?;
        let mut bytes = Vec::new();
        Read::by_ref(&mut *self.input)
            .take(16 * 1024 + 1)
            .read_until(b'\n', &mut bytes)
            .map_err(|_| "cannot read launcher input")?;
        // EOF is cancellation, never the empty default-yes answer.
        if bytes.is_empty() {
            return Ok(Answer::Quit);
        }
        if bytes.len() > 16 * 1024 {
            return Err("launcher input exceeds16KiB".into());
        }
        let value = String::from_utf8(bytes).map_err(|_| "launcher input must be UTF-8")?;
        let value = value.trim_end_matches(['\r', '\n']).to_owned();
        if value.chars().any(|character| character.is_control()) {
            return Err("launcher input cannot contain control characters".into());
        }
        match value.trim().to_ascii_lowercase().as_str() {
            "exit" | "quit" => Ok(Answer::Quit),
            "0" if navigation => Ok(Answer::Back),
            _ => Ok(Answer::Value(value)),
        }
    }

    pub fn fields(&mut self, fields: &[Field<'_>]) -> Result<Option<Vec<String>>, String> {
        self.fields_from(fields, Vec::new())
    }

    pub fn fields_from(
        &mut self,
        fields: &[Field<'_>],
        mut values: Vec<String>,
    ) -> Result<Option<Vec<String>>, String> {
        let mut index = values.len();
        while index < fields.len() {
            let label = match fields[index] {
                Field::Folder(label) | Field::Required(label) | Field::Optional(label) => label,
            };
            match self.ask(label, true)? {
                Answer::Quit => return Err("NAV_QUIT".into()),
                Answer::Back if index == 0 => return Ok(None),
                Answer::Back => {
                    index -= 1;
                    values.truncate(index);
                }
                Answer::Value(value) => {
                    let valid = match fields[index] {
                        Field::Folder(_) => {
                            !value.trim().is_empty() && self.root.join(&value).is_dir()
                        }
                        Field::Required(_) => !value.trim().is_empty(),
                        Field::Optional(_) => true,
                    };
                    if !valid {
                        self.line("[X] Required value or source folder not found.")?;
                        continue;
                    }
                    values.push(value);
                    index += 1;
                }
            }
        }
        Ok(Some(values))
    }

    pub fn command(&mut self, args: Vec<String>, label: &str) -> Result<i32, String> {
        self.request(&Request::Command(args), label)
    }

    pub fn request(&mut self, request: &Request, label: &str) -> Result<i32, String> {
        let code = match (self.execute)(request) {
            Ok(code) => code,
            Err(error) => {
                self.execution_failed = true;
                return Err(error);
            }
        };
        self.line(&match code {
            0 => format!("[OK] {label} finished."),
            3 => format!("[!] {label} partially finished; review the log before retrying."),
            _ => format!("[X] {label} failed (exit {code})."),
        })?;
        Ok(code)
    }

    pub fn show(&mut self) -> Result<(), String> {
        for (title, entries, color) in [
            (
                "Build a single pack",
                &[
                    ("A1", "Build a general emoji pack  (new bot)"),
                    ("A2", "Convert images to 100x100 PNGs only"),
                    ("A3", "Crypto-coin pack rebuild    (coin bot)"),
                ][..],
                "222",
            ),
            (
                "Collection (multi-format, duplicate-proof)",
                &[
                    ("B0", "Collect specific emoji by id (recommended start)"),
                    ("B1", "Collect emoji from existing packs (download)"),
                    ("B2", "Add media from a folder (build from scratch)"),
                    ("B3", "Publish the collection into new packs"),
                    ("B4", "Open web panel to pick & reorder emoji (browser)"),
                    (
                        "B5",
                        "Reorder a live pack to match the panel (report, then apply)",
                    ),
                ][..],
                "123",
            ),
            (
                "Bot",
                &[(
                    "C1",
                    "Run the Numera Emoji Mapper bot (premium-emoji ID extractor)",
                )][..],
                "219",
            ),
            (
                "Maintenance",
                &[
                    ("D1", "Run the project checks (byte-compile + unit tests)"),
                    ("D2", "Status: is everything current? (offline)"),
                    ("D3", "Check / refresh the pack roster (packs/)"),
                    ("D4", "Check / sync the pack archive"),
                ][..],
                "150",
            ),
        ] {
            self.line("")?;
            if self.color {
                self.line(&format!("\x1b[38;5;{color}m{title}\x1b[0m"))?;
            } else {
                self.line(title)?;
            }
            for (key, label) in entries {
                self.line(&format!("  {key} {label}"))?;
            }
        }
        Ok(())
    }
}

/// Runs the original menu without owning a runtime, installing dependencies or guessing process budgets.
/// The executor owns command logs, cancellation and child lifetime; scripted input never bypasses confirmation.
pub fn run<R, W, F>(
    root: &Path,
    input: &mut R,
    output: &mut W,
    color: bool,
    execute: F,
) -> Result<(), String>
where
    R: BufRead,
    W: Write,
    F: FnMut(&Request) -> Result<i32, String>,
{
    let root = tooling_environment::physical_root(root)?;
    let mut menu = Menu {
        root,
        input,
        output,
        execute,
        color,
        execution_failed: false,
    };
    menu.title("Numera Emoji Mapper")?;
    loop {
        menu.show()?;
        let choice = match menu.ask("Select", false)? {
            Answer::Value(value) => value.trim().to_ascii_lowercase(),
            Answer::Back | Answer::Quit => break,
        };
        if matches!(choice.as_str(), "0" | "q") {
            break;
        }
        if let Err(error) = crate::tooling_menu_actions::choose(&mut menu, &choice) {
            if menu.execution_failed {
                return Err(error);
            }
            if error == "NAV_QUIT" {
                break;
            }
            menu.line(&format!(
                "[X] Action failed: {}",
                logging::redact(&error, &[])
            ))?;
        }
    }
    menu.line("[*] Bye.")
}
