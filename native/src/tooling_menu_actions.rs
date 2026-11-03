//! Original launcher actions emit native requests; only the caller executes them.
use crate::tooling_menu::{Answer, Field, Menu, Request};
use serde_json::Value;
use std::io::{BufRead, Read, Write};
use std::path::Path;

type Result<T> = std::result::Result<T, String>;

fn argv(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}

fn document(path: &Path) -> Result<Option<Value>> {
    crate::tooling_environment::reject_links(path, "launcher state cannot follow links")?;
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err("cannot inspect launcher state".into()),
    };
    if !metadata.is_file() || metadata.len() > 2 * 1024 * 1024 {
        return Err("launcher state must be a regular file under2MiB".into());
    }
    let file = std::fs::File::open(path).map_err(|_| "cannot read launcher state")?;
    if !file
        .metadata()
        .map_err(|_| "cannot inspect open launcher state")?
        .is_file()
    {
        return Err("open launcher state must be a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(2 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "cannot read launcher state")?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("launcher state grew beyond2MiB".into());
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| "launcher state must be valid UTF-8 JSON".into())
}

fn family(root: &Path, base: &str) -> Result<&'static str> {
    crate::collection_names::valid_base(base)?;
    let Some(state) = document(&root.join(format!("collection/publish_{base}.json")))? else {
        return Ok("new");
    };
    let sets = state
        .get("sets")
        .and_then(Value::as_array)
        .ok_or("ambiguous publisher sets")?;
    if sets.is_empty() {
        return Ok("new");
    }
    let mut mixed = 0;
    for set in sets {
        match set.get("fmt").and_then(Value::as_str) {
            Some("mixed") => mixed += 1,
            Some("static" | "video" | "animated") => {}
            _ => return Err("ambiguous publisher format".into()),
        }
    }
    match mixed {
        0 => Ok("per-format"),
        count if count == sets.len() => Ok("mixed"),
        _ => Err("ambiguous mixed/per-format publisher state".into()),
    }
}

fn published(root: &Path) -> Result<Vec<u64>> {
    let directory = root.join("collection");
    crate::tooling_environment::reject_links(
        &directory,
        "launcher collection cannot follow links",
    )?;
    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err("cannot inspect published pack state".into()),
    };
    let mut packs = std::collections::BTreeSet::new();
    for entry in entries {
        let entry = entry.map_err(|_| "cannot inspect published pack entry")?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !name.starts_with("publish_") || !name.ends_with(".json") {
            continue;
        }
        // The original panel selector skips unreadable files, never inventing pack numbers.
        if let Ok(Some(state)) = document(&entry.path())
            && let Some(sets) = state.get("sets").and_then(Value::as_array)
        {
            packs.extend(sets.iter().filter_map(|set| set.get("index")?.as_u64()));
        }
    }
    Ok(packs.into_iter().collect())
}

