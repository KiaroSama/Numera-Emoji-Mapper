use super::*;

fn preflight_fixture() -> (Fixture, serde_json::Value) {
    let mut f = Fixture::empty();
    for i in 0..3 {
        let path = png(&f.data.join(format!("media/i{i}.png")), [200, 30, 30, 255]);
        let key = format!("s:item{i}");
        f.catalog()
            .insert(
                &json!({"content_key":key,"fmt":"static","file_path":path}),
                UTC,
            )
            .unwrap();
        f.keys.push(key);
    }
    let plan = json!({"static":f.keys});
    (f, plan)
}
fn checked_preflight(
    f: &Fixture,
    plan: &serde_json::Value,
    answers: &serde_json::Value,
) -> (u8, Vec<String>) {
    let mut asked = Vec::new();
    let log = RunLog::open(&f.root.0, "preflight-original", vec![]);
    let code = super::preflight_with_check(
        &mut f.catalog(),
        plan,
        &["static".into()],
        "testbase",
        &std::collections::BTreeSet::new(),
        (42, &log),
        |user, path, fmt, upload| {
            assert_eq!(user, 42);
            assert_eq!(fmt, "static");
            assert_eq!(upload.field, "sticker");
            assert_eq!(upload.bytes, std::fs::read(path).unwrap());
            let name = path.file_name().unwrap().to_str().unwrap().to_string();
            asked.push(name.clone());
            let answer = &answers[&name];
            match answer["kind"].as_str() {
                Some("refused") => Err(crate::telegram::ApiError::Rejected(
                    answer["why"].as_str().unwrap().into(),
                )),
                Some("unknown") => Err(crate::telegram::ApiError::Transport(
                    answer["why"].as_str().unwrap().into(),
                )),
                _ => Ok(()),
            }
        },
    )
    .unwrap();
    (code, asked)
}
// tests.test_preflight_outcomes.PreflightOutcomes.test_every_file_accepted_is_the_only_success
#[test]
fn preflight_every_file_accepted_is_the_only_success() {
    let (f, plan) = preflight_fixture();
    let (code, asked) = checked_preflight(&f, &plan, &json!({}));
    assert_eq!(code, 0);
    assert_eq!(asked.len(), 3);
}
// tests.test_preflight_outcomes.PreflightOutcomes.test_a_total_transport_failure_is_never_success
#[test]
fn preflight_a_total_transport_failure_is_never_success() {
    let (f, plan) = preflight_fixture();
    let answers = json!({"i0.png":{"kind":"unknown","why":"connection reset by peer"},"i1.png":{"kind":"unknown","why":"connection reset by peer"},"i2.png":{"kind":"unknown","why":"connection reset by peer"}});
    let (code, asked) = checked_preflight(&f, &plan, &answers);
    assert_eq!(asked.len(), 3);
    assert_ne!(code, 0);
    assert_eq!(code, 3);
}
// tests.test_preflight_outcomes.PreflightOutcomes.test_a_partial_transport_failure_is_not_success_either
#[test]
fn preflight_a_partial_transport_failure_is_not_success_either() {
    let (f, plan) = preflight_fixture();
    let (code, _) = checked_preflight(
        &f,
        &plan,
        &json!({"i1.png":{"kind":"unknown","why":"timed out"}}),
    );
    assert_eq!(code, 3);
}
// tests.test_preflight_outcomes.PreflightOutcomes.test_a_telegram_refusal_fails
#[test]
fn preflight_a_telegram_refusal_fails() {
    let (f, plan) = preflight_fixture();
    let (code, _) = checked_preflight(
        &f,
        &plan,
        &json!({"i2.png":{"kind":"refused","why":"Bad Request: wrong file type"}}),
    );
    assert_eq!(code, 4);
}
// tests.test_preflight_outcomes.PreflightOutcomes.test_a_missing_file_fails_and_is_never_sent
#[test]
fn preflight_a_missing_file_fails_and_is_never_sent() {
    let (f, plan) = preflight_fixture();
    std::fs::remove_file(f.data.join("media/i0.png")).unwrap();
    let (code, asked) = checked_preflight(&f, &plan, &json!({}));
    assert_eq!(code, 4);
    assert!(!asked.contains(&"i0.png".into()));
}
// tests.test_preflight_outcomes.PreflightOutcomes.test_a_refusal_outranks_an_unknown
#[test]
fn preflight_a_refusal_outranks_an_unknown() {
    let (f, plan) = preflight_fixture();
    let (code, _) = checked_preflight(
        &f,
        &plan,
        &json!({"i0.png":{"kind":"refused","why":"Bad Request: STICKER_VIDEO_LONG"},"i1.png":{"kind":"unknown","why":"connection reset"}}),
    );
    assert_eq!(code, 4);
}
// tests.test_preflight_outcomes.PreflightOutcomes.test_an_empty_queue_is_not_a_validation
#[test]
fn preflight_an_empty_queue_is_not_a_validation() {
    let (f, _) = preflight_fixture();
    let (code, asked) = checked_preflight(&f, &json!({"static":[]}), &json!({}));
    assert_eq!(code, 0);
    assert!(asked.is_empty());
}
// tests.test_preflight_outcomes.PreflightOutcomes.test_it_publishes_nothing_whatever_the_outcome
#[test]
fn preflight_it_publishes_nothing_whatever_the_outcome() {
    let (f, plan) = preflight_fixture();
    let before = f.catalog().all(None).unwrap();
    for answers in [
        json!({}),
        json!({"i0.png":{"kind":"unknown","why":"x"}}),
        json!({"i0.png":{"kind":"refused","why":"no"}}),
    ] {
        let (_, asked) = checked_preflight(&f, &plan, &answers);
        assert!(
            asked
                .iter()
                .all(|name| ["i0.png", "i1.png", "i2.png"].contains(&name.as_str()))
        );
        assert_eq!(f.catalog().all(None).unwrap(), before);
        assert!(!f.state_path().exists());
        assert!(!f.data.join("publish_testbase.json").exists());
    }
}
fn summary(
    accepted: usize,
    refused: serde_json::Value,
    missing: serde_json::Value,
    unknown: serde_json::Value,
) -> Run {
    let f = Fixture::empty();
    let bot = Bot::new(Model::ordinary());
    captured(
        &f,
        &bot,
        &[],
        &[],
        json!({"accepted":accepted,"refused":refused,"missing":missing,"unknown":unknown}),
    )
}
// tests.test_preflight_outcomes.TheSummaryMatchesTheOutcome.test_success_names_the_number_actually_accepted
#[test]
fn preflight_summary_success_names_the_number_actually_accepted() {
    let run = summary(3, json!([]), json!([]), json!([]));
    assert_eq!(run.code, 0);
    assert!(run.out.contains("3 accepted"));
    assert!(run.out.contains("All 3 queued file(s) are acceptable"));
}
// tests.test_preflight_outcomes.TheSummaryMatchesTheOutcome.test_an_unknown_never_prints_an_acceptance_claim
#[test]
fn preflight_summary_an_unknown_never_prints_an_acceptance_claim() {
    let run = summary(
        0,
        json!([]),
        json!([]),
        json!([["k", "n.png", "timed out"]]),
    );
    assert_eq!(run.code, 3);
    assert!(!run.out.contains("acceptable to Telegram"));
    assert!(run.out.contains("1 not checked"));
    assert!(run.out.contains("proves nothing about them"));
}
// tests.test_preflight_outcomes.TheSummaryMatchesTheOutcome.test_each_category_is_reported_separately
#[test]
fn preflight_summary_each_category_is_reported_separately() {
    let run = summary(
        1,
        json!([["a", "a.png", "refused"]]),
        json!([["b", "b.png"]]),
        json!([["c", "c.png", "timeout"]]),
    );
    assert_eq!(run.code, 4);
    for text in [
        "1 accepted",
        "1 refused",
        "1 missing",
        "1 not checked",
        "REFUSED a",
        "MISSING b",
        "UNCHECKED c",
    ] {
        assert!(run.out.contains(text), "missing {text}: {}", run.out);
    }
}

