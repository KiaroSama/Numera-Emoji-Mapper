//! New orchestration controls, separate from the1516 historical business identities.
use super::*;
use crate::baseline_catalog_support::Scratch;
use serde_json::json;
use std::collections::VecDeque;

mod conversion_session_proof;

struct Reply {
    phase: Phase,
    code: i32,
    cursor: Option<i32>,
    capture: String,
}
fn step(phase: Phase, code: i32, cursor: Option<i32>, capture: &str) -> Reply {
    Reply {
        phase,
        code,
        cursor,
        capture: capture.into(),
    }
}
struct Script {
    root: Scratch,
    replies: VecDeque<Reply>,
    calls: Vec<Phase>,
    sleeps: Vec<Duration>,
    notes: Vec<String>,
    interrupted: bool,
    marker: String,
}
impl Script {
    fn new(replies: Vec<Reply>) -> Self {
        let root = Scratch::new();
        std::fs::write(root.0.join("state.json"), "{\"cursor\":0}").unwrap();
        Self {
            root,
            replies: replies.into(),
            calls: vec![],
            sleeps: vec![],
            notes: vec![],
            interrupted: false,
            marker: "stuck.svg".into(),
        }
    }
    fn capture_path(&self, phase: Phase) -> PathBuf {
        self.root.0.join(if phase == Phase::Png {
            "png.log"
        } else {
            "svg.log"
        })
    }
    fn png(&self) {
        std::fs::create_dir(self.root.0.join("png")).unwrap();
    }
    fn drained(&self) {
        assert!(self.replies.is_empty(), "unused phase outcomes");
    }
}
impl Steps for Script {
    async fn phase(&mut self, phase: Phase) -> Result<i32> {
        let r = self.replies.pop_front().expect("unplanned extra phase");
        assert_eq!(r.phase, phase);
        self.calls.push(phase);
        if let Some(cursor) = r.cursor {
            crate::atomic::write_json(&self.root.0.join("state.json"), &json!({"cursor":cursor}))
                .unwrap();
        }
        std::fs::write(self.capture_path(phase), r.capture).unwrap();
        Ok(r.code)
    }
    async fn backoff(&mut self, d: Duration) -> Result<bool> {
        self.sleeps.push(d);
        Ok(!self.interrupted)
    }
    fn cursor(&mut self) -> i32 {
        cursor(&self.root.0.join("state.json"))
    }
    fn marker(&mut self) -> String {
        self.marker.clone()
    }
    fn capture(&mut self, p: Phase) -> Transcript {
        transcript(&self.capture_path(p))
    }
    fn png_exists(&self) -> bool {
        self.root.0.join("png").exists()
    }
    fn skip_file(&self) -> PathBuf {
        self.root.0.join(".svg_skip.txt")
    }
    fn note(&mut self, text: &str) -> Result<()> {
        self.notes.push(text.into());
        Ok(())
    }
}
fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}
#[test]
fn rebuild_runs_build_then_map_then_links_and_finishes_only_after_all_three() {
    let mut s = Script::new(vec![
        step(Phase::Build, 0, None, ""),
        step(Phase::Map, 0, None, ""),
        step(Phase::Links, 0, None, ""),
    ]);
    assert_eq!(runtime().block_on(rebuild(&mut s)).unwrap(), 0);
    assert_eq!(s.calls, [Phase::Build, Phase::Map, Phase::Links]);
    assert!(s.sleeps.is_empty());
    assert_eq!(s.notes.last().unwrap(), "=== ALL DONE ===");
    s.drained();
}
#[test]
fn rebuild_retries_only_progressing_three_and_backs_off_fifteen_seconds() {
    let mut s = Script::new(vec![
        step(Phase::Build, 3, Some(1), ""),
        step(Phase::Build, 3, Some(2), ""),
        step(Phase::Build, 0, None, ""),
        step(Phase::Map, 0, None, ""),
        step(Phase::Links, 0, None, ""),
    ]);
    assert_eq!(runtime().block_on(rebuild(&mut s)).unwrap(), 0);
    assert_eq!(s.sleeps, [Duration::from_secs(15); 2]);
    assert_eq!(
        s.calls,
        [
            Phase::Build,
            Phase::Build,
            Phase::Build,
            Phase::Map,
            Phase::Links
        ]
    );
    s.drained();
}
#[test]
fn rebuild_two_stalled_checkpoints_stop_before_another_sleep_or_phase() {
    let mut s = Script::new(vec![
        step(Phase::Build, 3, Some(0), ""),
        step(Phase::Build, 3, Some(0), ""),
    ]);
    assert_eq!(runtime().block_on(rebuild(&mut s)).unwrap(), 3);
    assert_eq!(s.calls, [Phase::Build, Phase::Build]);
    assert_eq!(s.sleeps, [Duration::from_secs(15)]);
    assert!(s.notes.iter().any(|s| s.contains("for 2 checkpoints")));
    assert!(s.notes.last().unwrap().contains("map + links skipped"));
    s.drained();
}
#[test]
fn rebuild_progress_resets_the_stall_counter() {
    let mut s = Script::new(
        [0, 1, 1, 2, 2, 2]
            .into_iter()
            .map(|cursor| step(Phase::Build, 3, Some(cursor), ""))
            .collect(),
    );
    assert_eq!(runtime().block_on(rebuild(&mut s)).unwrap(), 3);
    assert_eq!(s.calls, vec![Phase::Build; 6]);
    assert_eq!(s.sleeps, vec![Duration::from_secs(15); 5]);
    s.drained();
}
#[test]
fn rebuild_exactly_two_hundred_progressing_attempts_bound_the_retry_loop() {
    let mut s = Script::new(
        (1..=200)
            .map(|cursor| step(Phase::Build, 3, Some(cursor), ""))
            .collect(),
    );
    assert_eq!(runtime().block_on(rebuild(&mut s)).unwrap(), 3);
    assert_eq!(s.calls, vec![Phase::Build; 200]);
    assert_eq!(s.sleeps, vec![Duration::from_secs(15); 199]);
    assert!(
        s.notes
            .iter()
            .any(|s| s.contains("gave up after 200 attempts"))
    );
    s.drained();
}
#[test]
fn rebuild_propagates_every_terminal_build_status_without_retry() {
    for code in [1, 2, 4, 124, 125, 130, 255, -1073741819] {
        let mut s = Script::new(vec![step(Phase::Build, code, Some(1), "")]);
        assert_eq!(runtime().block_on(rebuild(&mut s)).unwrap(), code);
        assert_eq!(s.calls, [Phase::Build]);
        assert!(s.sleeps.is_empty());
        s.drained();
    }
}
#[test]
fn rebuild_map_refusal_including_three_never_sends_links() {
    for code in [2, 3, 4, 130] {
        let mut s = Script::new(vec![
            step(Phase::Build, 0, None, ""),
            step(Phase::Map, code, None, ""),
        ]);
        assert_eq!(runtime().block_on(rebuild(&mut s)).unwrap(), code);
        assert_eq!(s.calls, [Phase::Build, Phase::Map]);
        assert!(s.notes.last().unwrap().contains("links skipped"));
        s.drained();
    }
}
#[test]
fn rebuild_links_failure_propagates_without_success_note() {
    let mut s = Script::new(vec![
        step(Phase::Build, 0, None, ""),
        step(Phase::Map, 0, None, ""),
        step(Phase::Links, 4, None, ""),
    ]);
    assert_eq!(runtime().block_on(rebuild(&mut s)).unwrap(), 4);
    assert!(!s.notes.iter().any(|s| s.contains("ALL DONE")));
    s.drained();
}
#[test]
fn rebuild_interrupt_during_backoff_never_starts_the_next_attempt() {
    let mut s = Script::new(vec![step(Phase::Build, 3, Some(1), "")]);
    s.interrupted = true;
    assert_eq!(runtime().block_on(rebuild(&mut s)).unwrap(), 130);
    assert_eq!(s.calls, [Phase::Build]);
    assert_eq!(s.sleeps, [Duration::from_secs(15)]);
    s.drained();
    let config = Config::load_with_environment(s.root.0.clone(), &Default::default()).unwrap();
    let log = RunLog::open(&s.root.0, "actual-backoff-cancel", vec![]);
    let mut native = Native {
        config: &config,
        log: &log,
        coin: s.root.0.clone(),
        output: s.root.0.join("output"),
        forwarded: vec![],
        append: None,
        cancel: Box::pin(async { Ok(()) }),
    };
    assert!(
        !runtime()
            .block_on(async {
                tokio::time::timeout(
                    Duration::from_millis(200),
                    native.backoff(Duration::from_secs(15)),
                )
                .await
                .expect("default native backoff ignored cancellation")
            })
            .unwrap()
    );
}
#[test]
fn cursor_observes_real_state_and_preserves_the_script_integer_cast() {
    let root = Scratch::new();
    let path = root.0.join("state.json");
    assert_eq!(cursor(&path), -1);
    for (bytes, expected) in [
        ("{bad", -1),
        ("{}", 0),
        ("{\"cursor\":null}", 0),
        ("{\"cursor\":42}", 42),
        ("{\"cursor\":\"42\"}", 42),
        ("{\"cursor\":\" \"}", 0),
        ("{\"cursor\":true}", 1),
        ("{\"cursor\":2.5}", 2),
        ("{\"cursor\":3.5}", 4),
        ("{\"cursor\":2147483648}", -1),
    ] {
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(cursor(&path), expected, "{bytes}");
    }
}
#[test]
fn convert_completes_svg_then_runs_exactly_one_png_fallback() {
    let mut s = Script::new(vec![
        step(Phase::Svg, 0, None, "item\nDONE: svg ready\n"),
        step(Phase::Png, 0, None, "DONE: png ready\n"),
    ]);
    s.png();
    assert_eq!(runtime().block_on(convert(&mut s)).unwrap(), 0);
    assert_eq!(s.calls, [Phase::Svg, Phase::Png]);
    assert!(s.notes.iter().any(|s| s == "DONE: png ready"));
    s.drained();
}
#[test]
fn convert_retries_only_owned_idle_and_surfaces_killed_sources_for_review() {
    let mut s = Script::new(vec![
        step(
            Phase::Svg,
            124,
            None,
            "DONE: printed before the owned idle timeout\n",
        ),
        step(
            Phase::Svg,
            0,
            None,
            "QUARANTINE: stuck.svg\nREVIEW: stuck.svg\nDONE: rest ready\n",
        ),
    ]);
    assert_eq!(runtime().block_on(convert(&mut s)).unwrap(), 3);
    assert_eq!(s.calls, [Phase::Svg, Phase::Svg]);
    assert!(
        s.notes
            .iter()
            .any(|s| s.contains("REVIEW killed sources: stuck.svg"))
    );
    assert!(
        s.notes
            .iter()
            .any(|s| s == "[watchdog] QUARANTINE: stuck.svg")
    );
    s.drained();
}
#[test]
fn convert_one_hundred_idle_attempts_stop_with_review_without_png() {
    let mut s = Script::new(
        (0..100)
            .map(|_| step(Phase::Svg, 124, None, "working\n"))
            .collect(),
    );
    s.png();
    assert_eq!(runtime().block_on(convert(&mut s)).unwrap(), 3);
    assert_eq!(s.calls, vec![Phase::Svg; 100]);
    assert!(s.notes.iter().any(|s| s.contains("iter 100")));
    assert!(s.notes.last().unwrap().contains("did NOT complete"));
    s.drained();
}
#[test]
fn convert_wall_timeout_stops_without_retry_or_png() {
    for output in ["working\n", "DONE: printed before the wall timeout\n"] {
        let mut s = Script::new(vec![step(Phase::Svg, 125, None, output)]);
        s.png();
        assert_eq!(runtime().block_on(convert(&mut s)).unwrap(), 1);
        assert_eq!(s.calls, [Phase::Svg]);
        s.drained();
    }
    let mut s = Script::new(vec![
        step(Phase::Svg, 124, None, "working\n"),
        step(Phase::Svg, 125, None, "DONE: stale footer\n"),
    ]);
    s.png();
    assert_eq!(runtime().block_on(convert(&mut s)).unwrap(), 3);
    assert_eq!(s.calls, [Phase::Svg, Phase::Svg]);
    s.drained();
    assert_eq!(
        crate::tooling_convert::deadline_code(Duration::from_secs(3599), Duration::from_secs(119)),
        None
    );
    assert_eq!(
        crate::tooling_convert::deadline_code(Duration::from_secs(120), Duration::from_secs(120)),
        Some(124)
    );
    assert_eq!(
        crate::tooling_convert::deadline_code(Duration::from_secs(3600), Duration::from_secs(120)),
        Some(125)
    );
}
#[test]
fn convert_ordinary_exit_without_done_is_terminal_even_after_an_earlier_idle() {
    for code in [0, 1, 2, 4] {
        let mut s = Script::new(vec![step(Phase::Svg, code, None, "ordinary output\n")]);
        s.png();
        assert_eq!(runtime().block_on(convert(&mut s)).unwrap(), code.max(1));
        assert_eq!(s.calls, [Phase::Svg]);
        s.drained();
    }
    let mut s = Script::new(vec![
        step(Phase::Svg, 124, None, "working\n"),
        step(Phase::Svg, 0, None, "no done\n"),
    ]);
    s.png();
    assert_eq!(runtime().block_on(convert(&mut s)).unwrap(), 3);
    assert_eq!(s.calls, [Phase::Svg, Phase::Svg]);
    s.drained();
}
#[test]
fn convert_done_must_be_the_final_line_and_prefixes_keep_powershell_case_semantics() {
    for text in [
        "DONE: early\nunfinished\n",
        "DONE: early\n\n",
        "notDONE: false\n",
    ] {
        let mut s = Script::new(vec![step(Phase::Svg, 0, None, text)]);
        assert_eq!(runtime().block_on(convert(&mut s)).unwrap(), 1);
        s.drained();
    }
    let mut s = Script::new(vec![step(
        Phase::Svg,
        0,
        None,
        "review: source.svg\ndone: complete\n",
    )]);
    assert_eq!(runtime().block_on(convert(&mut s)).unwrap(), 3);
    assert!(s.notes.iter().any(|s| s == "[watchdog] review: source.svg"));
    s.drained();
}
#[test]
fn convert_completed_converter_errors_and_review_keep_the_highest_exit() {
    for (code, text, expected) in [
        (4, "REVIEW: skipped\nDONE: finished\n", 4),
        (2, "DONE: finished\n", 2),
        (0, "QUARANTINE: skipped\nDONE: finished\n", 3),
    ] {
        let mut s = Script::new(vec![
            step(Phase::Svg, code, None, text),
            step(Phase::Png, 0, None, "ordinary PNG output without DONE\n"),
        ]);
        s.png();
        assert_eq!(runtime().block_on(convert(&mut s)).unwrap(), expected);
        s.drained();
    }
}
#[test]
fn convert_png_timeouts_are_one_failure_and_never_retried() {
    for code in [124, 125] {
        let mut s = Script::new(vec![
            step(Phase::Svg, 0, None, "DONE: svg\n"),
            step(Phase::Png, code, None, "partial png\n"),
        ]);
        s.png();
        assert_eq!(runtime().block_on(convert(&mut s)).unwrap(), 1);
        assert_eq!(s.calls, [Phase::Svg, Phase::Png]);
        s.drained();
    }
    for code in [2, 4] {
        let mut s = Script::new(vec![
            step(Phase::Svg, 0, None, "DONE: svg\n"),
            step(Phase::Png, code, None, "ordinary PNG refusal\n"),
        ]);
        s.png();
        assert_eq!(runtime().block_on(convert(&mut s)).unwrap(), code);
        assert_eq!(s.calls, [Phase::Svg, Phase::Png]);
        s.drained();
    }
    let mut s = Script::new(vec![
        step(Phase::Svg, 0, None, "REVIEW: prior\nDONE: svg\n"),
        step(Phase::Png, 125, None, "partial\n"),
    ]);
    s.png();
    assert_eq!(runtime().block_on(convert(&mut s)).unwrap(), 3);
    s.drained();
}
#[test]
fn convert_interrupt_never_starts_fallback_or_retry() {
    for phase in [Phase::Svg, Phase::Png] {
        let mut replies = vec![];
        if phase == Phase::Png {
            replies.push(step(Phase::Svg, 0, None, "DONE: svg\n"));
        }
        replies.push(step(phase, 130, None, ""));
        let mut s = Script::new(replies);
        s.png();
        assert_eq!(runtime().block_on(convert(&mut s)).unwrap(), 130);
        s.drained();
    }
}

