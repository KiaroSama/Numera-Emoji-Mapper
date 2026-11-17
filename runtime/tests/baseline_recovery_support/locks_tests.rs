use crate::support::*;
use numera_runtime::locks::{self, Lease};
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
fn lock() -> (Scratch, PathBuf) {
    let s = Scratch::new();
    let p = s.0.join("state.json.lock");
    (s, p)
}
fn busy(path: &Path) -> io::Error {
    match Lease::acquire(path, "second") {
        Ok(_) => panic!("second publisher acquired held lease"),
        Err(e) => {
            // Lease preserves the native error kind; Windows lock violation33
            // need not share POSIX's WouldBlock mapping in this Rust version.
            let expected = if cfg!(windows) {
                io::Error::from_raw_os_error(33).kind()
            } else {
                io::ErrorKind::WouldBlock
            };
            assert_eq!(e.kind(), expected);
            let message = e.to_string();
            assert!(message.starts_with("OS lock acquisition failed:"));
            assert!(message.contains("Refusing to mutate"));
            e
        }
    }
}
fn old(path: &Path, seconds: u64) {
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_times(
            std::fs::FileTimes::new()
                .set_modified(SystemTime::now() - Duration::from_secs(seconds)),
        )
        .unwrap();
}
fn stale(path: &Path, age: u64) {
    std::fs::write(
        path,
        json!({"token":"gone","pid":999999999u32,"started":"2020-01-01 00:00:00 UTC"}).to_string(),
    )
    .unwrap();
    old(path, age)
}
fn unwind(path: &Path) {
    let caught = std::panic::catch_unwind(|| {
        let _held = Lease::acquire(path, "panic").unwrap();
        panic!("original exceptional exit")
    });
    assert!(caught.is_err());
    drop(Lease::acquire(path, "next").unwrap());
}
fn seconds() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs_f64()
}
fn child_branch() -> bool {
    let Some(path) = std::env::var_os("NUMERA_RECOVERY_CHILD_LOCK") else {
        return false;
    };
    let journal = std::env::var_os("NUMERA_RECOVERY_CHILD_JOURNAL").unwrap();
    let start = std::env::var("NUMERA_RECOVERY_CHILD_START")
        .unwrap()
        .parse::<f64>()
        .unwrap();
    while seconds() < start {
        std::thread::sleep(Duration::from_millis(2));
    }
    let note = |what: &str| {
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&journal)
            .unwrap();
        writeln!(
            f,
            "{}",
            json!({"what":what,"pid":std::process::id(),"t":seconds()})
        )
        .unwrap();
    };
    match Lease::acquire(Path::new(&path), "real child") {
        Ok(_lease) => {
            note("enter");
            if std::env::var("NUMERA_RECOVERY_CHILD_CRASH").as_deref() == Ok("1") {
                std::process::exit(9)
            }
            std::thread::sleep(Duration::from_secs_f64(
                std::env::var("NUMERA_RECOVERY_CHILD_HOLD")
                    .unwrap()
                    .parse()
                    .unwrap(),
            ));
            note("exit");
        }
        Err(error) => {
            note("busy");
            eprintln!("{error}");
            std::process::exit(3);
        }
    };
    true
}
struct Harness {
    scratch: Scratch,
    path: PathBuf,
    journals: Vec<PathBuf>,
    children: Vec<Child>,
}
impl Harness {
    fn new() -> Self {
        let (scratch, path) = lock();
        Self {
            scratch,
            path,
            journals: vec![],
            children: vec![],
        }
    }
    fn spawn(&mut self, hold: f64, start: f64, crash: bool) {
        let journal = self
            .scratch
            .0
            .join(format!("journal-{}.jsonl", self.children.len()));
        std::fs::write(&journal, b"").unwrap();
        let test = std::thread::current().name().unwrap().to_owned();
        let c = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &test, "--nocapture"])
            .env("NUMERA_RECOVERY_CHILD_LOCK", &self.path)
            .env("NUMERA_RECOVERY_CHILD_JOURNAL", &journal)
            .env("NUMERA_RECOVERY_CHILD_START", start.to_string())
            .env("NUMERA_RECOVERY_CHILD_HOLD", hold.to_string())
            .env("NUMERA_RECOVERY_CHILD_CRASH", if crash { "1" } else { "0" })
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        self.journals.push(journal);
        self.children.push(c);
    }
    fn wait(&mut self) -> Vec<std::process::Output> {
        let deadline = Instant::now() + Duration::from_secs(60);
        for c in &mut self.children {
            while c.try_wait().unwrap().is_none() {
                assert!(Instant::now() < deadline, "lock child timeout");
                std::thread::sleep(Duration::from_millis(5));
            }
        }
        self.children
            .drain(..)
            .map(|c| c.wait_with_output().unwrap())
            .collect()
    }
    fn spans(&self) -> Vec<(u64, f64, f64)> {
        let mut spans = std::collections::BTreeMap::<u64, (Option<f64>, f64)>::new();
        for p in &self.journals {
            for l in std::fs::read_to_string(p).unwrap().lines() {
                let v: Value = serde_json::from_str(l).unwrap();
                let t = v["t"].as_f64().unwrap();
                let s = spans
                    .entry(v["pid"].as_u64().unwrap())
                    .or_insert((None, f64::INFINITY));
                match v["what"].as_str().unwrap() {
                    "enter" => s.0 = Some(t),
                    "exit" => s.1 = t,
                    "busy" => (),
                    _ => panic!("unknown journal"),
                }
            }
        }
        spans
            .into_iter()
            .filter_map(|(p, (a, b))| a.map(|a| (p, a, b)))
            .collect()
    }
    fn entered(&self) {
        let end = Instant::now() + Duration::from_secs(20);
        while self.spans().is_empty() {
            assert!(Instant::now() < end, "holder never acquired");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
impl Drop for Harness {
    fn drop(&mut self) {
        for c in &mut self.children {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}
fn no_overlap(mut v: Vec<(u64, f64, f64)>) -> Result<(), String> {
    v.sort_by(|a, b| a.1.total_cmp(&b.1));
    for p in v.windows(2) {
        let overlap = p[0].2.min(p[1].2) - p[0].1.max(p[1].1);
        if overlap > 0.0 {
            return Err(format!(
                "pids {} and {} both held the lock for {overlap:.3}s",
                p[0].0, p[1].0
            ));
        }
    }
    Ok(())
}

// Original: tests.test_pack_locks.CanonicalMapLock.test_all_callers_get_the_same_path
#[test]
fn canonicalmaplock_test_all_callers_get_the_same_path() {
    let (s, _) = lock();
    let a = locks::canonical_map_path(&s.0);
    let b = locks::canonical_map_path(&s.0);
    assert_eq!(a, b);
    assert_eq!(a, s.0.join(".locks/canonical_map.lock"));
    let expected = std::collections::BTreeSet::from([
        "coin_manifest_cli".to_string(),
        "coin_map_cli".into(),
        "coin_provider_recover".into(),
        "coin_rebuild_map_disk".into(),
        "coin_remap".into(),
        "coin_replace_disk".into(),
        "coin_replace_effects".into(),
    ]);
    let mut callers = std::collections::BTreeSet::new();
    for (name, source) in order::sources() {
        let calls = order::call_paths(&source);
        if calls.contains(&vec![
            "crate".into(),
            "locks".into(),
            "canonical_map_path".into(),
        ]) {
            callers.insert(name.clone());
        }
        if name != "locks" {
            assert!(
                !order::string_literals(&source)
                    .iter()
                    .any(|s| s.contains("canonical_map.lock")),
                "{name} bypasses shared map coordinate"
            );
        }
    }
    assert_eq!(callers, expected);
}

// Original: tests.test_pack_locks.CanonicalMapLock.test_it_actually_excludes
#[test]
fn canonicalmaplock_test_it_actually_excludes() {
    let (s, _) = lock();
    let p = locks::canonical_map_path(&s.0);
    let _h = Lease::acquire(&p, "map writer").unwrap();
    busy(&p);
}

// Original: tests.test_pack_locks.CanonicalMapLock.test_it_is_not_the_pack_family_lock
#[test]
fn canonicalmaplock_test_it_is_not_the_pack_family_lock() {
    let (s, _) = lock();
    let _map = Lease::acquire(&locks::canonical_map_path(&s.0), "map writer").unwrap();
    let _pack =
        Lease::acquire(&locks::pack_family_path(&s.0, "cryptoemoji"), "pack writer").unwrap();
}

// Original: tests.test_pack_locks.LockOwnership.test_a_dead_holder_is_reclaimed
#[test]
fn lockownership_test_a_dead_holder_is_reclaimed() {
    let (_s, p) = lock();
    stale(&p, 10000);
    drop(Lease::acquire(&p, "next").unwrap());
}

// Original: tests.test_pack_locks.LockOwnership.test_a_killed_run_can_resume_within_minutes_not_hours
#[test]
fn lockownership_test_a_killed_run_can_resume_within_minutes_not_hours() {
    let (_s, p) = lock();
    stale(&p, 300);
    drop(Lease::acquire(&p, "next").unwrap());
}

// Original: tests.test_pack_locks.LockOwnership.test_a_live_holder_is_never_reclaimed_however_old
#[test]
fn lockownership_test_a_live_holder_is_never_reclaimed_however_old() {
    let (_s, p) = lock();
    let _h = Lease::acquire(&p, "holder").unwrap();
    old(&p, 10 * 24 * 3600);
    busy(&p);
}

// Original: tests.test_pack_locks.LockOwnership.test_a_reclaimed_lock_is_not_deleted_by_the_old_holder
#[test]
fn lockownership_test_a_reclaimed_lock_is_not_deleted_by_the_old_holder() {
    let (_s, p) = lock();
    let h = Lease::acquire(&p, "holder").unwrap();
    let bytes = json!({"token":"other","pid":4242,"started":"now"}).to_string();
    std::fs::write(&p, &bytes).unwrap();
    drop(h);
    assert_eq!(std::fs::read_to_string(&p).unwrap(), bytes);
}

// Original: tests.test_pack_locks.LockOwnership.test_an_unwritten_record_never_reads_as_a_free_lock
#[test]
fn lockownership_test_an_unwritten_record_never_reads_as_a_free_lock() {
    let (_s, p) = lock();
    std::fs::write(&p, b"").unwrap();
    let _h = Lease::acquire(&p, "holder").unwrap();
    std::fs::write(&p, b"").unwrap();
    busy(&p);
}

// Original: tests.test_pack_locks.PackFamilyLock.test_different_bases_do_not_collide
#[test]
fn packfamilylock_test_different_bases_do_not_collide() {
    let (s, _) = lock();
    assert_ne!(
        locks::pack_family_path(&s.0, "one"),
        locks::pack_family_path(&s.0, "two")
    );
}

// Original: tests.test_pack_locks.PackFamilyLock.test_same_base_yields_the_same_path
#[test]
fn packfamilylock_test_same_base_yields_the_same_path() {
    let (s, _) = lock();
    assert_eq!(
        locks::pack_family_path(&s.0, "cryptoemoji"),
        locks::pack_family_path(&s.0, "cryptoemoji")
    );
}

// Original: tests.test_pack_locks.PackFamilyLock.test_unsafe_characters_are_normalised
#[test]
fn packfamilylock_test_unsafe_characters_are_normalised() {
    let (s, _) = lock();
    let p = locks::pack_family_path(&s.0, "../../etc/passwd");
    assert_eq!(p.parent().unwrap(), s.0.join(".locks"));
    assert!(!p.file_name().unwrap().to_str().unwrap().contains(".."));
}

// Original: tests.test_pack_locks.PublisherLock.test_a_record_rewritten_underneath_a_holder_grants_nothing
#[test]
fn publisherlock_test_a_record_rewritten_underneath_a_holder_grants_nothing() {
    let (_s, p) = lock();
    let _h = Lease::acquire(&p, "holder").unwrap();
    std::fs::write(
        &p,
        json!({"token":"someone-else","pid":1,"started":"now"}).to_string(),
    )
    .unwrap();
    busy(&p);
}

// Original: tests.test_pack_locks.PublisherLock.test_a_record_that_names_nobody_grants_nothing_either
#[test]
fn publisherlock_test_a_record_that_names_nobody_grants_nothing_either() {
    let (_s, p) = lock();
    let _h = Lease::acquire(&p, "holder").unwrap();
    std::fs::write(&p, b"").unwrap();
    busy(&p);
}

// Original: tests.test_pack_locks.PublisherLock.test_a_refusal_is_always_LockBusy_never_a_bare_oserror
#[test]
fn publisherlock_test_a_refusal_is_always_lockbusy_never_a_bare_oserror() {
    let (_s, p) = lock();
    let _h = Lease::acquire(&p, "holder").unwrap();
    let e = busy(&p);
    assert!(e.to_string().contains("OS lock acquisition failed"));
    assert!(e.to_string().contains("Refusing to mutate"));
}

// Original: tests.test_pack_locks.PublisherLock.test_lock_is_released_even_on_error
#[test]
fn publisherlock_test_lock_is_released_even_on_error() {
    let (_s, p) = lock();
    unwind(&p);
}

// Original: tests.test_pack_locks.PublisherLock.test_lock_is_released_on_exit
#[test]
fn publisherlock_test_lock_is_released_on_exit() {
    let (_s, p) = lock();
    {
        let _h = Lease::acquire(&p, "first").unwrap();
        assert!(p.exists());
    }
    assert!(p.exists());
    drop(Lease::acquire(&p, "next").unwrap());
}

// Original: tests.test_pack_locks.PublisherLock.test_second_holder_is_refused
#[test]
fn publisherlock_test_second_holder_is_refused() {
    let (_s, p) = lock();
    let _h = Lease::acquire(&p, "first").unwrap();
    busy(&p);
}

// Original: tests.test_pack_locks.PublisherLock.test_stale_lock_is_reclaimed
#[test]
fn publisherlock_test_stale_lock_is_reclaimed() {
    let (_s, p) = lock();
    std::fs::write(&p, "pid=999 (crashed)").unwrap();
    old(&p, 10000);
    drop(Lease::acquire(&p, "next").unwrap());
}

// Original: tests.test_pack_locks_exclusion.JournalRecordsAreComplete.test_separate_child_journals_keep_overlapping_owners_visible
#[test]
fn journalrecordsarecomplete_test_separate_child_journals_keep_overlapping_owners_visible() {
    let mut h = Harness::new();
    for (i, pid, a, b) in [(0, 4104, 10.0, 11.0), (1, 5312, 10.5, 11.5)] {
        let p = h.scratch.0.join(format!("journal-{i}.jsonl"));
        std::fs::write(
            &p,
            format!(
                "{}\n{}\n",
                json!({"what":"enter","pid":pid,"t":a}),
                json!({"what":"exit","pid":pid,"t":b})
            ),
        )
        .unwrap();
        h.journals.push(p);
    }
    let spans = h.spans();
    assert_eq!(spans, vec![(4104, 10.0, 11.0), (5312, 10.5, 11.5)]);
    assert!(
        no_overlap(spans)
            .unwrap_err()
            .contains("both held the lock")
    );
}

// Original: tests.test_pack_locks_exclusion.OnlyOneProcessHoldsTheLock.test_a_live_holder_is_never_displaced_however_old_the_record_looks
#[test]
fn onlyoneprocessholdsthelock_test_a_live_holder_is_never_displaced_however_old_the_record_looks() {
    if child_branch() {
        return;
    }
    let mut h = Harness::new();
    h.spawn(2.5, 0.0, false);
    h.entered();
    old(&h.path, 10000);
    busy(&h.path);
    h.wait();
    no_overlap(h.spans()).unwrap();
}

// Original: tests.test_pack_locks_exclusion.OnlyOneProcessHoldsTheLock.test_four_runs_starting_at_once_never_overlap
#[test]
fn onlyoneprocessholdsthelock_test_four_runs_starting_at_once_never_overlap() {
    if child_branch() {
        return;
    }
    let mut h = Harness::new();
    let start = seconds() + 0.5;
    for _ in 0..4 {
        h.spawn(0.35, start, false);
    }
    let o = h.wait();
    assert!(
        o.iter()
            .all(|o| [Some(0), Some(3)].contains(&o.status.code())),
        "{o:?}"
    );
    let spans = h.spans();
    assert!(!spans.is_empty());
    no_overlap(spans).unwrap();
}

// Original: tests.test_pack_locks_exclusion.OnlyOneProcessHoldsTheLock.test_two_racing_runs_over_a_stale_leftover_never_overlap
#[test]
fn onlyoneprocessholdsthelock_test_two_racing_runs_over_a_stale_leftover_never_overlap() {
    if child_branch() {
        return;
    }
    let mut h = Harness::new();
    stale(&h.path, 10000);
    let start = seconds() + 0.4;
    for _ in 0..2 {
        h.spawn(0.35, start, false);
    }
    let o = h.wait();
    assert!(
        o.iter()
            .all(|o| [Some(0), Some(3)].contains(&o.status.code())),
        "{o:?}"
    );
    let spans = h.spans();
    assert!(!spans.is_empty());
    no_overlap(spans).unwrap();
}

// Original: tests.test_pack_locks_exclusion.RefusalIsInformative.test_a_second_acquisition_in_one_process_is_refused
#[test]
fn refusalisinformative_test_a_second_acquisition_in_one_process_is_refused() {
    let (_s, p) = lock();
    let _h = Lease::acquire(&p, "holder").unwrap();
    busy(&p);
}

// Original: tests.test_pack_locks_exclusion.RefusalIsInformative.test_an_unreadable_record_still_refuses_rather_than_proceeds
#[test]
fn refusalisinformative_test_an_unreadable_record_still_refuses_rather_than_proceeds() {
    if child_branch() {
        return;
    }
    let mut h = Harness::new();
    h.spawn(2.0, 0.0, false);
    h.entered();
    std::fs::write(&h.path, "not json at all").unwrap();
    busy(&h.path);
    h.wait();
}

// Original: tests.test_pack_locks_exclusion.RefusalIsInformative.test_the_refusal_names_the_live_holder
#[test]
fn refusalisinformative_test_the_refusal_names_the_live_holder() {
    if child_branch() {
        return;
    }
    let mut h = Harness::new();
    h.spawn(2.5, 0.0, false);
    h.entered();
    let pid = h.spans()[0].0;
    let record = std::fs::read(&h.path).unwrap();
    let error = busy(&h.path);
    assert_eq!(
        std::fs::read(&h.path).unwrap(),
        record,
        "refusal changed the holder record"
    );
    let message = error.to_string();
    assert!(message.contains(&pid.to_string()), "{message}");
    assert!(message.contains("Refusing to mutate"));
    h.wait();
}

// Original: tests.test_pack_locks_exclusion.TheOperatingSystemReleasesTheLock.test_a_crashed_holder_leaves_an_acquirable_lock
#[test]
fn theoperatingsystemreleasesthelock_test_a_crashed_holder_leaves_an_acquirable_lock() {
    if child_branch() {
        return;
    }
    let mut h = Harness::new();
    h.spawn(0.35, 0.0, true);
    let o = h.wait();
    assert_eq!(o[0].status.code(), Some(9));
    assert!(h.path.exists());
    drop(Lease::acquire(&h.path, "after crash").unwrap());
}

// Original: tests.test_pack_locks_exclusion.TheOperatingSystemReleasesTheLock.test_an_exception_inside_the_block_still_releases
#[test]
fn theoperatingsystemreleasesthelock_test_an_exception_inside_the_block_still_releases() {
    let (_s, p) = lock();
    unwind(&p);
}

// Original: tests.test_pack_locks_exclusion.TheOperatingSystemReleasesTheLock.test_releasing_lets_the_next_run_in
#[test]
fn theoperatingsystemreleasesthelock_test_releasing_lets_the_next_run_in() {
    let (_s, p) = lock();
    {
        let _h = Lease::acquire(&p, "first").unwrap();
        assert!(p.exists());
    }
    assert!(p.exists());
    drop(Lease::acquire(&p, "next").unwrap());
}

// Original: tests.test_pack_locks_exclusion.TheOperatingSystemReleasesTheLock.test_the_lock_file_is_never_recreated_underneath_a_holder
#[test]
fn theoperatingsystemreleasesthelock_test_the_lock_file_is_never_recreated_underneath_a_holder() {
    let (_s, p) = lock();
    let a = {
        let _h = Lease::acquire(&p, "first").unwrap();
        std::fs::metadata(&p).unwrap()
    };
    let b = {
        let _h = Lease::acquire(&p, "second").unwrap();
        std::fs::metadata(&p).unwrap()
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!((a.ino(), a.dev()), (b.ino(), b.dev()));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        assert_eq!(a.creation_time(), b.creation_time());
    }
}

// Original: tests.test_pack_locks.LockOwnership.test_heartbeat_refreshes_the_lock
#[test]
fn lockownership_test_heartbeat_refreshes_the_lock() {
    let (_s, p) = lock();
    let held = Lease::acquire(&p, "holder").unwrap();
    old(&p, 10000);
    let before = std::fs::metadata(&p).unwrap().modified().unwrap();
    held.heartbeat().unwrap();
    assert!(
        std::fs::metadata(&p).unwrap().modified().unwrap() > before + Duration::from_secs(1000)
    );
    busy(&p);
}
