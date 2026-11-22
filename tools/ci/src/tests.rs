#[cfg(unix)]
use super::scanner::bounded;
use super::scanner::{extract_binary, find_git, parse_scanner_result, verify_archive};
use super::*;
use serde_json::json;
use std::fs;
use std::path::Path;
use std::process::Command;
#[cfg(unix)]
use std::{
    io::Write,
    process::Stdio,
    time::{Duration, Instant},
};

fn finding(identity: &(&str, &str, u64, &str)) -> Finding {
    Finding {
        rule: identity.0.into(),
        file: identity.1.into(),
        line: Some(identity.2),
        end_line: Some(identity.2),
        commit: identity.3.into(),
    }
}

#[test]
fn exact_reviewed_identities_only() {
    let all = REVIEWED.iter().map(finding).collect::<Vec<_>>();
    assert_eq!(
        partition_findings(all.clone(), |_| None),
        (vec![], REVIEWED.len())
    );
    for original in &all {
        for field in 0..6 {
            let mut changed = original.clone();
            match field {
                0 => changed.rule.push_str("-unknown"),
                1 => changed.file.push_str(".other"),
                2 => changed.line = Some(changed.line.unwrap() + 1),
                3 => changed.commit = "f".repeat(40),
                4 => changed.end_line = Some(changed.end_line.unwrap() + 1),
                _ => changed.end_line = None,
            }
            let mut input = all.clone();
            input.push(changed.clone());
            assert_eq!(
                partition_findings(input, |_| None),
                (vec![changed], REVIEWED.len())
            );
        }
    }
    assert_eq!(partition_findings(vec![], |_| None), (vec![], 0));
}

#[test]
fn malformed_or_incomplete_metadata_cannot_claim_reviewed_identity() {
    let original = REVIEWED[0];
    let spoofed: Finding = serde_json::from_value(json!({"RuleID":original.0,
        "File":original.1,"StartLine":original.2,"Commit":"f".repeat(40),
        "Fingerprint":format!("{}:{}:generic-api-key:{}",original.3,original.1,original.2)}))
    .unwrap();
    assert_eq!(
        partition_findings(vec![spoofed.clone()], |_| None),
        (vec![spoofed], 0)
    );
    for line in [json!("227"), json!(true), json!(-1), json!(227.0)] {
        let spoofed =
            json!({"RuleID":original.0,"File":original.1,"StartLine":line,"Commit":original.3});
        assert!(serde_json::from_value::<Finding>(spoofed).is_err());
    }
    for missing in ["RuleID", "File", "StartLine", "Commit"] {
        let mut value = json!({"RuleID":original.0,"File":original.1,"StartLine":original.2,"Commit":original.3});
        value.as_object_mut().unwrap().remove(missing);
        let incomplete: Finding = serde_json::from_value(value).unwrap();
        assert_eq!(
            partition_findings(vec![incomplete.clone()], |_| None),
            (vec![incomplete], 0)
        );
    }
}

#[test]
fn merge_origin_requires_valid_single_line_metadata_and_exact_reviewed_origin() {
    let original = finding(&REVIEWED[0]);
    let mut repeated = original.clone();
    repeated.commit = "a".repeat(40);
    assert_eq!(
        partition_findings(vec![repeated.clone()], |_| Some(original.identity())),
        (vec![], 1)
    );
    assert_eq!(
        partition_findings(vec![repeated.clone()], |_| None),
        (vec![repeated.clone()], 0)
    );
    let mut wrong = original.identity();
    wrong.line += 1;
    assert_eq!(
        partition_findings(vec![repeated.clone()], |_| Some(wrong.clone())),
        (vec![repeated.clone()], 0)
    );
    for invalid in 0..7 {
        let mut changed = repeated.clone();
        match invalid {
            0 => changed.end_line = Some(changed.line.unwrap() + 1),
            1 => {
                changed.line = Some(0);
                changed.end_line = Some(0);
            }
            2 => changed.commit = "A".repeat(40),
            3 => changed.commit = "a".repeat(39),
            4 => changed.commit = "g".repeat(40),
            5 => changed.file.push_str(".other"),
            _ => changed.rule.push_str(".other"),
        }
        let mut called = false;
        assert_eq!(
            partition_findings(vec![changed.clone()], |_| {
                called = true;
                Some(original.identity())
            }),
            (vec![changed], 0)
        );
        assert!(
            !called,
            "invalid metadata must never ask Git for an exclusion"
        );
    }
}