impl<R: BufRead, W: Write, F: FnMut(&Request) -> Result<i32>> Menu<'_, R, W, F> {
    fn yes(&mut self, label: &str, default: bool) -> Result<Option<bool>> {
        match self.ask(
            &format!("{label} {}", if default { "[Y/n]" } else { "[y/N]" }),
            true,
        )? {
            Answer::Value(value) => {
                let value = value.trim().to_ascii_lowercase();
                Ok(Some(
                    value == "y" || value == "yes" || value.is_empty() && default,
                ))
            }
            Answer::Back => Ok(None),
            Answer::Quit => Err("NAV_QUIT".into()),
        }
    }

    fn general(&mut self) -> Result<()> {
        self.title("Build a general emoji pack (@YourEmojiBot)")?;
        let fields = [
            Field::Folder("Source image folder (e.g. input/myset)"),
            Field::Required("Pack base name (letters/digits/_), e.g. myset"),
            Field::Required("Pack title, e.g. My Emojis"),
            Field::Optional("Associated standard emoji (default 😀)"),
        ];
        let mut previous = Vec::new();
        let values = loop {
            let Some(values) = self.fields_from(&fields, previous)? else {
                return Ok(());
            };
            match self.yes("Convert + dry-run + upload now?", true)? {
                Some(true) => break values,
                Some(false) => return self.line("[*] Cancelled."),
                None => {
                    previous = values;
                    previous.pop();
                }
            }
        };
        crate::collection_names::valid_base(&values[1])?;
        let build = format!("build/{}", values[1]);
        let emoji = if values[3].trim().is_empty() {
            "😀"
        } else {
            &values[3]
        };
        if self.command(
            argv(&["make-emoji-pngs", "--in", &values[0], "--out", &build]),
            "Conversion",
        )? != 0
        {
            return Ok(());
        }
        let args = argv(&[
            "build-pack",
            "--base",
            &values[1],
            "--title",
            &values[2],
            "--source-dir",
            &build,
            "--token-env",
            "GENERAL_BOT_TOKEN",
            "--emoji",
            emoji,
        ]);
        let mut dry = args.clone();
        dry.push("--dry-run".into());
        if self.command(dry, "Dry-run")? == 0 {
            self.command(args, "Pack build")?;
        }
        Ok(())
    }

    fn convert(&mut self) -> Result<()> {
        self.title("Convert images to 100x100 PNGs")?;
        let Some(values) = self.fields(&[
            Field::Folder("Source image folder"),
            Field::Optional("Output folder (blank = <folder>_emoji)"),
        ])?
        else {
            return Ok(());
        };
        let mut args = argv(&["make-emoji-pngs", "--in", &values[0]]);
        if !values[1].trim().is_empty() {
            args.extend(argv(&["--out", &values[1]]));
        }
        self.command(args, "Conversion")?;
        Ok(())
    }

    fn rebuild(&mut self) -> Result<()> {
        self.title("Crypto-coin pack rebuild (TELEGRAM_BOT_TOKEN)")?;
        let legacy = self.root.join("coins/rebuild_state.json");
        let state = document(&legacy);
        let (question, default) = match state {
            Ok(None) => (
                "Run coin rebuild now? (duplicate-proof: build + map + links)".into(),
                true,
            ),
            state => {
                let count = state
                    .ok()
                    .flatten()
                    .and_then(|value| value["sets"].as_array().map(Vec::len));
                (
                    format!(
                        "This DELETES the {} pack(s) recorded in coins/rebuild_state.json, then rebuilds them. Continue?",
                        count
                            .map(|count| count.to_string())
                            .unwrap_or_else(|| "?".into())
                    ),
                    false,
                )
            }
        };
        if self.yes(&question, default)? == Some(true) {
            self.command(argv(&["coin-rebuild"]), "Coin pack rebuild")?;
        }
        Ok(())
    }

    fn collect(&mut self, ids: bool) -> Result<()> {
        self.title(if ids {
            "Collect specific emoji by id (the GUIDE's recommended start)"
        } else {
            "Collect emoji from existing Telegram packs"
        })?;
        self.line("[*] Blank line to finish; 0 removes the last one.")?;
        let mut values = Vec::<String>::new();
        let token = loop {
            match self.ask(
                if ids {
                    "Emoji id (blank = done)"
                } else {
                    "Pack (blank = done)"
                },
                true,
            )? {
                Answer::Quit => return Err("NAV_QUIT".into()),
                Answer::Back => {
                    if values.pop().is_some() {
                        self.line("[*] Removed last.")?;
                    }
                }
                Answer::Value(value) if value.trim().is_empty() => {
                    if values.is_empty() {
                        self.line(if ids {
                            "[!] No ids entered."
                        } else {
                            "[!] No packs entered."
                        })?;
                        return Ok(());
                    }
                    if ids {
                        break None;
                    }
                    match self.ask("Token env var (default GENERAL_BOT_TOKEN)", true)? {
                        Answer::Quit => return Err("NAV_QUIT".into()),
                        Answer::Back => {}
                        Answer::Value(value) => {
                            break Some(if value.trim().is_empty() {
                                "GENERAL_BOT_TOKEN".into()
                            } else {
                                value
                            });
                        }
                    }
                }
                Answer::Value(value) => values.push(value.trim().into()),
            }
        };
        let mut args = argv(&[if ids { "fetch-emoji-ids" } else { "fetch-pack" }]);
        for value in values {
            if ids {
                args.push("--id".into());
            }
            args.push(value);
        }
        if let Some(token) = token {
            args.extend(["--token-env".into(), token]);
        }
        self.command(args, if ids { "Collect by id" } else { "Collect" })?;
        Ok(())
    }

    fn ingest(&mut self) -> Result<()> {
        self.title("Build emoji from scratch (folder of images/animations/videos)")?;
        let Some(values) = self.fields(&[
            Field::Folder("Source folder"),
            Field::Optional("Associated standard emoji (default 😀)"),
        ])?
        else {
            return Ok(());
        };
        let emoji = if values[1].trim().is_empty() {
            "😀"
        } else {
            &values[1]
        };
        self.command(
            argv(&["add-media", "--in", &values[0], "--emoji", emoji]),
            "Add media",
        )?;
        Ok(())
    }

    fn publish(&mut self) -> Result<()> {
        self.title("Publish the collection into new packs (multi-format)")?;
        let fields = [
            Field::Required("Pack base name (letters/digits only), e.g. mypack"),
            Field::Required("Pack title, e.g. My Collection"),
            Field::Optional("Token env var (default GENERAL_BOT_TOKEN)"),
        ];
        let mut previous = Vec::new();
        let (values, mode) = loop {
            let Some(values) = self.fields_from(&fields, previous)? else {
                return Ok(());
            };
            let recorded = family(&self.root, &values[0]).map_err(|error| {
                format!(
                    "ambiguous publisher state; inspect it before running build-collection: {error}"
                )
            })?;
            let mode = loop {
                let mode = if recorded == "new" {
                    match self.yes("Publish as ONE mixed family (recommended)?", true)? {
                        Some(true) => "mixed",
                        Some(false) => "per-format",
                        None => break None,
                    }
                } else {
                    self.line(&format!("[*] Existing family: continuing it {recorded}."))?;
                    recorded
                };
                match self.yes("Dry-run then upload now?", true)? {
                    Some(true) => break Some(mode),
                    Some(false) => return self.line("[*] Cancelled."),
                    None => {}
                }
            };
            if let Some(mode) = mode {
                break (values, mode);
            }
            previous = values;
            previous.pop();
        };
        let token = if values[2].trim().is_empty() {
            "GENERAL_BOT_TOKEN"
        } else {
            &values[2]
        };
        let mut args = argv(&[
            "build-collection",
            "--base",
            &values[0],
            "--title",
            &values[1],
            "--token-env",
            token,
        ]);
        if mode == "mixed" {
            args.push("--mixed".into());
        }
        let mut dry = args.clone();
        dry.push("--dry-run".into());
        if self.command(dry, "Dry-run")? != 0 {
            return Ok(());
        }
        let mut check = args.clone();
        check.push("--preflight".into());
        if self.command(check, "Preflight")? != 0 {
            return self.line("[X] Preflight refused a file. Nothing was published.");
        }
        self.command(args, "Collection publish")?;
        Ok(())
    }

    fn panel(&mut self) -> Result<()> {
        self.title("Curate panel (pick which emoji go into the pack)")?;
        let packs = published(&self.root)?;
        let mut args = argv(&["panel"]);
        if !packs.is_empty() {
            match self.yes(
                &format!(
                    "Also show the {} pack(s) already published, so they can be rearranged?",
                    packs.len()
                ),
                true,
            )? {
                None => return Ok(()),
                Some(true) => {
                    for pack in packs {
                        args.extend(["--with-pack".into(), pack.to_string()]);
                    }
                }
                Some(false) => {}
            }
        }
        self.command(args, "Web panel")?;
        Ok(())
    }

    fn reorder(&mut self) -> Result<()> {
        self.title("Reorder a live pack to match the panel (ids survive)")?;
        let base = match self.ask("Pack base name", true)? {
            Answer::Value(value) if !value.trim().is_empty() => value,
            Answer::Quit => return Err("NAV_QUIT".into()),
            _ => return Ok(()),
        };
        let pack = match self.ask("Pack number (blank = every pack)", true)? {
            Answer::Value(value) => value,
            Answer::Quit => return Err("NAV_QUIT".into()),
            Answer::Back => return Ok(()),
        };
        let mut args = argv(&["sync-order", "--base", &base]);
        if !pack.trim().is_empty() {
            args.extend(argv(&["--pack", pack.trim()]));
        }
        if self.command(args.clone(), "Reorder report")? != 0 {
            return Ok(());
        }
        if self.yes("Apply these moves to the live pack?", false)? == Some(true) {
            args.push("--apply".into());
            self.command(args, "Reorder")?;
        } else {
            self.line("[*] Nothing was moved.")?;
        }
        Ok(())
    }

    fn maintenance(&mut self, archive: bool) -> Result<()> {
        let command = if archive {
            "pack-archive"
        } else {
            "pack-manifest"
        };
        if self.command(
            argv(&[command, "--check"]),
            if archive {
                "Archive check"
            } else {
                "Roster check"
            },
        )? == 0
        {
            return self.line(if archive {
                "[OK] The archive is current."
            } else {
                "[OK] The roster is current."
            });
        }
        let question = if archive {
            "Sync it now (moves every FULL pack's media to the archive)?"
        } else {
            "Refresh it now (reads the live packs)?"
        };
        if self.yes(question, !archive)? == Some(true) {
            self.command(
                argv(&[command, if archive { "--sync" } else { "--refresh" }]),
                if archive {
                    "Archive sync"
                } else {
                    "Roster refresh"
                },
            )?;
        }
        Ok(())
    }
}

pub(crate) fn choose<R: BufRead, W: Write, F: FnMut(&Request) -> Result<i32>>(
    menu: &mut Menu<'_, R, W, F>,
    choice: &str,
) -> Result<()> {
    match choice {
        "a1" => menu.general(),
        "a2" => menu.convert(),
        "a3" => menu.rebuild(),
        "b0" => menu.collect(true),
        "b1" => menu.collect(false),
        "b2" => menu.ingest(),
        "b3" => menu.publish(),
        "b4" => menu.panel(),
        "b5" => menu.reorder(),
        "c1" => {
            menu.command(argv(&["emoji-bot"]), "Bot")?;
            Ok(())
        }
        "d1" => {
            menu.request(&Request::Checks, "Project checks")?;
            Ok(())
        }
        "d2" => {
            menu.command(argv(&["status"]), "Status")?;
            Ok(())
        }
        "d3" => menu.maintenance(false),
        "d4" => menu.maintenance(true),
        _ => menu.line(&format!(
            "[!] Unknown option: {choice} (use e.g. A1, B3, C1, D1, or exit)"
        )),
    }
}
