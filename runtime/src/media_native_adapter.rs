//! Same-binary codec IPC used by panel previews and the native media acceptance suite.
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static NEXT: AtomicU64 = AtomicU64::new(0);
const ENVELOPE_LIMIT: usize = 65536;
type CapturedResponse = (
    Value,
    Option<(crate::codec_video_encode::Staged, PathBuf)>,
    Vec<u8>,
);

pub struct NativeMediaAdapter {
    pub executable: PathBuf,
    pub root: PathBuf,
    pub scratch: PathBuf,
}

/// Select a production companion. Compatibility installations are never a fallback.
pub(crate) fn native_executable(root: &Path, current: &Path) -> Result<PathBuf, String> {
    let name = if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    };
    if current.file_name().is_some_and(|file| file == name) {
        return Ok(current.to_path_buf());
    }
    let directory = current
        .parent()
        .ok_or("native host has no executable directory")?;
    let directory = if directory.file_name().is_some_and(|name| name == "deps") {
        directory
            .parent()
            .ok_or("native host has no companion directory")?
    } else {
        directory
    };
    [
        directory.join(name),
        root.join(name),
        root.join("runtime/target/release").join(name),
        root.join("runtime/target/debug").join(name),
    ]
    .into_iter()
    .find(|path| path.is_file())
    .ok_or_else(|| "Standalone native executable is missing; install a standalone package or explicitly build runtime/Cargo.toml".into())
}
struct Workspace {
    directory: PathBuf,
    diagnostics_root: Option<PathBuf>,
    cleanup_attempted: bool,
    retained_in_place: bool,
}

fn diagnostic_directory(root: &Path) -> Result<PathBuf, String> {
    let parent = crate::tooling_environment::confined_directory(root, "logs", true)?;
    loop {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!(
            "native-codec-diagnostics-{}-{id}",
            std::process::id()
        ));
        match std::fs::create_dir(&path) {
            Ok(()) => return crate::tooling_environment::physical_root(&path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(_) => return Err("cannot create codec diagnostics; scratch retained".into()),
        }
    }
}

impl Workspace {
    fn clean(&mut self) -> Result<(), String> {
        // remove_dir_all can partly delete before failing; Drop must never retry it.
        self.cleanup_attempted = true;
        if crate::process_owner::cleanup_pending(&self.directory) {
            return Err("codec ownership is unresolved; scratch retained".into());
        }
        let result = match std::fs::remove_dir_all(&self.directory) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("cannot clean native codec workspace".into()),
        };
        if result.is_ok() {
            self.retained_in_place = false;
        }
        result
    }

    fn witness(root: &Path) -> Result<Self, String> {
        let witness = Self {
            directory: diagnostic_directory(root)?,
            diagnostics_root: None,
            cleanup_attempted: false,
            retained_in_place: true,
        };
        std::fs::create_dir(witness.directory.join("envelope"))
            .map_err(|_| "cannot create codec witness envelope; original evidence retained")?;
        Ok(witness)
    }

    fn persist(&self, request: &[u8], response: &[u8]) -> Result<(), String> {
        if request.len() > ENVELOPE_LIMIT || response.len() > ENVELOPE_LIMIT {
            return Err(
                "codec witness exceeds64KiB per envelope; original evidence retained".into(),
            );
        }
        let envelope = self.directory.join("envelope");
        crate::tooling_environment::physical_root(&envelope)?;
        for (name, bytes) in [("request.json", request), ("response.json", response)] {
            let path = envelope.join(name);
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|_| "cannot create codec witness; original evidence retained")?;
            file.write_all(bytes)
                .and_then(|_| file.sync_all())
                .map_err(|_| "cannot persist codec witness; original evidence retained")?;
            drop(file);
            regular(&path)?;
            let mut observed = Vec::new();
            std::fs::File::open(&path)
                .map_err(|_| "cannot open codec witness readback")?
                .take((ENVELOPE_LIMIT + 1) as u64)
                .read_to_end(&mut observed)
                .map_err(|_| "cannot read codec witness; original evidence retained")?;
            if observed != bytes {
                return Err("codec witness readback differs; original evidence retained".into());
            }
        }
        Ok(())
    }

    fn retain(&self, root: &Path) -> Result<PathBuf, String> {
        if crate::process_owner::cleanup_pending(&self.directory) {
            return Err("codec ownership is unresolved; scratch retained".into());
        }
        let source = crate::tooling_environment::physical_root(&self.directory)?;
        if !source.starts_with(root) {
            return Err("codec diagnostics escaped project; scratch retained".into());
        }
        let destination = diagnostic_directory(root)?;
        let retained = destination.join("envelope");
        // Move the whole envelope only after the later-declared process owner has finished.
        // Its response may deliberately be malformed or a link: never read or rewrite it here.
        if std::fs::rename(&self.directory, &retained).is_err() {
            let _ = std::fs::remove_dir(&destination);
            return Err("cannot relocate codec diagnostics; scratch retained".into());
        }
        Ok(retained)
    }
}
impl Drop for Workspace {
    fn drop(&mut self) {
        if self.retained_in_place {
            eprintln!(
                "{}",
                crate::logging::redact(
                    &format!(
                        "WARNING: native codec evidence retained at {}",
                        self.directory.display()
                    ),
                    &[]
                )
            );
            return;
        }
        if self.cleanup_attempted {
            return;
        }
        let notice = if let Some(root) = &self.diagnostics_root {
            match self.retain(root) {
                Ok(path) => format!(
                    "WARNING: native codec evidence retained at {}",
                    path.display()
                ),
                Err(error) => format!(
                    "ERROR: {error}; native codec evidence remains at {}",
                    self.directory.display()
                ),
            }
        } else if let Err(error) = self.clean() {
            format!("ERROR: {error}")
        } else {
            return;
        };
        eprintln!("{}", crate::logging::redact(&notice, &[]));
    }
}

