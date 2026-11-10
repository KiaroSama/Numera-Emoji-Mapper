//! Untrusted, byte-bound observations of pinned standard libtest sidecars; never CI authentication.
#[cfg(test)]
use serde_json::Value;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
#[path = "tooling_test_observation_manifest.rs"]
pub(crate) mod manifest;
#[path = "tooling_test_paths.rs"]
pub(crate) mod paths;
use manifest::{Admission, SMALL_LIMIT, read_file, relative, sync_existing};

fn refusal(message: &str) -> String {
    format!(
        "test observation refused: {message}; correct the manifest/options and rerun; retain the observation directory"
    )
}
fn text(value: &str) -> bool {
    !value.is_empty() && !value.chars().any(char::is_control)
}
pub(crate) fn validate_args(args: &[String]) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    let mut index = 0;
    while index < args.len() {
        let arg = &args[index];
        index += 1;
        if !text(arg) {
            return Err(refusal("empty/control-bearing libtest argument"));
        }
        if !arg.starts_with('-') {
            continue;
        }
        let (key, attached) = arg
            .split_once('=')
            .map_or((arg.as_str(), None), |(k, v)| (k, Some(v)));
        let (group, choices): (&str, &[&str]) = match key {
            "--skip" => ("skip", &[]),
            "--test-threads" => ("threads", &["1", "2"]),
            "--format" => ("format", &["pretty", "terse"]),
            "--color" => ("color", &["auto", "always", "never"]),
            "--exact" | "--test" | "--show-output" => (key, &[]),
            "--ignored" | "--include-ignored" => ("ignored", &[]),
            "--nocapture" | "--no-capture" => ("capture", &[]),
            "-q" | "--quiet" => ("format", &[]),
            _ => {
                return Err(refusal(
                    "unsupported observation option; use the ordinary runner for this libtest mode",
                ));
            }
        };
        if group != "skip" && !seen.insert(group) {
            return Err(refusal("duplicate/conflicting observation options"));
        }
        if matches!(group, "skip" | "threads" | "color") || key == "--format" {
            let value = if let Some(value) = attached {
                value
            } else {
                let value = args
                    .get(index)
                    .ok_or_else(|| refusal("libtest option requires its value"))?;
                index += 1;
                value.as_str()
            };
            if !text(value)
                || value.starts_with('-')
                || (!choices.is_empty() && !choices.contains(&value))
            {
                return Err(refusal(
                    "unsupported libtest option value; use workers1..2 and stable pretty/terse output",
                ));
            }
        } else if attached.is_some() {
            return Err(refusal("libtest flag does not take a value"));
        }
    }
    Ok(())
}
fn lines(value: &str) -> Result<Vec<&str>, String> {
    if value.is_empty() || !value.ends_with('\n') {
        return Err(refusal("empty or truncated libtest sidecar"));
    }
    value
        .split_terminator('\n')
        .map(|line| {
            let line = line.strip_suffix('\r').unwrap_or(line);
            if !text(line) {
                return Err(refusal("empty/control-bearing libtest record"));
            }
            Ok(line)
        })
        .collect()
}
pub(crate) fn listing(value: &str) -> Result<BTreeSet<String>, String> {
    let mut names = BTreeSet::new();
    for line in lines(value)? {
        let name = line
            .strip_prefix("test ")
            .filter(|n| text(n) && !n.chars().any(char::is_whitespace))
            .ok_or_else(|| refusal("expected standard libtest test-list record"))?;
        if !names.insert(name.into()) {
            return Err(refusal("duplicate compiled test name"));
        }
    }
    Ok(names)
}
pub(crate) fn results(
    value: &str,
    names: &BTreeSet<String>,
) -> Result<BTreeMap<String, String>, String> {
    let mut records = BTreeMap::new();
    for line in lines(value)? {
        let (status, name) = line
            .rsplit_once(' ')
            .ok_or_else(|| refusal("unrecognized libtest result record"))?;
        let kind = match status {
            "ok" | "ignored" | "failed" => status,
            s if s.strip_prefix("ignored: ").is_some_and(text) => "ignored",
            s if s.strip_prefix("failed: ").is_some_and(text) => "failed",
            "failed (time limit exceeded)" => "failed",
            _ => {
                return Err(refusal(
                    "unsupported libtest status/reason; no result inferred from stdout",
                ));
            }
        };
        if !names.contains(name) || records.insert(name.into(), kind.into()).is_some() {
            return Err(refusal("duplicate or unlisted libtest result"));
        }
    }
    if records.len() != names.len() {
        return Err(refusal(
            "partial libtest results; exit status is not case evidence",
        ));
    }
    Ok(records)
}

