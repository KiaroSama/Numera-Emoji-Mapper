//! Linux group/session accounting; SID membership alone is never a PID identity.
use super::*;
use std::io::{ErrorKind, Read, Seek};
use std::os::unix::process::CommandExt;

unsafe extern "C" {
    fn kill(pid: i32, signal: i32) -> i32;
    fn setsid() -> i32;
}

pub(super) struct Member {
    pub(super) pid: i32,
    pub(super) parent: i32,
    group: i32,
    session: i32,
    pub(super) birth: u64,
    pub(super) cpu: [u64; 2],
    pub(super) live: bool,
    zombie: bool,
}

pub struct Tree {
    group: i32,
    session: Option<Session>,
    descendants: Option<super::descendants::Descendants>,
}

fn process_stat<T>(result: std::io::Result<T>) -> Result<Option<T>, String> {
    match result {
        Ok(value) => Ok(Some(value)),
        // proc_single_show returns ESRCH when the task exits after enumeration.
        Err(error) if error.kind() == ErrorKind::NotFound || error.raw_os_error() == Some(3) => {
            Ok(None)
        }
        Err(error) => Err(format!(
            "cannot verify codec process membership: kind={:?} errno={:?}",
            error.kind(),
            error.raw_os_error()
        )),
    }
}

pub(super) fn member(pid: i32) -> Result<Option<Member>, String> {
    let Some(mut file) = process_stat(std::fs::File::open(format!("/proc/{pid}/stat")))? else {
        return Ok(None);
    };
    // Keep this proc inode pinned: reopening the numeric PID could inspect a replacement task.
    member_from_stat(pid, || process_stat(read_stat(&mut file)))
}

fn read_stat(file: &mut std::fs::File) -> std::io::Result<String> {
    file.rewind()?;
    let mut stat = String::new();
    file.read_to_string(&mut stat)?;
    Ok(stat)
}

#[derive(Debug)]
enum MemberParseError {
    Invalid(&'static str),
    Reaped,
}
impl From<&'static str> for MemberParseError {
    fn from(error: &'static str) -> Self {
        Self::Invalid(error)
    }
}
impl MemberParseError {
    fn message(self) -> &'static str {
        match self {
            Self::Invalid(error) => error,
            Self::Reaped => "invalid Linux process group/session identity",
        }
    }
}

fn member_from_stat(
    pid: i32,
    mut read: impl FnMut() -> Result<Option<String>, String>,
) -> Result<Option<Member>, String> {
    let Some(stat) = read()? else {
        return Ok(None);
    };
    match parse_member(pid, &stat) {
        Ok(value) => Ok(Some(value)),
        Err(MemberParseError::Reaped) => {
            // do_task_stat can retain ppid0/PGID-1/SID-1 after lock_task_sighand
            // loses a concurrent reap. This is unknown membership until the same
            // open stat inode independently confirms ENOENT/ESRCH on one reread.
            if read()?.is_none() {
                Ok(None)
            } else {
                Err(MemberParseError::Reaped.message().into())
            }
        }
        Err(error) => Err(error.message().into()),
    }
}

fn parse_member(pid: i32, stat: &str) -> Result<Member, MemberParseError> {
    let (head, fields) = stat
        .rsplit_once(") ")
        .ok_or("malformed Linux process accounting")?;
    if head
        .split_whitespace()
        .next()
        .and_then(|value| value.parse::<i32>().ok())
        != Some(pid)
    {
        return Err("Linux process accounting returned another PID".into());
    }
    let fields: Vec<&str> = fields.split_whitespace().collect();
    if fields.len() < 20 {
        return Err("incomplete Linux process accounting".into());
    }
    if pid <= 0
        || !matches!(
            fields[0],
            "R" | "S" | "D" | "Z" | "T" | "t" | "X" | "x" | "K" | "W" | "P" | "I"
        )
    {
        return Err("invalid Linux process identity/state".into());
    }
    let value = Member {
        pid,
        parent: fields[1]
            .parse()
            .map_err(|_| "invalid Linux process parent")?,
        group: fields[2]
            .parse()
            .map_err(|_| "invalid Linux process group")?,
        session: fields[3]
            .parse()
            .map_err(|_| "invalid Linux process session")?,
        birth: fields[19]
            .parse()
            .map_err(|_| "invalid process birth time")?,
        cpu: [
            fields[11].parse().map_err(|_| "invalid process CPU time")?,
            fields[12].parse().map_err(|_| "invalid process CPU time")?,
        ],
        live: !matches!(fields[0], "Z" | "X" | "x"),
        zombie: fields[0] == "Z",
    };
    // Classify only the exact sentinel after validating PID, state, birth and CPU fields.
    if value.parent == 0 && value.group == -1 && value.session == -1 {
        return Err(MemberParseError::Reaped);
    }
    // Kernel threads legitimately report PGID/SID0; they cannot match our positive owner ID.
    if value.parent < 0 || value.group < 0 || value.session < 0 {
        return Err("invalid Linux process group/session identity".into());
    }
    Ok(value)
}