fn regular(path: &Path) -> Result<(), String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "codec result must be a regular non-symlink file")?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("codec result must be a regular non-symlink file".into());
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err("codec result must not be a reparse point".into());
        }
    }
    Ok(())
}
fn destination_parent(path: &Path, create: bool) -> Result<(), String> {
    if !path.is_absolute()
        || path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err("codec destination must be absolute without parent traversal".into());
    }
    let parent = path.parent().ok_or("codec output has no parent")?;
    crate::tooling_environment::reject_links(
        parent,
        "codec destination must not use linked/reparse ancestors",
    )?;
    if create {
        std::fs::create_dir_all(parent).map_err(|_| "cannot create codec destination directory")?;
    }
    crate::tooling_environment::physical_root(parent)?;
    Ok(())
}

fn unoccupied(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) => Err("codec destination already exists".into()),
        Err(_) => Err("cannot inspect codec destination".into()),
    }
}

fn finalize(
    root: &Path,
    workspace: &mut Workspace,
    publication: Option<(crate::codec_video_encode::Staged, PathBuf)>,
    request: &[u8],
    response: &[u8],
) -> Result<(), String> {
    let mut witness = if publication.is_some() {
        let witness = Workspace::witness(root)?;
        witness.persist(request, response)?;
        Some(witness)
    } else {
        None
    };
    // A persisted witness survives final publication failure without leaving active scratch.
    // Cleanup refusal still stops before any destination can become visible.
    workspace.clean()?;
    if let Some((stage, target)) = publication {
        let published = destination_parent(&target, false)
            .and_then(|_| unoccupied(&target))
            .and_then(|_| stage.publish(&target).map(|_| ()));
        stage.clean()?;
        published?;
    }
    if let Some(witness) = &mut witness {
        witness.clean()?;
    }
    Ok(())
}

impl NativeMediaAdapter {
    /// Locate the native companion for both CLI commands and library callers.
    /// Installed and Cargo-built hosts use the same standalone native companion.
    pub fn for_project(root: &Path, scratch: PathBuf) -> Result<Self, String> {
        let current = std::env::current_exe().map_err(|error| error.to_string())?;
        let executable = native_executable(root, &current)?;
        Ok(Self {
            executable,
            root: root.into(),
            scratch,
        })
    }