#[test]
fn an_already_cancelled_owned_phase_has_no_spawn_or_capture_side_effects() {
    let root = Scratch::new();
    let launch = tooling_launch::Launch {
        root: root.0.clone(),
        argv: vec!["must-not-be-spawned".into()],
        environment: Default::default(),
    };
    let log = RunLog::open(&root.0, "workflow-cancel", vec![]);
    let mut cancel: Pin<Box<dyn Future<Output = Result<()>>>> = Box::pin(async { Ok(()) });
    assert_eq!(
        runtime()
            .block_on(owned(
                &launch,
                Some(&root.0.join("capture.log")),
                &mut cancel,
                &log
            ))
            .unwrap(),
        130
    );
    assert!(!root.0.join("capture.log").exists());
    assert!(!root.0.join("logs/tmp").exists());
}
#[test]
fn owned_capture_setup_failure_cleans_its_unspawned_workspace() {
    let root = Scratch::new();
    let directory = root.0.join("not-a-log-file");
    std::fs::create_dir(&directory).unwrap();
    let launch = tooling_launch::Launch {
        root: root.0.clone(),
        argv: vec![
            std::env::current_exe().unwrap().into_os_string(),
            "--help".into(),
        ],
        environment: std::env::vars().collect(),
    };
    let log = RunLog::open(&root.0, "workflow-setup", vec![]);
    let mut cancel: Pin<Box<dyn Future<Output = Result<()>>>> = Box::pin(std::future::pending());
    assert!(
        runtime()
            .block_on(owned(&launch, Some(&directory), &mut cancel, &log))
            .is_err()
    );
    assert!(
        std::fs::read_dir(root.0.join("logs/tmp/tooling"))
            .unwrap()
            .next()
            .is_none()
    );
}
fn child_role() -> bool {
    let Ok(role) = std::env::var("NUMERA_COIN_WORKFLOW_CHILD") else {
        return false;
    };
    if role == "grandchild" {
        loop {
            std::thread::sleep(Duration::from_secs(60));
        }
    }
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "coin_workflows::tests::owned_cancellation_stops_the_actual_child_and_grandchild",
            "--nocapture",
        ])
        .env("NUMERA_COIN_WORKFLOW_CHILD", "grandchild")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    std::fs::write(
        std::env::var_os("NUMERA_COIN_WORKFLOW_READY").unwrap(),
        format!("{}\n{}\n", std::process::id(), child.id()),
    )
    .unwrap();
    let _ = child.wait();
    true
}
#[allow(unsafe_code)]
fn assert_process(pid: u32, stopped: bool) {
    #[cfg(target_os = "linux")]
    match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(stat) => assert_eq!(
            stat.rsplit_once(") ").unwrap().1.starts_with('Z'),
            stopped,
            "fixture PID{pid} state: {stat}"
        ),
        Err(error) => assert!(
            stopped
                && (error.kind() == std::io::ErrorKind::NotFound
                    || error.raw_os_error() == Some(3)),
            "cannot account for fixture PID{pid}: {error}"
        ),
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::{FromRawHandle, OwnedHandle};
        use windows_sys::Win32::Foundation::{
            ERROR_INVALID_PARAMETER, GetLastError, WAIT_OBJECT_0, WAIT_TIMEOUT,
        };
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
        };
        // Inspect only acknowledged fixture IDs, with a noninheritable read-only synchronization handle.
        unsafe {
            let raw = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
            if raw.is_null() {
                assert!(stopped);
                assert_eq!(GetLastError(), ERROR_INVALID_PARAMETER);
            } else {
                let _handle = OwnedHandle::from_raw_handle(raw);
                assert_eq!(
                    WaitForSingleObject(raw, 0),
                    if stopped { WAIT_OBJECT_0 } else { WAIT_TIMEOUT }
                );
            }
        }
    }
}
#[test]
fn owned_cancellation_stops_the_actual_child_and_grandchild() {
    if child_role() {
        return;
    }
    let root = Scratch::new();
    let ready = root.0.join("ready");
    let mut environment = std::env::vars().collect::<std::collections::BTreeMap<_, _>>();
    environment.insert("NUMERA_COIN_WORKFLOW_CHILD".into(), "parent".into());
    environment.insert(
        "NUMERA_COIN_WORKFLOW_READY".into(),
        ready.to_str().unwrap().into(),
    );
    let launch = tooling_launch::Launch {
        root: root.0.clone(),
        argv: vec![
            std::env::current_exe().unwrap().into_os_string(),
            "--exact".into(),
            "coin_workflows::tests::owned_cancellation_stops_the_actual_child_and_grandchild"
                .into(),
            "--nocapture".into(),
        ],
        environment,
    };
    let observed = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u32>::new()));
    let seen = observed.clone();
    let mut cancel: Pin<Box<dyn Future<Output = Result<()>>>> = Box::pin(async move {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            if ready.is_file() {
                let pids = std::fs::read_to_string(&ready)
                    .unwrap()
                    .lines()
                    .map(|s| s.parse::<u32>().unwrap())
                    .collect::<Vec<_>>();
                assert_eq!(pids.len(), 2);
                for p in &pids {
                    assert_process(*p, false);
                }
                *seen.lock().unwrap() = pids;
                return Ok(());
            }
            if std::time::Instant::now() >= deadline {
                return Err("controlled child never became ready".into());
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    });
    let log = RunLog::open(&root.0, "actual-workflow-cancel", vec![]);
    assert_eq!(
        runtime()
            .block_on(owned(&launch, None, &mut cancel, &log))
            .unwrap(),
        130
    );
    for pid in observed.lock().unwrap().iter() {
        assert_process(*pid, true);
    }
    assert!(
        std::fs::read_dir(root.0.join("logs/tmp/tooling"))
            .unwrap()
            .next()
            .is_none()
    );
}
fn installed(root: &Path) -> PathBuf {
    let name = if cfg!(windows) {
        "numera-emoji.exe"
    } else {
        "numera-emoji"
    };
    let current = std::env::current_exe().unwrap();
    let source = current
        .ancestors()
        .skip(1)
        .map(|p| p.join(name))
        .find(|p| p.is_file())
        .expect("Cargo must build the actual primary binary");
    let binary = root.join(name);
    std::fs::copy(source, &binary).unwrap();
    std::fs::create_dir(root.join("assets")).unwrap();
    let project = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    std::fs::copy(
        project.join("assets/panel.html"),
        root.join("assets/panel.html"),
    )
    .unwrap();
    binary
}
#[test]
fn actual_primary_conversion_quarantines_a_pending_marker_and_runs_png_fallback() {
    let root = Scratch::new();
    let binary = installed(&root.0);
    let coin = root.0.join("coins");
    for dir in ["logos/svg", "logos/png", "logos/emoji"] {
        std::fs::create_dir_all(coin.join(dir)).unwrap();
    }
    let svg=b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"8\" height=\"8\"><rect width=\"8\" height=\"8\" fill=\"blue\"/></svg>";
    std::fs::write(coin.join("logos/svg/art.svg"), svg).unwrap();
    std::fs::write(coin.join("logos/svg/stuck.svg"), svg).unwrap();
    std::fs::write(coin.join("logos/emoji/.svg_cur"), "stuck").unwrap();
    image::RgbaImage::from_pixel(8, 8, image::Rgba([0u8, 180, 90, 255]))
        .save(coin.join("logos/png/raster.png"))
        .unwrap();
    let mut environment = std::env::vars().collect::<std::collections::BTreeMap<_, _>>();
    environment.insert("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into());
    let launch = tooling_launch::Launch {
        root: root.0.clone(),
        argv: vec![binary.into_os_string(), "coin-convert".into()],
        environment,
    };
    let log = RunLog::open(&root.0, "real-coin-convert", vec![]);
    let capture = root.0.join("primary-output.txt");
    let mut cancel: Pin<Box<dyn Future<Output = Result<()>>>> = Box::pin(async {
        tokio::time::sleep(Duration::from_secs(30)).await;
        Err("actual conversion exceeded its30s test bound".into())
    });
    assert_eq!(
        runtime()
            .block_on(owned(&launch, Some(&capture), &mut cancel, &log))
            .unwrap(),
        3
    );
    let out = std::fs::read_to_string(&capture).unwrap();
    assert!(out.contains("SVG conversion complete: DONE:"), "{out}");
    assert!(out.contains("REVIEW:"), "{out}");
    assert!(
        std::fs::read_to_string(coin.join("logos/emoji/.svg_skip.txt"))
            .unwrap()
            .lines()
            .any(|s| s == "stuck")
    );
    assert!(!coin.join("logos/emoji/.svg_cur").exists());
    assert!(!coin.join("logos/emoji/stuck.png").exists());
    for name in ["art.png", "raster.png"] {
        let image = image::open(coin.join("logos/emoji").join(name)).unwrap();
        assert_eq!((image.width(), image.height()), (100, 100));
    }
    assert_eq!(
        std::fs::read(coin.join("logos/svg/stuck.svg")).unwrap(),
        svg
    );
    assert!(coin.join("emoji_png_log.txt").is_file());
    assert!(
        transcript(&coin.join("emoji_png_log.txt"))
            .last
            .unwrap()
            .starts_with("DONE:")
    );
    assert!(!crate::process_owner::cleanup_pending(
        &root.0.join("logs/tmp")
    ));
}
#[test]
fn help_and_bad_arguments_never_start_an_operator_phase() {
    let root = Scratch::new();
    let config = Config::load_with_environment(
        root.0.clone(),
        &std::collections::BTreeMap::from([("NUMERA_EMOJI_MAPPER_NO_DOTENV".into(), "1".into())]),
    )
    .unwrap();
    let log = RunLog::open(&root.0, "workflow-help", vec![]);
    for command in ["coin-convert", "coin-rebuild-loop"] {
        assert_eq!(run(&config, command, &["--help".into()], &log).unwrap(), 0);
    }
    assert!(
        run(&config, "coin-rebuild-loop", &["--typo".into()], &log)
            .unwrap_err()
            .contains("unknown")
    );
    assert!(
        run(
            &config,
            "coin-convert",
            &["--in".into(), "source".into()],
            &log
        )
        .is_err()
    );
    assert!(!root.0.join("logs/tmp").exists());
}