impl Tree {
    pub fn ready_to_reap(&self) -> Result<bool, String> {
        let Some(session) = &self.session else {
            return Ok(true);
        };
        session.before_reap(
            || {
                let root = member(self.group)?
                    .ok_or("Linux session root disappeared before verified reaping")?;
                if root.session != self.group {
                    return Err("Linux session root changed identity before reaping".into());
                }
                Ok(root.zombie)
            },
            || self.sample().map(|_| ()),
        )
    }

    pub fn root_reaped(&self) {
        if let Some(descendants) = &self.descendants {
            descendants.root_reaped();
        }
        if let Some(session) = &self.session {
            session.root_reaped();
        }
    }

    fn contains(&self, value: &Member) -> bool {
        if self.session.is_some() {
            value.session == self.group
        } else {
            value.group == self.group
        }
    }

    fn members(&self) -> Result<Vec<Member>, String> {
        let mut values = Vec::new();
        let entries =
            std::fs::read_dir("/proc").map_err(|_| "codec ownership requires Linux /proc")?;
        for entry in entries {
            let entry = entry.map_err(|_| "cannot enumerate codec processes")?;
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<i32>().ok())
            else {
                continue;
            };
            if let Some(value) = member(pid)?
                && self.contains(&value)
            {
                values.push(value);
            }
        }
        values.sort_by_key(|value| value.pid);
        if let Some(session) = &self.session {
            // Keep zombie root identity until try_wait reaps it; descendants cannot
            // join a new session with the same ID while that original ID is held.
            let identities: Vec<_> = values
                .iter()
                .map(|value| (value.pid, [value.birth, 0]))
                .collect();
            session.admit(&identities, |pid| {
                Ok(member(pid)?
                    .filter(|value| value.session == self.group)
                    .map(|value| [value.birth, 0]))
            })?;
        }
        Ok(values)
    }

    pub fn sample(&self) -> Result<Vec<u64>, String> {
        if let Some(descendants) = &self.descendants {
            return descendants.sample();
        }
        Ok(self
            .members()?
            .into_iter()
            .filter(|value| value.live)
            .flat_map(|value| [value.pid as u64, value.birth, value.cpu[0], value.cpu[1]])
            .collect())
    }

    pub fn interrupt(&self) -> Result<(), String> {
        if self.session.is_none() {
            return Err("console interruption requires an owned session".into());
        }
        self.signal_session(2)
    }

    fn signal_session(&self, signal: i32) -> Result<(), String> {
        for expected in self.members()?.into_iter().filter(|value| value.live) {
            let Some(current) = member(expected.pid)? else {
                continue;
            };
            if current.birth != expected.birth || current.session != self.group || !current.live {
                continue;
            }
            // SAFETY: fresh birth+SID recheck; never signal a foreign process group.
            if unsafe { kill(current.pid, signal) } != 0
                && std::io::Error::last_os_error().raw_os_error() != Some(3)
            {
                return Err("cannot signal verified codec session member".into());
            }
        }
        Ok(())
    }

    pub fn terminate(&self) -> Result<(), String> {
        if let Some(descendants) = &self.descendants {
            return descendants.terminate(self.group);
        }
        if self.session.is_some() {
            // POSIX lacks identity-conditional kill; fresh birth+SID checks are not
            // claimed as kernel-handle pinning or an OS sandbox.
            return self.signal_session(9);
        }
        if self.members()?.iter().all(|value| !value.live) {
            return Ok(());
        }
        // Group mode keeps its established PGID behavior; session mode never
        // sends a numeric -PGID signal across nested groups.
        if unsafe { kill(-self.group, 9) } != 0
            && std::io::Error::last_os_error().raw_os_error() != Some(3)
        {
            return Err("cannot terminate codec process group".into());
        }
        Ok(())
    }

    pub fn quiescent(&self) -> Result<bool, String> {
        if let Some(descendants) = &self.descendants {
            return descendants.quiescent();
        }
        Ok(self.sample()?.is_empty())
    }
}