    pub async fn call(&self, request: Value) -> Result<Value, String> {
        self.call_with_budget(request, Duration::from_secs(320), Duration::from_secs(60))
            .await
    }

    pub async fn call_with_budget(
        &self,
        request: Value,
        wall: Duration,
        idle: Duration,
    ) -> Result<Value, String> {
        crate::tooling_environment::physical_root(&self.root)?;
        let config = crate::config::Config::load(self.root.clone())?;
        let mut environment = std::env::vars_os().collect::<std::collections::BTreeMap<_, _>>();
        environment.insert(
            "EMOJI_FFMPEG_TIMEOUT".into(),
            config.value("EMOJI_FFMPEG_TIMEOUT").into(),
        );
        self.call_with_environment(request, wall, idle, &environment)
            .await
    }

    pub async fn call_with_environment(
        &self,
        mut request: Value,
        wall: Duration,
        idle: Duration,
        environment: &std::collections::BTreeMap<std::ffi::OsString, std::ffi::OsString>,
    ) -> Result<Value, String> {
        if !request.is_object() {
            return Err("expected a media request object".into());
        }
        if wall.is_zero() || idle.is_zero() {
            return Err("codec time budgets must be positive".into());
        }
        let raw = environment
            .get(std::ffi::OsStr::new("EMOJI_FFMPEG_TIMEOUT"))
            .map(|value| value.to_str().ok_or("codec timeout is not UTF-8"))
            .transpose()?;
        let budget = crate::codec_process::codec_budget(raw)?;
        if let Some(warning) = budget.warning {
            eprint!("{warning}");
        }
        let timeout = budget.wall.as_secs();
        let root = crate::tooling_environment::physical_root(&self.root)?;
        let executable = self
            .executable
            .canonicalize()
            .map_err(|_| "cannot resolve native codec executable")?;
        regular(&executable)?;
        crate::codec_worker::physical_directory(&root, &self.scratch)?;
        let directory = loop {
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = self
                .scratch
                .join(format!("native-codec-{}-{id}", std::process::id()));
            match std::fs::create_dir(&path) {
                Ok(()) => break path,
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(_) => return Err("cannot create native codec workspace".into()),
            }
        };
        let mut workspace = Workspace {
            directory: directory.clone(),
            diagnostics_root: None,
            cleanup_attempted: false,
            retained_in_place: false,
        };
        let ownership = directory.join("owner");
        std::fs::create_dir(&ownership).map_err(|_| "cannot create codec ownership directory")?;
        let request_path = directory.join("request.json");
        let response_path = directory.join("response.json");
        let mut output = None;
        if let Some(value) = request.get("output") {
            let target = value
                .as_str()
                .filter(|value| !value.is_empty())
                .map(PathBuf::from)
                .ok_or("output must be a non-empty path")?;
            if !target.is_absolute() {
                return Err("output must be absolute".into());
            }
            unoccupied(&target)?;
            // Final artwork may be outside the project; only IPC and temporary storage are confined.
            destination_parent(&target, true)?;
            let name = target.file_name().ok_or("codec output has no filename")?;
            if name == "request.json"
                || name == "response.json"
                || name == "codec-cleanup-pending.json"
            {
                return Err("codec output filename collides with its envelope".into());
            }
            // Preserve the basename: validators include the caller's output name in errors.
            let staged = directory.join(name);
            request["output"] = json!(staged);
            output = Some((staged, target));
        }
        let bytes = serde_json::to_vec(&request).map_err(|_| "cannot serialize codec request")?;
        if bytes.len() > ENVELOPE_LIMIT {
            return Err("codec request exceeds64KiB".into());
        }
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&request_path)
            .map_err(|_| "cannot create codec request")?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| "cannot persist codec request")?;
        drop(file);
        let mut command = Command::new(executable);
        command
            .arg("codec-worker")
            .arg(&request_path)
            .arg(&response_path)
            .current_dir(&root)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .env_clear();
        for key in [
            "PATH",
            "SystemRoot",
            "WINDIR",
            "SYSTEMDRIVE",
            "LD_LIBRARY_PATH",
            "DYLD_LIBRARY_PATH",
            "DYLD_FALLBACK_LIBRARY_PATH",
            "VIRTUAL_ENV",
            "LANG",
            "LC_ALL",
        ] {
            if let Some(value) = environment.get(std::ffi::OsStr::new(key)) {
                command.env(key, value);
            }
        }
        command
            .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
            .env("EMOJI_FFMPEG_TIMEOUT", timeout.to_string())
            .env("TEMP", &directory)
            .env("TMP", &directory)
            .env("TMPDIR", &directory);
        // Local admission refusals have no worker evidence. Once spawn is attempted,
        // cancellation/unwind drops this earlier workspace only after the owner.
        workspace.diagnostics_root = Some(root.clone());
        let mut owner = crate::process_owner::OwnedProcess::spawn_session(&mut command, ownership)?;
        let result: Result<CapturedResponse, String> = async {
            let status = owner.wait(wall, idle).await?;
            // Keep envelope/output files outside owner/ so containment stops before reading them.
            owner.finish()?;
            regular(&response_path)?;
            let file = std::fs::File::open(&response_path)
                .map_err(|_| "media adapter returned no response")?;
            let mut bytes = Vec::new();
            file.take((ENVELOPE_LIMIT + 1) as u64)
                .read_to_end(&mut bytes)
                .map_err(|_| "cannot read codec response")?;
            if bytes.len() > ENVELOPE_LIMIT {
                return Err("media adapter response exceeds64KiB".into());
            }
            let response: Value =
                serde_json::from_slice(&bytes).map_err(|_| "invalid media adapter response")?;
            if !status.success() || response.get("ok") != Some(&Value::Bool(true)) {
                let error = response
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("media adapter failed");
                return Err(crate::logging::redact(error, &[]));
            }
            let result = response
                .get("result")
                .cloned()
                .ok_or("media adapter response has no result")?;
            let publication = if let Some((staged, target)) = &output {
                regular(staged)?;
                destination_parent(target, false)?;
                unoccupied(target)?;
                // Source and destination may use different volumes; reuse exclusive sibling staging.
                let stage = crate::codec_video_encode::Staged::new(target, "tmp")?;
                let mut input =
                    std::fs::File::open(staged).map_err(|_| "cannot read rendered codec output")?;
                let mut file = std::fs::OpenOptions::new()
                    .write(true)
                    .truncate(true)
                    .open(&stage.0)
                    .map_err(|_| "cannot stage codec destination")?;
                std::io::copy(&mut input, &mut file)
                    .and_then(|_| file.sync_all())
                    .map_err(|_| "cannot persist codec destination")?;
                drop(file);
                Some((stage, target.clone()))
            } else {
                None
            };
            Ok((result, publication, bytes))
        }
        .await;
        // No destination becomes visible before containment and scratch cleanup are verified.
        owner.finish()?;
        let (result, publication, response) = result?;
        finalize(&root, &mut workspace, publication, &bytes, &response)?;
        Ok(result)
    }

    pub async fn compare(&self, a: &Path, b: &Path, format: &str) -> Result<Option<bool>, String> {
        crate::media_adapter::MediaCodec::compare(self, a, b, format).await
    }
}