#[test]
fn summary_escapes_metadata_and_discards_secret_bearing_fields() {
    let value = json!({"RuleID":"rule|<script>&\"'\r\n", "File":"path|<img>","StartLine":7,
        "Commit":"commit|<svg>","Secret":"SYNTHETIC-DO-NOT-DISCLOSE", "Match":"SYNTHETIC-SNIPPET",
        "Message":"SYNTHETIC-COMMIT-MESSAGE", "Author":"SYNTHETIC-AUTHOR"});
    let parsed: Finding = serde_json::from_value(value).unwrap();
    let output = summary(&[parsed], PR_SCOPE, 2);
    assert!(output.contains("rule&#124;&lt;script&gt;&amp;&quot;&#x27;  "));
    assert!(output.contains("path&#124;&lt;img&gt;"));
    assert!(output.contains("Excluded **2**"));
    assert!(!output.contains("SYNTHETIC"));
    assert!(!output.contains("<script>"));
    assert_eq!(safe_cell("plain"), "plain");
    let long = vec![finding(&REVIEWED[0]); 201];
    let long_summary = summary(&long, FULL_SCOPE, 0);
    assert_eq!(
        long_summary.matches("<code>generic-api-key</code>").count(),
        200
    );
    assert!(long_summary.contains("Showing 200 of 201 findings"));
    assert!(summary(&[], FULL_SCOPE, 0).contains("No unreviewed secret matches remain"));
}

#[test]
fn scanner_status_output_git_status_and_report_must_agree() {
    assert!(
        parse_scanner_result(Some(0), b" \n", b"", b"0", b"[]")
            .unwrap()
            .is_empty()
    );
    let report = br#"[{"RuleID":"unknown","Secret":"SYNTHETIC-PRIVATE"}]"#;
    assert_eq!(
        parse_scanner_result(Some(2), b"", b"", b"0", report)
            .unwrap()
            .len(),
        1
    );
    for (code, stdout, stderr, git_status, report) in [
        (
            Some(1),
            b"".as_slice(),
            b"".as_slice(),
            b"0".as_slice(),
            b"[]".as_slice(),
        ),
        (None, b"", b"", b"0", b"[]"),
        (Some(0), b"SYNTHETIC-PRIVATE", b"", b"0", b"[]"),
        (Some(0), b"", b"SYNTHETIC-PRIVATE", b"0", b"[]"),
        (Some(0), b"", b"", b"1", b"[]"),
        (Some(0), b"", b"", b"", b"[]"),
        (Some(0), b"", b"", b"0", b"null"),
        (Some(0), b"", b"", b"0", b"[1]"),
        (Some(2), b"", b"", b"0", b"[]"),
        (Some(0), b"", b"", b"0", report),
    ] {
        let error = parse_scanner_result(code, stdout, stderr, git_status, report).unwrap_err();
        assert!(!error.contains("SYNTHETIC"));
    }
}

fn archive(entries: &[(&str, tar::EntryType)]) -> Vec<u8> {
    let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    for (name, kind) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(*kind);
        header.set_mode(0o700);
        header.set_size(4);
        if kind.is_symlink() {
            header.set_link_name("/outside").unwrap();
        }
        header.set_cksum();
        builder
            .append_data(&mut header, name, b"test".as_slice())
            .unwrap();
    }
    builder.into_inner().unwrap().finish().unwrap()
}

#[test]
fn archive_extraction_selects_only_one_regular_expected_member() {
    let temporary = tempfile::tempdir().unwrap();
    let binary = temporary.path().join("scanner");
    let bytes = archive(&[
        ("ignored", tar::EntryType::Regular),
        ("gitleaks", tar::EntryType::Regular),
    ]);
    extract_binary(&bytes, &binary).unwrap();
    assert_eq!(fs::read(&binary).unwrap(), b"test");
    assert!(!temporary.path().join("ignored").exists());
    assert!(
        extract_binary(&bytes, &binary).is_err(),
        "must not overwrite"
    );
    for entries in [
        vec![("other", tar::EntryType::Regular)],
        vec![("gitleaks", tar::EntryType::Symlink)],
        vec![
            ("gitleaks", tar::EntryType::Regular),
            ("gitleaks", tar::EntryType::Regular),
        ],
    ] {
        assert!(extract_binary(&archive(&entries), &temporary.path().join("refused")).is_err());
        assert!(!temporary.path().join("refused").exists());
    }
    assert!(verify_archive(b"untrusted-download").is_err());
    assert!(extract_binary(b"invalid gzip", &temporary.path().join("invalid")).is_err());
}