pub(crate) struct Observation {
    root: PathBuf,
    store: PathBuf,
    admission: Admission,
    args: Vec<String>,
    names: Option<BTreeSet<String>>,
    listing_hash: Option<String>,
    listing_path: String,
    results_path: String,
    cwd: Option<String>,
}
fn working_directory(root: &Path) -> Result<Option<String>, String> {
    let cwd = crate::tooling_environment::physical_root(
        &std::env::current_dir().map_err(|_| refusal("cannot inspect current directory"))?,
    )?;
    if cwd == root {
        Ok(Some(".".into()))
    } else if cwd.starts_with(root) {
        relative(root, &cwd).map(Some)
    } else {
        // Pure admission fixtures use independent roots; this is not a custody assertion.
        Ok(None)
    }
}
impl Observation {
    pub(crate) fn begin(
        root: &Path,
        store: &Path,
        manifest: &Path,
        executable: &Path,
        args: &[String],
    ) -> Result<Self, String> {
        validate_args(args)?;
        let root = crate::tooling_environment::physical_root(root)?;
        let store = crate::tooling_environment::physical_root(store)?;
        let parent =
            crate::tooling_environment::confined_directory(&root, "logs/test-observations", false)?;
        if store.parent() != Some(parent.as_path())
            || std::fs::read_dir(&store)
                .map_err(|_| refusal("cannot inspect store"))?
                .next()
                .is_some()
        {
            return Err(refusal(
                "caller must supply an empty exclusive logs/test-observations child directory",
            ));
        }
        let admission = Admission::new(&root, manifest, executable)?;
        let as_text = |path: PathBuf| {
            path.to_str()
                .filter(|s| text(s))
                .map(str::to_owned)
                .ok_or_else(|| refusal("store path requires control-free UTF-8"))
        };
        let listing_path = as_text(store.join("listing.log"))?;
        let results_path = as_text(store.join("results.log"))?;
        for path in [&listing_path, &results_path] {
            let mut options = std::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let file = options
                .open(path)
                .map_err(|_| refusal("cannot reserve unique libtest sidecar"))?;
            file.sync_all()
                .map_err(|_| refusal("cannot sync reserved sidecar"))?;
        }
        crate::atomic::write_bytes(&store.join("manifest.json"), &admission.bytes)
            .map_err(|_| refusal("cannot preserve admitted manifest"))?;
        let cwd = working_directory(&root)?;
        Ok(Self {
            root,
            store,
            admission,
            args: args.to_vec(),
            names: None,
            listing_hash: None,
            listing_path,
            results_path,
            cwd,
        })
    }
    fn unchanged(&self) -> Result<(), String> {
        crate::tooling_environment::physical_root(&self.store)?;
        let mut names = BTreeSet::new();
        for entry in std::fs::read_dir(&self.store)
            .map_err(|_| refusal("cannot inspect observation store"))?
        {
            let entry = entry.map_err(|_| refusal("cannot enumerate observation store"))?;
            names.insert(
                entry
                    .file_name()
                    .into_string()
                    .map_err(|_| refusal("unknown observation store member"))?,
            );
        }
        if names
            != BTreeSet::from([
                "listing.log".into(),
                "results.log".into(),
                "manifest.json".into(),
            ])
        {
            return Err(refusal("observation store has unexpected members"));
        }
        if working_directory(&self.root)? != self.cwd {
            return Err(refusal("working directory changed during observation"));
        }
        self.admission.recheck()?;
        if read_file(&self.store.join("manifest.json"), SMALL_LIMIT, false)?.0
            != self.admission.hash
        {
            return Err(refusal("preserved manifest changed during observation"));
        }
        Ok(())
    }
    pub(crate) fn list_args(&self) -> Vec<String> {
        let mut args = self.args.clone();
        args.extend([
            "--list".into(),
            "--logfile".into(),
            self.listing_path.clone(),
        ]);
        args
    }
    pub(crate) fn run_args(&mut self, list_code: i32) -> Result<Vec<String>, String> {
        if list_code != 0 || self.names.is_some() {
            return Err(refusal("compiled listing failed or was already consumed"));
        }
        self.unchanged()?;
        if read_file(Path::new(&self.results_path), SMALL_LIMIT, false)?.2 != 0 {
            return Err(refusal("result sidecar was populated before execution"));
        }
        let (hash, bytes, _) = read_file(Path::new(&self.listing_path), SMALL_LIMIT, true)?;
        self.names = Some(listing(
            std::str::from_utf8(&bytes).map_err(|_| refusal("listing is not UTF-8"))?,
        )?);
        self.listing_hash = Some(hash);
        let mut args = self.args.clone();
        args.extend(["--logfile".into(), self.results_path.clone()]);
        Ok(args)
    }
    /// Caller must finish the existing descendant owner before reading or publishing observations.
    pub(crate) fn finish(self, run_code: i32) -> Result<PathBuf, String> {
        let names = self
            .names
            .as_ref()
            .ok_or_else(|| refusal("no admitted compiled listing"))?;
        self.unchanged()?;
        let (listing_hash, _, _) = read_file(Path::new(&self.listing_path), SMALL_LIMIT, false)?;
        if Some(listing_hash) != self.listing_hash {
            return Err(refusal("compiled listing changed during execution"));
        }
        let (result_hash, result_bytes, _) =
            read_file(Path::new(&self.results_path), SMALL_LIMIT, true)?;
        let rows = results(
            std::str::from_utf8(&result_bytes).map_err(|_| refusal("results are not UTF-8"))?,
            names,
        )?;
        let passed = rows.values().any(|s| s == "ok");
        let failed = rows.values().any(|s| s == "failed");
        if run_code == 0 && (!passed || failed) {
            return Err(refusal(
                "successful exit lacks passed cases or conflicts with failed cases",
            ));
        }
        let dispositions = rows
            .iter()
            .map(|(name, status)| format!("{status} {name}\n"))
            .collect::<String>();
        let mut report = self.admission.report();
        report.as_object_mut().unwrap().extend(json!({"schema":1,"result":"observed","trust":"untrusted","release_ready":false,"completed":run_code==0 && !failed,"exit_code":run_code,
            "platform":format!("{}-{}",std::env::consts::OS,std::env::consts::ARCH),"argv":self.args,
            "runtime":"caller must supply authenticated toolchain/external-runtime identity",
            "closure_sha256":crate::codec_raster::digest_hex(&serde_json::to_vec(&self.admission.value["files"]).map_err(|_|refusal("cannot serialize closure"))?),
            "dispositions_sha256":crate::codec_raster::digest_hex(dispositions.as_bytes()),
            "working_directory":self.cwd,"working_directory_project_contained":self.cwd.is_some(),
            "listing_sha256":self.listing_hash,"results_sha256":result_hash,"listed":names,
            "cases":rows.iter().map(|(name,status)|json!({"name":name,"status":status})).collect::<Vec<_>>(),
            "limits":{"closure_complete":"caller claim only","source_discovery":"not established","ci_provenance":"not authenticated","concurrent_mutation":"before/after checks only"}}).as_object().unwrap().clone());
        let bytes = serde_json::to_vec_pretty(&report)
            .map_err(|_| refusal("cannot serialize observation"))?;
        if bytes.len() as u64 > SMALL_LIMIT {
            return Err(refusal("observation report exceeds8MiB"));
        }
        // Write the report last: its presence must not precede durable supporting sidecars.
        sync_existing(Path::new(&self.listing_path))?;
        sync_existing(Path::new(&self.results_path))?;
        crate::atomic::write_bytes(
            &self.store.join("dispositions.log"),
            dispositions.as_bytes(),
        )
        .map_err(|_| refusal("cannot preserve public result dispositions"))?;
        let path = self.store.join("observation.json");
        crate::atomic::write_bytes(&path, &bytes)
            .map_err(|_| refusal("cannot durably write observation report"))?;
        Ok(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upstream_sidecars_keep_actual_dispositions_and_refuse_ambiguous_output() {
        let names = listing("test module::café\ntest module::skip\ntest module::bad\n").unwrap();
        let rows = results("ok module::café\nignored: unavailable platform module::skip\nfailed: test did not panic as expected module::bad\n", &names).unwrap();
        assert_eq!(rows["module::café"], "ok");
        assert_eq!(rows["module::skip"], "ignored");
        assert_eq!(rows["module::bad"], "failed");
        assert_eq!(listing("test café\r\n").unwrap().len(), 1);
        for text in [
            "",
            "test x",
            "test x\ntest x\n",
            "benchmark x\n",
            "test x y\n",
            "test x\n\n",
        ] {
            assert!(listing(text).is_err(), "{text:?}");
        }
        let one = listing("test x\n").unwrap();
        for text in [
            "",
            "ok x",
            "ok x\nok x\n",
            "ok other\n",
            "test x ... ok\n",
            "ignored: reason\ncontinued x\n",
            "ok x\nignored y\n",
        ] {
            assert!(results(text, &one).is_err(), "{text:?}");
        }
        assert_eq!(results("ignored x\n", &one).unwrap()["x"], "ignored");
        assert_eq!(
            results("failed (time limit exceeded) x\n", &one).unwrap()["x"],
            "failed"
        );
    }
    #[test]
    fn observation_admits_only_explicit_finite_libtest_arguments_without_rewriting() {
        let args = [
            "filter",
            "--exact",
            "--skip=other",
            "--skip",
            "third",
            "--ignored",
            "--test-threads=2",
            "--format",
            "terse",
            "--nocapture",
            "--color=never",
        ]
        .map(str::to_owned)
        .to_vec();
        assert!(validate_args(&args).is_ok());
        for values in [
            vec!["--list"],
            vec!["--help"],
            vec!["--bench"],
            vec!["--logfile=x"],
            vec!["-Zunstable-options"],
            vec!["--show-stats"],
            vec!["--report-time"],
            vec!["--shuffle"],
            vec!["--format=json"],
            vec!["--test-threads=3"],
            vec!["--ignored", "--include-ignored"],
            vec!["--exact", "--exact"],
            vec!["--skip"],
            vec!["--", "x"],
            vec!["bad\nfilter"],
        ] {
            assert!(
                validate_args(&values.into_iter().map(str::to_owned).collect::<Vec<_>>()).is_err()
            );
        }
    }
    #[test]
    fn synthetic_transcript_is_byte_bound_but_never_becomes_authenticated_evidence() {
        let (root, store, manifest, binary) = manifest::sample();
        let mut observation = Observation::begin(&root.0, &store, &manifest, &binary, &[]).unwrap();
        assert!(observation.list_args().contains(&"--list".into()));
        std::fs::write(store.join("listing.log"), b"test case\n").unwrap();
        assert_eq!(observation.run_args(0).unwrap()[0], "--logfile");
        assert!(observation.run_args(0).is_err());
        std::fs::write(store.join("results.log"), b"ok case\n").unwrap();
        let report = observation.finish(0).unwrap();
        let report: Value = serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
        assert_eq!(report["trust"], "untrusted");
        assert_eq!(report["result"], "observed");
        assert_eq!(report["release_ready"], false);
        assert_eq!(report["completed"], true);
        assert_eq!(report["cases"][0]["status"], "ok");
        assert_eq!(
            std::fs::read(store.join("listing.log")).unwrap(),
            b"test case\n"
        );
        assert_eq!(
            std::fs::read(store.join("results.log")).unwrap(),
            b"ok case\n"
        );
        assert_eq!(
            std::fs::read(store.join("dispositions.log")).unwrap(),
            b"ok case\n"
        );
        assert_eq!(
            report["dispositions_sha256"],
            crate::codec_raster::digest_hex(b"ok case\n")
        );
        assert_eq!(std::fs::read_dir(&store).unwrap().count(), 5);
        root.clean().unwrap();
    }
    #[test]
    fn store_reservation_zero_listing_and_changed_listing_refuse_without_overwrite() {
        let (root, store, manifest, binary) = manifest::sample();
        let mut o = Observation::begin(&root.0, &store, &manifest, &binary, &[]).unwrap();
        assert!(Observation::begin(&root.0, &store, &manifest, &binary, &[]).is_err());
        assert!(o.run_args(0).is_err());
        std::fs::write(store.join("listing.log"), b"test case\n").unwrap();
        o.run_args(0).unwrap();
        std::fs::write(store.join("listing.log"), b"test other\n").unwrap();
        std::fs::write(store.join("results.log"), b"ok case\n").unwrap();
        assert!(o.finish(0).is_err());
        assert!(!store.join("observation.json").exists());
        root.clean().unwrap();
    }
    #[test]
    fn failed_cases_remain_diagnostic_while_zero_allignored_and_drift_never_complete() {
        for (body, code, report) in [
            ("failed case\n", 101, true),
            ("failed: synthetic-private-reason case\n", 101, true),
            ("", 0, false),
            ("ignored case\n", 0, false),
            ("ok case\n", 101, true),
        ] {
            let (root, store, manifest, binary) = manifest::sample();
            let mut o = Observation::begin(&root.0, &store, &manifest, &binary, &[]).unwrap();
            std::fs::write(store.join("listing.log"), b"test case\n").unwrap();
            o.run_args(0).unwrap();
            std::fs::write(store.join("results.log"), body).unwrap();
            assert_eq!(o.finish(code).is_ok(), report);
            if body.contains("synthetic-private-reason") {
                assert_eq!(
                    std::fs::read_to_string(store.join("results.log")).unwrap(),
                    body
                );
                let published = std::fs::read_to_string(store.join("observation.json")).unwrap();
                let dispositions = std::fs::read_to_string(store.join("dispositions.log")).unwrap();
                assert!(!published.contains("synthetic-private-reason"));
                assert_eq!(dispositions, "failed case\n");
                let value: Value = serde_json::from_str(&published).unwrap();
                assert_eq!(value["cases"], json!([{"name":"case","status":"failed"}]));
                assert_eq!(
                    value["results_sha256"],
                    crate::codec_raster::digest_hex(body.as_bytes())
                );
            }
            assert!(store.join("listing.log").is_file());
            root.clean().unwrap();
        }
        let (root, store, manifest, binary) = manifest::sample();
        let mut o = Observation::begin(&root.0, &store, &manifest, &binary, &[]).unwrap();
        std::fs::write(store.join("listing.log"), b"test case\n").unwrap();
        o.run_args(0).unwrap();
        std::fs::write(store.join("results.log"), b"ok case\n").unwrap();
        std::fs::write(root.0.join("tests/fixtures/sample.json"), b"changed").unwrap();
        assert!(o.finish(0).is_err());
        assert!(!store.join("observation.json").exists());
        root.clean().unwrap();
    }
}