impl crate::media_adapter::MediaCodec for NativeMediaAdapter {
    fn with_scratch(&self, scratch: PathBuf) -> Self {
        Self {
            executable: self.executable.clone(),
            root: self.root.clone(),
            scratch,
        }
    }

    async fn call(&self, request: Value) -> Result<Value, String> {
        NativeMediaAdapter::call(self, request).await
    }

    async fn publication_usable<'a>(
        &'a self,
        source: &'a Path,
        format: &'a str,
    ) -> Result<bool, String> {
        if format != "static" {
            // Video blankness admission already owns its Rust/FFmpeg capture;
            // it needs no extra codec envelope or application scratch directory.
            return crate::publication_media::usable(&self.root, source, format).await;
        }
        self.call(json!({"operation":"publication_usable","source":source,"format":format}))
            .await?
            .as_bool()
            .ok_or_else(|| "invalid publication media admission result".into())
    }
}

#[cfg(all(test, not(numera_native_runtime)))]
mod publication_evidence_tests {
    use super::*;
    const REQUEST: &[u8] = br#"{"operation":"preview","output":"owned-render.webp"}"#;
    const RESPONSE: &[u8] = b"{ \"ok\": true, \"result\": {\"bytes\":7} }\n";

    fn root() -> PathBuf {
        let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let parent =
            crate::tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
        let root = parent.join(format!(
            "codec publication evidence {}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&root).unwrap();
        root
    }

    fn original(root: &Path) -> Workspace {
        let directory = root.join("original");
        std::fs::create_dir(&directory).unwrap();
        std::fs::write(directory.join("request.json"), REQUEST).unwrap();
        std::fs::write(directory.join("response.json"), RESPONSE).unwrap();
        Workspace {
            directory,
            diagnostics_root: Some(root.into()),
            cleanup_attempted: false,
            retained_in_place: false,
        }
    }

    #[test]
    fn final_publication_retains_exact_evidence_without_moving_cleanup_after_publish() {
        for mode in [
            "success",
            "collision",
            "missing-stage",
            "cleanup-blocked",
            "persistence-blocked",
            "no-output",
        ] {
            let root = root();
            let mut workspace = original(&root);
            let output = root.join("destination.webp");
            let stage = crate::codec_video_encode::Staged::new(&output, "tmp").unwrap();
            std::fs::write(&stage.0, b"rendered").unwrap();
            let stage_path = stage.0.clone();
            if mode == "collision" {
                std::fs::write(&output, b"caller bytes").unwrap();
            }
            if mode == "missing-stage" {
                std::fs::remove_file(&stage.0).unwrap();
            }
            if mode == "cleanup-blocked" {
                std::fs::write(
                    workspace.directory.join("codec-cleanup-pending.json"),
                    b"synthetic owner",
                )
                .unwrap();
            }
            if matches!(mode, "persistence-blocked" | "no-output") {
                std::fs::write(root.join("logs"), b"caller diagnostics blocker").unwrap();
            }
            let publication = if mode == "no-output" {
                drop(stage);
                None
            } else {
                Some((stage, output.clone()))
            };
            let result = finalize(&root, &mut workspace, publication, REQUEST, RESPONSE);
            if mode == "success" {
                result.unwrap();
                assert_eq!(std::fs::read(&output).unwrap(), b"rendered");
                assert!(!workspace.directory.exists());
                assert_eq!(std::fs::read_dir(root.join("logs")).unwrap().count(), 0);
            } else if mode == "no-output" {
                result.unwrap();
                assert!(!workspace.directory.exists());
                assert_eq!(
                    std::fs::read(root.join("logs")).unwrap(),
                    b"caller diagnostics blocker"
                );
                assert!(!output.exists());
            } else {
                let error = result.unwrap_err();
                if mode == "collision" {
                    assert_eq!(error, "codec destination already exists");
                }
                if mode == "missing-stage" {
                    assert_eq!(error, "cannot inspect staged video output");
                }
                if mode == "persistence-blocked" {
                    assert_eq!(
                        std::fs::read(root.join("logs")).unwrap(),
                        b"caller diagnostics blocker"
                    );
                } else {
                    let retained = std::fs::read_dir(root.join("logs"))
                        .unwrap()
                        .map(|entry| entry.unwrap().path())
                        .collect::<Vec<_>>();
                    assert_eq!(retained.len(), 1);
                    let envelope = retained[0].join("envelope");
                    assert_eq!(
                        std::fs::read(envelope.join("request.json")).unwrap(),
                        REQUEST
                    );
                    assert_eq!(
                        std::fs::read(envelope.join("response.json")).unwrap(),
                        RESPONSE
                    );
                    assert_eq!(std::fs::read_dir(envelope).unwrap().count(), 2);
                }
                if matches!(mode, "cleanup-blocked" | "persistence-blocked") {
                    assert_eq!(
                        std::fs::read(workspace.directory.join("request.json")).unwrap(),
                        REQUEST
                    );
                    assert_eq!(
                        std::fs::read(workspace.directory.join("response.json")).unwrap(),
                        RESPONSE
                    );
                } else {
                    assert!(!workspace.directory.exists());
                }
                if mode == "collision" {
                    assert_eq!(std::fs::read(&output).unwrap(), b"caller bytes");
                } else {
                    assert!(!output.exists());
                }
            }
            assert!(
                !stage_path.exists(),
                "owned sibling stage must close on every path"
            );
            if mode == "cleanup-blocked" {
                // Only this fixture's synthetic marker exists; no process was launched.
                assert!(workspace.cleanup_attempted);
                std::fs::remove_file(workspace.directory.join("codec-cleanup-pending.json"))
                    .unwrap();
            }
            if workspace.directory.exists() {
                workspace.clean().unwrap();
            }
            drop(workspace);
            std::fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn partial_witness_persistence_never_overwrites_or_deletes_original_evidence() {
        let root = root();
        let mut workspace = original(&root);
        let mut witness = Workspace::witness(&root).unwrap();
        let retained = witness.directory.clone();
        assert!(
            witness
                .persist(&vec![b'x'; ENVELOPE_LIMIT + 1], RESPONSE)
                .is_err()
        );
        assert!(
            witness
                .persist(REQUEST, &vec![b'x'; ENVELOPE_LIMIT + 1])
                .is_err()
        );
        assert_eq!(
            std::fs::read_dir(retained.join("envelope"))
                .unwrap()
                .count(),
            0
        );
        let response = retained.join("envelope/response.json");
        std::fs::write(&response, b"caller evidence").unwrap();
        assert!(witness.persist(REQUEST, RESPONSE).is_err());
        let marker = retained.join("codec-cleanup-pending.json");
        std::fs::write(&marker, b"synthetic cleanup stop").unwrap();
        assert!(witness.clean().is_err());
        assert!(witness.cleanup_attempted && witness.retained_in_place);
        drop(witness);
        assert_eq!(std::fs::read(&marker).unwrap(), b"synthetic cleanup stop");
        assert_eq!(
            std::fs::read(retained.join("envelope/request.json")).unwrap(),
            REQUEST
        );
        assert_eq!(std::fs::read(response).unwrap(), b"caller evidence");
        assert_eq!(
            std::fs::read(workspace.directory.join("response.json")).unwrap(),
            RESPONSE
        );
        workspace.clean().unwrap();
        drop(workspace);
        std::fs::remove_dir_all(root).unwrap();
    }
}

#[cfg(test)]
mod production_selection_tests {
    use super::*;

    #[test]
    fn production_fallback_refuses_old_installations_and_selects_standalone_locations() {
        let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let parent =
            crate::tooling_environment::confined_directory(project, "logs/tmp", true).unwrap();
        let root = parent.join(format!(
            "standalone binary selection {}",
            std::process::id()
        ));
        std::fs::create_dir(&root).unwrap();
        let name = if cfg!(windows) {
            "numera-emoji.exe"
        } else {
            "numera-emoji"
        };
        let old_binary = root.join("native/runtime").join(name);
        std::fs::create_dir_all(old_binary.parent().unwrap()).unwrap();
        std::fs::write(
            &old_binary,
            b"old compatibility installation; must never be selected",
        )
        .unwrap();
        let host = root.join("embedding/host");
        assert!(native_executable(&root, &host).is_err());
        let debug = root.join("runtime/target/debug").join(name);
        std::fs::create_dir_all(debug.parent().unwrap()).unwrap();
        std::fs::write(&debug, b"production debug location").unwrap();
        assert_eq!(native_executable(&root, &host).unwrap(), debug);
        let release = root.join("runtime/target/release").join(name);
        std::fs::create_dir_all(release.parent().unwrap()).unwrap();
        std::fs::write(&release, b"production release location").unwrap();
        assert_eq!(native_executable(&root, &host).unwrap(), release);
        let installed = root.join(name);
        std::fs::write(&installed, b"installed standalone location").unwrap();
        assert_eq!(native_executable(&root, &host).unwrap(), installed);
        assert_eq!(
            std::fs::read(old_binary).unwrap(),
            b"old compatibility installation; must never be selected"
        );
        assert!(!root.join("pyproject.toml").exists());
        assert!(!root.join(".venv").exists());
        std::fs::remove_dir_all(root).unwrap();
    }
}