fn fixture_git(root: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Synthetic Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
        .env("GIT_COMMITTER_NAME", "Synthetic Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "synthetic Git fixture command failed"
    );
    String::from_utf8(output.stdout).unwrap().trim().into()
}

#[test]
fn real_git_proves_only_unchanged_merge_line_origins() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().join("repository");
    fs::create_dir(&root).unwrap();
    fixture_git(&root, &["init", "-b", "main"]);
    let file = REVIEWED[2].1;
    fs::create_dir_all(root.join(file).parent().unwrap()).unwrap();
    fs::write(
        root.join(file),
        "one\ntwo\nthree\nfour\nsynthetic reviewed content\n",
    )
    .unwrap();
    fixture_git(&root, &["add", "."]);
    fixture_git(&root, &["commit", "-m", "synthetic original"]);
    let original = fixture_git(&root, &["rev-parse", "HEAD"]);
    fixture_git(&root, &["checkout", "-b", "side"]);
    fs::write(root.join("side"), "synthetic side").unwrap();
    fixture_git(&root, &["add", "."]);
    fixture_git(&root, &["commit", "-m", "synthetic side"]);
    fixture_git(&root, &["checkout", "main"]);
    fs::write(root.join("main"), "synthetic main").unwrap();
    fixture_git(&root, &["add", "."]);
    fixture_git(&root, &["commit", "-m", "synthetic main"]);
    fixture_git(
        &root,
        &["merge", "--no-ff", "side", "-m", "synthetic merge"],
    );
    let merge = fixture_git(&root, &["rev-parse", "HEAD"]);
    let mut repeated = finding(&REVIEWED[2]);
    repeated.commit = merge;
    let git = find_git().unwrap();
    let origin =
        reviewed_merge_origin(&repeated, &root.join(".git"), &git, temporary.path()).unwrap();
    assert_eq!(
        origin,
        Identity {
            rule: repeated.rule.clone(),
            file: repeated.file.clone(),
            line: 5,
            commit: original.clone()
        }
    );
    let accepted = |identity: &Identity| {
        identity.rule == repeated.rule
            && identity.file == repeated.file
            && identity.line == 5
            && identity.commit == original
    };
    let probe = |finding: &Finding| {
        reviewed_merge_origin(finding, &root.join(".git"), &git, temporary.path())
    };
    assert_eq!(
        partition_by(vec![repeated.clone()], probe, accepted),
        (vec![], 1)
    );
    for invalid in 0..5 {
        let mut unknown = repeated.clone();
        match invalid {
            0 => unknown.commit = "f".repeat(40),
            1 => unknown.file = "side".into(),
            2 => {
                unknown.line = Some(6);
                unknown.end_line = Some(6);
            }
            3 => unknown.end_line = Some(6),
            _ => unknown.rule = "another-rule".into(),
        }
        assert_eq!(
            partition_by(vec![unknown.clone()], probe, accepted),
            (vec![unknown], 0)
        );
    }
    // Replayed new content under a new merge SHA must remain reportable.
    fixture_git(&root, &["checkout", "side"]);
    fs::write(
        root.join(file),
        "one\ntwo\nthree\nfour\nchanged synthetic content\nnew synthetic content\n",
    )
    .unwrap();
    fixture_git(&root, &["add", "."]);
    fixture_git(&root, &["commit", "-m", "changed synthetic content"]);
    let changed = fixture_git(&root, &["rev-parse", "HEAD"]);
    fixture_git(&root, &["checkout", "main"]);
    fs::write(root.join("main"), "main advanced").unwrap();
    fixture_git(&root, &["add", "."]);
    fixture_git(&root, &["commit", "-m", "main advanced"]);
    fixture_git(&root, &["checkout", "side"]);
    fixture_git(
        &root,
        &["merge", "--no-ff", "main", "-m", "changed content merge"],
    );
    let changed_merge = fixture_git(&root, &["rev-parse", "HEAD"]);
    for commit in [changed, changed_merge] {
        for line in [5, 6] {
            let mut unknown = repeated.clone();
            unknown.commit = commit.clone();
            unknown.line = Some(line);
            unknown.end_line = Some(line);
            assert_eq!(
                partition_by(vec![unknown.clone()], probe, accepted),
                (vec![unknown], 0)
            );
        }
    }
    repeated.commit = original;
    assert!(
        reviewed_merge_origin(&repeated, &root.join(".git"), &git, temporary.path()).is_none(),
        "ordinary commit is not a merge repetition"
    );
    repeated.commit = "f".repeat(40);
    assert!(
        reviewed_merge_origin(&repeated, &root.join(".git"), &git, temporary.path()).is_none(),
        "missing commit must not exclude"
    );
}