fn frame(visible: usize) -> Vec<u8> {
    let mut raw = vec![0; crate::codec_video_timeline::FRAME_BYTES];
    for i in 0..visible {
        raw[i * 4 + 3] = 255;
    }
    raw
}
fn probe(raw: &[u8], hang: bool) -> Run {
    let f = Fixture::empty();
    let bin = f.root.0.join("probe-bin");
    std::fs::create_dir(&bin).unwrap();
    let ffmpeg = bin.join(if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    });
    let build = f.root.0.join("probe-build");
    std::fs::create_dir(&build).unwrap();
    let mut cmd = Command::new(env!("NUMERA_BASELINE_RUSTC"));
    cmd.arg("--edition=2024")
        .arg(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/baseline_publication_support/probe_fixture.rs"),
        )
        .arg("-o")
        .arg(&ffmpeg)
        .stdout(Stdio::from(
            std::fs::File::create(build.join("stdout")).unwrap(),
        ))
        .stderr(Stdio::from(
            std::fs::File::create(build.join("stderr")).unwrap(),
        ));
    let mut compiler = OwnedProcess::spawn_session(&mut cmd, build.clone()).unwrap();
    let status = runtime()
        .block_on(compiler.wait(Duration::from_secs(60), Duration::from_secs(60)))
        .unwrap();
    let error = std::fs::read_to_string(build.join("stderr")).unwrap();
    compiler.finish().unwrap();
    assert!(status.success(), "probe fixture compile failed: {error}");
    std::fs::copy(
        &ffmpeg,
        bin.join(if cfg!(windows) {
            "ffprobe.exe"
        } else {
            "ffprobe"
        }),
    )
    .unwrap();
    let source = f.data.join("clip.webm");
    std::fs::write(&source, b"\x1a\x45\xdf\xa3publication probe fixture").unwrap();
    let frames = f.root.0.join("frames.rgba");
    std::fs::write(&frames, raw).unwrap();
    let calls = f.root.0.join("probe-calls.txt");
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    let path = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(&inherited)))
        .unwrap();
    let bot = Bot::new(Model::ordinary());
    captured(
        &f,
        &bot,
        &[],
        &[],
        json!({"probe":{"path":path.to_str().unwrap(),"raw":frames,"calls":calls,"hang":hang,"source":source}}),
    )
}
// tests.test_publish_contracts.VideoBlankCheck.test_the_probe_runs_under_a_finite_wall_limit
#[test]
fn video_the_probe_runs_under_a_finite_wall_limit() {
    let raw = frame(900).repeat(2);
    let run = probe(&raw, false);
    assert_eq!(run.code, 0);
    let receipt = &run.receipt["probe"];
    assert!(
        receipt["calls"]
            .as_str()
            .unwrap()
            .lines()
            .any(|line| line.contains("|ffmpeg|"))
    );
    assert!(receipt["wall_ms"].as_u64().unwrap() > 0);
    let hanging = probe(&raw, true);
    assert_eq!(hanging.code, 0);
    assert!(
        hanging.receipt["probe"]["elapsed_ms"].as_u64().unwrap() < 5000,
        "real hanging control exceeded finite wall bound"
    );
}
// tests.test_publish_contracts.VideoBlankCheck.test_a_hanging_ffmpeg_is_killed_and_lets_the_upload_decide
#[test]
fn video_a_hanging_ffmpeg_is_killed_and_lets_the_upload_decide() {
    let run = probe(&[], true);
    assert_eq!(run.code, 0);
    assert!(
        run.receipt["probe"]["calls"]
            .as_str()
            .unwrap()
            .contains("|ffmpeg|")
    );
    assert!(run.receipt["probe"]["elapsed_ms"].as_u64().unwrap() < 5000);
}
// tests.test_publish_contracts.VideoBlankCheck.test_fade_in_video_is_accepted
#[test]
fn video_fade_in_video_is_accepted() {
    let mut raw = frame(0).repeat(2);
    raw.extend(frame(500));
    raw.extend(frame(900));
    assert_eq!(probe(&raw, false).code, 0);
}
// tests.test_publish_contracts.VideoBlankCheck.test_fully_blank_video_is_still_rejected
#[test]
fn video_fully_blank_video_is_still_rejected() {
    assert_eq!(probe(&frame(0).repeat(4), false).code, 4);
}
// tests.test_publish_contracts.VideoBlankCheck.test_a_handful_of_stray_pixels_is_still_blank
#[test]
fn video_a_handful_of_stray_pixels_is_still_blank() {
    assert_eq!(probe(&frame(3).repeat(3), false).code, 4);
}
// tests.test_publish_contracts.VideoBlankCheck.test_undecodable_output_lets_the_upload_decide
#[test]
fn video_undecodable_output_lets_the_upload_decide() {
    assert_eq!(probe(&[], false).code, 0);
}
