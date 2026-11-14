//! Native converter-watchdog arguments and the shared process/deadline owner.
use crate::logging::RunLog;
use std::path::{Path, PathBuf};

#[derive(Debug, PartialEq)]
pub enum Arguments {
    Convert {
        source: PathBuf,
        output: PathBuf,
        stdout: PathBuf,
        stderr: PathBuf,
    },
    Exit(i32),
}

pub fn arguments(args: &[String]) -> Result<Arguments, String> {
    let names = ["--in", "--out", "--stdout", "--stderr"];
    let mut values: [Option<PathBuf>; 4] = Default::default();
    let mut index = 0;
    while index < args.len() {
        let raw = &args[index];
        if raw == "--help" || raw == "-h" {
            println!(
                "Usage: numera-emoji native-convert-owner --in DIR --out DIR --stdout FILE --stderr FILE\nOwn one conversion attempt, its progress deadline and every descendant."
            );
            return Ok(Arguments::Exit(0));
        }
        let (name, attached) = raw
            .split_once('=')
            .map_or((raw.as_str(), None), |(name, value)| (name, Some(value)));
        let matches = names
            .iter()
            .enumerate()
            .filter(|(_, candidate)| candidate.starts_with(name))
            .collect::<Vec<_>>();
        let option = if let Some(exact) = names.iter().position(|candidate| *candidate == name) {
            exact
        } else if name.starts_with("--") && matches.len() == 1 {
            matches[0].0
        } else {
            return Err(format!("unknown or ambiguous conversion option: {name}"));
        };
        index += 1;
        let value = match attached {
            Some(value) => value,
            None => {
                let value = args
                    .get(index)
                    .filter(|value| !value.starts_with('-'))
                    .ok_or_else(|| format!("{} requires a path", names[option]))?;
                index += 1;
                value
            }
        };
        values[option] = Some(PathBuf::from(value));
    }
    let [source, output, stdout, stderr] = values;
    Ok(Arguments::Convert {
        source: source.ok_or("--in is required")?,
        output: output.ok_or("--out is required")?,
        stdout: stdout.ok_or("--stdout is required")?,
        stderr: stderr.ok_or("--stderr is required")?,
    })
}

pub fn run(root: &Path, args: &[String], log: &RunLog) -> i32 {
    let result = (|| -> Result<i32, String> {
        match arguments(args)? {
            Arguments::Exit(code) => Ok(code),
            Arguments::Convert {
                source,
                output,
                stdout,
                stderr,
            } => {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|_| "Cannot initialize converter process monitor")?;
                runtime.block_on(crate::tooling_convert::run(
                    root, &source, &output, &stdout, &stderr, log,
                ))
            }
        }
    })();
    match result {
        Ok(code) => code,
        Err(error) => {
            log.event("ERROR", "native-convert-owner", &error);
            2
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_watchdog_paths_are_required_and_abbreviations_are_unambiguous() {
        assert!(arguments(&[]).is_err());
        assert!(arguments(&["--st".into(), "ambiguous".into()]).is_err());
        let args = [
            "--in=source café",
            "--out",
            "output space",
            "--stdou",
            "stdout.txt",
            "--stde=stderr.txt",
        ]
        .map(String::from);
        assert_eq!(
            arguments(&args).unwrap(),
            Arguments::Convert {
                source: "source café".into(),
                output: "output space".into(),
                stdout: "stdout.txt".into(),
                stderr: "stderr.txt".into(),
            }
        );
    }
}