pub fn spawn(command: &mut Command, scope: Scope) -> Result<(Child, Tree), String> {
    member(std::process::id() as i32)?
        .ok_or("Linux codec lifecycle requires readable /proc accounting")?;
    let descendants = if scope == Scope::Descendants {
        Some(super::descendants::Descendants::new()?)
    } else {
        None
    };
    if matches!(scope, Scope::Session | Scope::Console) {
        // SAFETY: setsid is async-signal-safe; no allocator, locks, or Rust logging.
        // Do not request process_group(0): std sets PGID before this closure, making
        // this child a group leader and setsid fail. A preconfigured caller PGID
        // likewise refuses spawn rather than silently weakening Session to Group.
        unsafe {
            command.pre_exec(|| {
                if setsid() < 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
    } else {
        command.process_group(0);
    }
    command.env("NUMERA_CODEC_OWNED_GROUP", "1");
    let child = command
        .spawn()
        .map_err(|_| "cannot spawn required codec child".to_owned())?;
    // Linux pid_t cannot exceed positive i32. No fallible post-spawn step may
    // abandon this Child before OwnedProcess holds its cleanup marker and tree.
    let group = child.id() as i32;
    Ok((
        child,
        Tree {
            group,
            session: (matches!(scope, Scope::Session | Scope::Console))
                .then(|| Session::new(group)),
            descendants,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::{member_from_stat, parse_member, process_stat, read_stat};

    #[test]
    fn reaped_task_sentinel_requires_absence_from_the_same_stat_reader() {
        // do_task_stat leaves ppid0/PGID-1/SID-1 when lock_task_sighand loses a reap race.
        let detached = "10 (exiting) X 0 -1 -1 0 0 0 0 0 0 0 7 8 0 0 0 0 0 0 123";
        for errno in [2, 3] {
            let mut reads = 0;
            let observed = member_from_stat(10, || {
                reads += 1;
                if reads == 1 {
                    process_stat(Ok(detached.to_owned()))
                } else {
                    process_stat(Err(std::io::Error::from_raw_os_error(errno)))
                }
            });
            assert!(observed.unwrap().is_none());
            assert_eq!(reads, 2, "missing identity needs independent absence proof");
        }
    }

    #[test]
    fn unresolved_or_replaced_stat_identity_is_never_admitted_as_absence() {
        let detached = "10 (exiting) X 0 -1 -1 0 0 0 0 0 0 0 7 8 0 0 0 0 0 0 123";
        for next in [
            detached,
            "10 (replacement) S 1 10 10 0 0 0 0 0 0 0 7 8 0 0 0 0 0 0 456",
        ] {
            let mut reads = 0;
            assert!(
                member_from_stat(10, || {
                    reads += 1;
                    Ok(Some(if reads == 1 { detached } else { next }.to_owned()))
                })
                .is_err()
            );
            assert_eq!(reads, 2, "a readable identity cannot prove disappearance");
        }
        for errno in [5, 13] {
            let mut reads = 0;
            assert!(
                member_from_stat(10, || {
                    reads += 1;
                    if reads == 1 {
                        Ok(Some(detached.to_owned()))
                    } else {
                        process_stat(Err(std::io::Error::from_raw_os_error(errno)))
                    }
                })
                .is_err()
            );
            assert_eq!(reads, 2, "verification errors must not be suppressed");
        }
    }

    #[test]
    fn malformed_identity_never_uses_a_later_missing_record_as_absence_proof() {
        for stat in [
            "10 (exiting) X 0 -1 -2 0 0 0 0 0 0 0 7 8 0 0 0 0 0 0 123",
            "10 (exiting) X -1 -1 -1 0 0 0 0 0 0 0 7 8 0 0 0 0 0 0 123",
            "10 (exiting) X 0 -1 -1 0 0 0 0 0 0 0 bad 8 0 0 0 0 0 0 123",
            "11 (exiting) X 0 -1 -1 0 0 0 0 0 0 0 7 8 0 0 0 0 0 0 123",
        ] {
            let mut reads = 0;
            assert!(
                member_from_stat(10, || {
                    reads += 1;
                    Ok((reads == 1).then(|| stat.to_owned()))
                })
                .is_err()
            );
            assert_eq!(reads, 1, "malformed accounting must fail immediately");
        }
    }

    #[test]
    fn pinned_stat_inode_reports_absence_after_each_rapid_exit_is_reaped() {
        for _ in 0..128 {
            let mut child = std::process::Command::new("/bin/true").spawn().unwrap();
            let pid = child.id() as i32;
            let mut file = std::fs::File::open(format!("/proc/{pid}/stat")).unwrap();
            let before = member_from_stat(pid, || process_stat(read_stat(&mut file)))
                .unwrap()
                .unwrap();
            assert_eq!(before.pid, pid);
            assert!(before.group > 0 && before.session > 0);
            assert!(child.wait().unwrap().success());
            assert!(
                member_from_stat(pid, || process_stat(read_stat(&mut file)))
                    .unwrap()
                    .is_none(),
                "the same proc inode must report absence after reaping"
            );
        }
    }

    #[test]
    fn malformed_accounting_is_unknown_not_quiescence() {
        // Linux proc_pid_stat fields3state/5PGID/6SID/14+15CPU/22birth.
        let fields = [
            "S", "1", "10", "10", "0", "0", "0", "0", "0", "0", "0", "7", "8", "0", "0", "0", "0",
            "0", "0", "123",
        ];
        let record = |fields: &[&str]| format!("10 (name with ) spaces) {}", fields.join(" "));
        let member = parse_member(10, &record(&fields)).unwrap();
        assert_eq!(
            (
                member.pid,
                member.group,
                member.session,
                member.birth,
                member.cpu,
                member.live
            ),
            (10, 10, 10, 123, [7, 8], true)
        );
        assert!(parse_member(10, "bad").is_err());
        assert!(parse_member(11, &record(&fields)).is_err());
        assert!(parse_member(10, &record(&fields[..19])).is_err());
        for (index, bad) in [
            (0, "?"),
            (2, "-1"),
            (3, "invalid"),
            (11, "overflow"),
            (19, "18446744073709551616"),
        ] {
            let mut changed = fields;
            changed[index] = bad;
            assert!(
                parse_member(10, &record(&changed)).is_err(),
                "invalid field{index} was admitted"
            );
        }
        let mut zombie = fields;
        zombie[0] = "Z";
        let zombie = parse_member(10, &record(&zombie)).unwrap();
        assert!(!zombie.live && zombie.zombie);
        let mut exiting = fields;
        exiting[0] = "X";
        let exiting = parse_member(10, &record(&exiting)).unwrap();
        assert!(!exiting.live && !exiting.zombie);
    }
    #[test]
    fn disappeared_task_is_not_an_accounting_failure_but_permission_and_io_are() {
        assert_eq!(
            process_stat::<String>(Ok("record".into())).unwrap(),
            Some("record".into())
        );
        for errno in [2, 3] {
            assert_eq!(
                process_stat::<String>(Err(std::io::Error::from_raw_os_error(errno))).unwrap(),
                None
            );
        }
        for errno in [5, 13] {
            assert!(process_stat::<String>(Err(std::io::Error::from_raw_os_error(errno))).is_err());
        }
    }
}