#[cfg(unix)]
fn child_fixture(mode: &str, pid_file: &Path) -> Command {
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .env_clear()
        .env(
            "NUMERA_RUNTIME_TEST_ADMISSION",
            std::env::var_os("NUMERA_RUNTIME_TEST_ADMISSION").expect("actual test admission"),
        )
        .env("NUMERA_EMOJI_MAPPER_NO_DOTENV", "1")
        .env("NUMERA_CI_BOUNDED_FIXTURE", mode)
        .env("NUMERA_CI_BOUNDED_PID", pid_file)
        .args([
            "--exact",
            "tests::bounded_child_fixture",
            "--nocapture",
            "--quiet",
        ]);
    command
}

// Technical self-exec entrypoint; never a production binary mode or an original behavior ID.
#[cfg(unix)]
#[test]
fn bounded_child_fixture() {
    let Some(mode) = std::env::var_os("NUMERA_CI_BOUNDED_FIXTURE") else {
        return;
    };
    match mode.to_str().unwrap() {
        "output" => {
            std::io::stdout().write_all(b"synthetic").unwrap();
            std::io::stdout().flush().unwrap();
            std::io::stderr().write_all(b"private").unwrap();
            std::io::stderr().flush().unwrap();
            std::process::exit(0);
        }
        "tree" => {
            let pid_file = std::env::var_os("NUMERA_CI_BOUNDED_PID").unwrap();
            let mut leaf = child_fixture("leaf", Path::new(&pid_file))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            let _ = leaf.wait();
            std::process::exit(0);
        }
        "leaf" => {
            fs::write(
                std::env::var_os("NUMERA_CI_BOUNDED_PID").unwrap(),
                std::process::id().to_string(),
            )
            .unwrap();
            std::thread::sleep(Duration::from_secs(10));
            std::process::exit(0);
        }
        _ => panic!("unknown bounded fixture role"),
    }
}

#[cfg(unix)]
#[test]
fn private_child_output_and_process_tree_are_bounded() {
    let temporary = tempfile::tempdir().unwrap();
    let pid_file = temporary.path().join("descendant.pid");
    let output = bounded(
        &mut child_fixture("output", &pid_file),
        Duration::from_secs(2),
        temporary.path(),
    )
    .unwrap();
    assert!(output.status.success());
    // The exact libtest prelude is incidental to the Rust technical entrypoint;
    // no additional payload bytes or private stderr may enter stdout.
    assert_eq!(output.stdout, b"\nrunning 1 test\nsynthetic");
    assert_eq!(output.stderr, b"private");
    let started = Instant::now();
    assert!(
        bounded(
            &mut child_fixture("tree", &pid_file),
            Duration::from_millis(200),
            temporary.path()
        )
        .is_err()
    );
    assert!(started.elapsed() < Duration::from_secs(3));
    let pid: i32 = fs::read_to_string(pid_file)
        .expect("real descendant started before timeout")
        .parse()
        .unwrap();
    #[cfg(target_os = "linux")]
    {
        // SAFETY: this only opens/observes the recorded fixture PID, never signals another owner.
        let fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) } as i32;
        if fd < 0 {
            assert_eq!(
                std::io::Error::last_os_error().raw_os_error(),
                Some(libc::ESRCH)
            );
        } else {
            // SAFETY: fd is an owned pidfd; poll observes kernel process termination.
            let mut ready = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            let observed = unsafe { libc::poll(&mut ready, 1, 1000) };
            unsafe { libc::close(fd) };
            assert_eq!(
                observed, 1,
                "owned descendant survived the bounded process-group timeout"
            );
            assert_ne!(ready.revents & libc::POLLIN, 0);
        }
    }
}
