//! Real-owner supplemental progress contracts; called only inside the dedicated retained driver.
use super::{OwnedProcess, WaitOutcome};
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const SENTINEL: &str = "retained progress callback sentinel";

#[derive(Clone, Copy, Debug)]
enum Case {
    Idle,
    FileProgressThenError,
    WallDespiteProgress,
}

struct Witness {
    result: Result<WaitOutcome, String>,
    before: Vec<u64>,
    after: Vec<u64>,
    elapsed: Duration,
    writes: u32,
}

pub(crate) async fn exercise(
    root: &Path,
    argv: &[OsString],
    environment: &BTreeMap<String, String>,
) {
    // These are unmeasured failure ceilings, not observed platform timings.
    for (name, case) in [
        ("owner-idle", Case::Idle),
        ("owner-progress-error", Case::FileProgressThenError),
        ("owner-wall", Case::WallDespiteProgress),
    ] {
        let directory = root.join("logs/tmp").join(name);
        std::fs::create_dir(&directory).unwrap();
        let workspace = directory.join("owner");
        std::fs::create_dir(&workspace).unwrap();
        let output = directory.join("stdout.log");
        let errors = directory.join("stderr.log");
        let create = |path: &Path| {
            std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .unwrap()
        };
        let mut command = Command::new(&argv[0]);
        command
            .args(&argv[1..])
            .current_dir(root)
            .env_clear()
            .envs(environment)
            .env("NUMERA_RETAINED_CASE", "quiet")
            .env("NUMERA_RETAINED_CONTROL", &directory)
            .stdin(Stdio::null())
            .stdout(Stdio::from(create(&output)))
            .stderr(Stdio::from(create(&errors)));
        let mut owner = OwnedProcess::spawn_descendants(&mut command, workspace.clone()).unwrap();
        drop(command);
        let witness = observe(&mut owner, &directory, &output, case).await;
        // No assertion may unwind across a live owner; even a readiness/callback error must finish.
        let finish = owner.finish();
        let settled = finish.is_ok() && owner.tree.quiescent() == Ok(true);
        drop(owner);
        finish.unwrap();
        assert!(settled, "{case:?}: owner did not reach verified quiescence");
        assert!(!workspace.exists());
        let witness = witness.unwrap();
        assert!(
            !witness.before.is_empty(),
            "{case:?}: no live child was sampled"
        );
        assert_eq!(
            witness.before, witness.after,
            "{case:?}: CPU/Job IO changed; this run does not isolate supplemental file progress"
        );
        assert!(!directory.join("fallback").exists());
        match case {
            Case::Idle => {
                assert!(
                    matches!(witness.result, Ok(WaitOutcome::Idle(limit)) if limit == Duration::from_millis(700))
                );
                assert!(witness.elapsed >= Duration::from_millis(700));
                assert_eq!(witness.writes, 0);
            }
            Case::FileProgressThenError => {
                assert!(matches!(witness.result, Err(ref error) if error == SENTINEL));
                assert!(witness.elapsed >= Duration::from_millis(1800));
                assert!(
                    witness.writes >= 5,
                    "progress must span multiple original idle windows"
                );
                assert!(
                    std::fs::read(output)
                        .unwrap()
                        .ends_with(b"external progress\n")
                );
            }
            Case::WallDespiteProgress => {
                assert!(
                    matches!(witness.result, Ok(WaitOutcome::Wall(limit)) if limit == Duration::from_millis(1500))
                );
                assert!(witness.elapsed >= Duration::from_millis(1500));
                assert!(witness.writes >= 5);
            }
        }
    }
}

async fn observe(
    owner: &mut OwnedProcess,
    directory: &Path,
    output: &Path,
    case: Case,
) -> Result<Witness, String> {
    let deadline = Instant::now() + Duration::from_secs(4);
    while std::fs::read(directory.join("ready")).ok().as_deref() != Some(b"ready") {
        if Instant::now() >= deadline {
            return Err("quiet owner fixture readiness exceeded4s".into());
        }
        if owner.try_reap()?.is_some() {
            return Err("quiet owner fixture exited before readiness".into());
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let mut before = owner.tree.sample()?;
    let mut stable = Instant::now();
    loop {
        if Instant::now() >= deadline {
            return Err("quiet owner fixture did not establish stable accounting within4s".into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        let next = owner.tree.sample()?;
        if next != before {
            before = next;
            stable = Instant::now();
        }
        if !before.is_empty() && stable.elapsed() >= Duration::from_millis(400) {
            break;
        }
    }
    // Driver writes are outside the leaf's Windows Job and Linux descendant CPU sample.
    // With the live-child sample unchanged across the whole wait, neither can mask a lost callback.
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(output)
        .map_err(|e| e.to_string())?;
    let started = Instant::now();
    let mut last = started;
    let mut writes = 0;
    let (wall, idle) = match case {
        Case::Idle => (Duration::from_secs(3), Duration::from_millis(700)),
        Case::FileProgressThenError => (Duration::from_secs(5), Duration::from_millis(600)),
        Case::WallDespiteProgress => (Duration::from_millis(1500), Duration::from_millis(600)),
    };
    let result = owner
        .wait_progress(wall, idle, || {
            if matches!(case, Case::FileProgressThenError)
                && started.elapsed() >= Duration::from_millis(1800)
            {
                return Err(SENTINEL.into());
            }
            if matches!(case, Case::Idle) || last.elapsed() < Duration::from_millis(200) {
                return Ok(false);
            }
            file.write_all(b"external progress\n")
                .map_err(|e| e.to_string())?;
            last = Instant::now();
            writes += 1;
            Ok(true)
        })
        .await;
    let elapsed = started.elapsed();
    let after = owner.tree.sample()?;
    drop(file);
    Ok(Witness {
        result,
        before,
        after,
        elapsed,
        writes,
    })
}
