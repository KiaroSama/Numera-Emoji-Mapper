//! Fresh Linux test bootstrap: private loopback, irreversible privilege drop, then same-PID exec.
#![allow(unsafe_code)]
use crate::logging::RunLog;
use std::ffi::{CStr, OsString};
use std::io::Write;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests {
    use super::target;
    use std::ffi::OsString;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    #[test]
    fn bootstrap_arguments_require_separator_absolute_target_and_preserve_target_argv() {
        for args in [
            vec![],
            vec!["--"],
            vec!["--help"],
            vec!["--", "relative"],
            vec!["/absolute/test"],
            vec!["--", "/absolute/te\0st"],
        ] {
            let args = args.into_iter().map(OsString::from).collect::<Vec<_>>();
            assert!(target(&args).is_err());
        }
        let args = vec![
            OsString::from("--"),
            OsString::from("/absolute/test café"),
            OsString::from("--help"),
            OsString::from_vec(vec![0xff, b'x']),
        ];
        assert_eq!(
            target(&args).unwrap(),
            std::path::Path::new("/absolute/test café")
        );
        assert_eq!(args[2], "--help");
        assert_eq!(args[3].as_bytes(), &[0xff, b'x']);
        let mut invalid = args;
        invalid.push(OsString::from_vec(vec![b'a', 0]));
        assert!(target(&invalid).is_err());
    }
}

fn target(args: &[OsString]) -> Result<&Path, String> {
    if args.len() < 2 || args[0] != "--" || args.iter().any(|arg| arg.as_bytes().contains(&0)) {
        return Err(
            "test-net-bootstrap requires -- ABSOLUTE_TEST_EXECUTABLE [ARGS]; use test-runner"
                .into(),
        );
    }
    let path = Path::new(&args[1]);
    if !path.is_absolute() {
        return Err("test bootstrap executable must be absolute; use test-runner".into());
    }
    Ok(path)
}

fn failed(action: &str) -> String {
    format!(
        "{action}: {}; no test started",
        std::io::Error::last_os_error()
    )
}

fn prctl(option: i32, arg: libc::c_ulong) -> Result<i32, String> {
    // SAFETY: GNU prctl unconditionally reads all four unsigned-long variadic arguments.
    let result = unsafe {
        libc::prctl(
            option,
            arg,
            0 as libc::c_ulong,
            0 as libc::c_ulong,
            0 as libc::c_ulong,
        )
    };
    if result < 0 {
        Err(failed("cannot restrict test bootstrap privileges"))
    } else {
        Ok(result)
    }
}

fn close_inherited() -> Result<(), String> {
    // SAFETY: this entry has no shared threads or live Rust-owned descriptors above stderr.
    if unsafe { libc::syscall(libc::SYS_close_range, 3_u32, u32::MAX, 2_u32) } != 0 {
        return Err(failed(
            "cannot close inherited descriptors; use a host supporting close_range",
        ));
    }
    Ok(())
}

fn stdio(root: &Path) -> Result<(), String> {
    let null = std::fs::metadata("/dev/null")
        .map_err(|_| "cannot admit test stdin; /dev/null is unavailable")?;
    for fd in 0..=2 {
        // SAFETY: all-zero stat is valid storage; fstat initializes it before any field is read.
        let mut info: libc::stat = unsafe { std::mem::zeroed() };
        if unsafe { libc::fstat(fd, &mut info) } != 0 {
            return Err(failed("cannot inspect test bootstrap stdio"));
        }
        // SAFETY: fcntl with F_GETFL takes no third argument and does not change the descriptor.
        let access = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if access < 0 {
            return Err(failed("cannot inspect test bootstrap stdio access"));
        }
        if fd == 0 {
            if info.st_mode & libc::S_IFMT != libc::S_IFCHR
                || info.st_rdev != null.rdev()
                || access & libc::O_ACCMODE == libc::O_WRONLY
            {
                return Err(
                    "test bootstrap stdin must be /dev/null, never a socket; use test-runner"
                        .into(),
                );
            }
        } else {
            if info.st_mode & libc::S_IFMT != libc::S_IFREG
                || access & libc::O_ACCMODE == libc::O_RDONLY
            {
                return Err(
                    "test bootstrap stdout/stderr must be writable capture files; use test-runner"
                        .into(),
                );
            }
            let path = std::fs::read_link(format!("/proc/self/fd/{fd}"))
                .map_err(|_| "cannot bind bootstrap capture to its project")?;
            crate::tooling_environment::reject_links(&path, "test capture cannot follow links")?;
            if !path.starts_with(root) {
                return Err("test capture must stay project-contained".into());
            }
            let current =
                std::fs::metadata(path).map_err(|_| "cannot inspect test capture path")?;
            if current.dev() != info.st_dev || current.ino() != info.st_ino {
                return Err("test capture descriptor does not match its path".into());
            }
        }
    }
    Ok(())
}

fn single_thread() -> Result<(), String> {
    let mut tasks =
        std::fs::read_dir("/proc/self/task").map_err(|_| "test bootstrap requires Linux procfs")?;
    let first = tasks
        .next()
        .ok_or("cannot inspect bootstrap thread")?
        .map_err(|_| "cannot inspect bootstrap thread")?;
    if first.file_name() != std::process::id().to_string().as_str() || tasks.next().is_some() {
        return Err("test bootstrap must start before threads; no test started".into());
    }
    Ok(())
}

#[repr(C)]
struct CapHeader {
    version: u32,
    pid: i32,
}
#[repr(C)]
#[derive(Clone, Copy, Default, PartialEq, Eq)]
struct CapData {
    effective: u32,
    permitted: u32,
    inheritable: u32,
}
const _: () = {
    assert!(std::mem::size_of::<CapHeader>() == 8);
    assert!(std::mem::size_of::<CapData>() == 12);
};

fn capabilities() -> Result<[CapData; 2], String> {
    let mut header = CapHeader {
        version: 0x2008_0522,
        pid: 0,
    };
    let mut data = [CapData::default(); 2];
    // SAFETY: documented Linux capability v3 UAPI: 8-byte header, two 12-byte rows, calling thread only.
    if unsafe {
        libc::syscall(
            libc::SYS_capget,
            &mut header as *mut CapHeader,
            data.as_mut_ptr(),
        )
    } != 0
    {
        return Err(failed("cannot inspect test capability sets"));
    }
    Ok(data)
}

fn identity() -> Result<(u32, u32), String> {
    let (mut real, mut effective, mut saved) = (0, 0, 0);
    let (mut group, mut effective_group, mut saved_group) = (0, 0, 0);
    // SAFETY: live writable uid_t/gid_t storage; neither query changes identity.
    if unsafe { libc::getresuid(&mut real, &mut effective, &mut saved) } != 0
        || unsafe { libc::getresgid(&mut group, &mut effective_group, &mut saved_group) } != 0
    {
        return Err(failed("cannot inspect test bootstrap identity"));
    }
    if real == 0
        || real != effective
        || real != saved
        || group != effective_group
        || group != saved_group
        || capabilities()? != [CapData::default(); 2]
    {
        return Err(
            "test isolation requires an ordinary unprivileged identity; no test started".into(),
        );
    }
    Ok((real, group))
}

fn map_once(path: &str, bytes: &[u8]) -> Result<(), String> {
    // Kernel-owned one-shot procfs maps are not normal durable files: never atomic-replace or retry them.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|_| failed("cannot open private namespace identity map"))?;
    if file
        .write(bytes)
        .map_err(|_| failed("cannot write private namespace identity map"))?
        != bytes.len()
    {
        return Err("private namespace identity map was incomplete; no test started".into());
    }
    Ok(())
}

fn namespace(uid: u32, gid: u32) -> Result<(), String> {
    // SAFETY: fresh single-thread bootstrap; NEWUSER is created first and owns the new network namespace.
    if unsafe { libc::unshare(libc::CLONE_NEWUSER | libc::CLONE_NEWNET) } != 0 {
        return Err(failed(
            "cannot create private test namespaces; use a host permitting user/network namespaces",
        ));
    }
    map_once("/proc/self/uid_map", format!("{uid} {uid} 1\n").as_bytes())?;
    map_once("/proc/self/setgroups", b"deny\n")?;
    map_once("/proc/self/gid_map", format!("{gid} {gid} 1\n").as_bytes())?;
    // SAFETY: read-only queries after the one-entry mappings; host identity must not change.
    if unsafe { libc::getuid() } != uid
        || unsafe { libc::geteuid() } != uid
        || unsafe { libc::getgid() } != gid
        || unsafe { libc::getegid() } != gid
    {
        return Err("private namespace identity did not retain the caller; no test started".into());
    }
    Ok(())
}

fn loopback() -> Result<(), String> {
    // SAFETY: socket belongs to the already-private namespace and is never inherited through exec.
    let raw = unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 0) };
    if raw < 0 {
        return Err(failed("cannot configure private test loopback"));
    }
    let socket = unsafe { OwnedFd::from_raw_fd(raw) };
    // libc supplies the target ifreq union layout; never use a hand-sized ioctl buffer.
    let mut request: libc::ifreq = unsafe { std::mem::zeroed() };
    request.ifr_name[0] = b'l' as libc::c_char;
    request.ifr_name[1] = b'o' as libc::c_char;
    if unsafe {
        libc::ioctl(
            socket.as_raw_fd(),
            libc::SIOCGIFFLAGS,
            &mut request as *mut libc::ifreq,
        )
    } != 0
    {
        return Err(failed("cannot inspect private loopback flags"));
    }
    let flags = unsafe { request.ifr_ifru.ifru_flags };
    if flags & libc::IFF_LOOPBACK as i16 == 0 {
        return Err("private lo is not a loopback device; no test started".into());
    }
    request.ifr_ifru.ifru_flags = flags | libc::IFF_UP as i16;
    if unsafe {
        libc::ioctl(
            socket.as_raw_fd(),
            libc::SIOCSIFFLAGS,
            &request as *const libc::ifreq,
        )
    } != 0
    {
        return Err(failed("cannot enable private test loopback"));
    }
    // Kernel NETDEV_UP installs 127.0.0.1/8 and ::1/128; require both rather than changing host sysctls.
    verify_loopback()
}

fn verify_loopback() -> Result<(), String> {
    struct Names(*mut libc::if_nameindex);
    impl Drop for Names {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { libc::if_freenameindex(self.0) };
            }
        }
    }
    let names = Names(unsafe { libc::if_nameindex() });
    if names.0.is_null() {
        return Err(failed("cannot enumerate private interfaces"));
    }
    // SAFETY: successful libc call returns a zero/null-terminated array. Require exactly one interface.
    let only = unsafe { &*names.0 };
    if only.if_index == 0
        || only.if_name.is_null()
        || unsafe { CStr::from_ptr(only.if_name) }.to_bytes() != b"lo"
    {
        return Err("private test network must contain only lo; no test started".into());
    }
    // A real first entry guarantees that the terminating entry is readable.
    let end = unsafe { &*names.0.add(1) };
    if end.if_index != 0 || !end.if_name.is_null() {
        return Err("private test network must contain only lo; no test started".into());
    }
    struct Addresses(*mut libc::ifaddrs);
    impl Drop for Addresses {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe { libc::freeifaddrs(self.0) };
            }
        }
    }
    let mut addresses = Addresses(std::ptr::null_mut());
    if unsafe { libc::getifaddrs(&mut addresses.0) } != 0 {
        return Err(failed("cannot enumerate private loopback addresses"));
    }
    let mut item = addresses.0;
    let (mut ipv4, mut ipv6) = (false, false);
    for _ in 0..16 {
        if item.is_null() {
            break;
        }
        // SAFETY: entries/family-specific sockaddr storage remain owned by the successful libc list.
        let entry = unsafe { &*item };
        if entry.ifa_name.is_null()
            || unsafe { CStr::from_ptr(entry.ifa_name) }.to_bytes() != b"lo"
            || entry.ifa_flags & (libc::IFF_UP | libc::IFF_LOOPBACK) as u32
                != (libc::IFF_UP | libc::IFF_LOOPBACK) as u32
        {
            return Err("private interface is not an enabled loopback; no test started".into());
        }
        if !entry.ifa_addr.is_null() {
            let family = unsafe { (*entry.ifa_addr).sa_family } as i32;
            match family {
                libc::AF_INET => {
                    if ipv4 || entry.ifa_netmask.is_null() {
                        return Err("ambiguous private IPv4 loopback; no test started".into());
                    }
                    let address = unsafe { &*entry.ifa_addr.cast::<libc::sockaddr_in>() };
                    let mask = unsafe { &*entry.ifa_netmask.cast::<libc::sockaddr_in>() };
                    if address.sin_addr.s_addr.to_ne_bytes() != [127, 0, 0, 1]
                        || mask.sin_addr.s_addr.to_ne_bytes() != [255, 0, 0, 0]
                    {
                        return Err("unexpected private IPv4 address; no test started".into());
                    }
                    ipv4 = true;
                }
                libc::AF_INET6 => {
                    if ipv6 || entry.ifa_netmask.is_null() {
                        return Err("ambiguous private IPv6 loopback; no test started".into());
                    }
                    let address = unsafe { &*entry.ifa_addr.cast::<libc::sockaddr_in6>() };
                    let mask = unsafe { &*entry.ifa_netmask.cast::<libc::sockaddr_in6>() };
                    if address.sin6_addr.s6_addr != std::net::Ipv6Addr::LOCALHOST.octets()
                        || mask.sin6_addr.s6_addr != [255; 16]
                    {
                        return Err("unexpected private IPv6 address; no test started".into());
                    }
                    ipv6 = true;
                }
                libc::AF_PACKET => {}
                _ => {
                    return Err("unexpected private network address family; no test started".into());
                }
            }
        }
        item = entry.ifa_next;
    }
    if !item.is_null() || !ipv4 || !ipv6 {
        return Err(
            "private test loopback requires complete IPv4 and IPv6; no test started".into(),
        );
    }
    Ok(())
}

fn drop_privileges() -> Result<(), String> {
    const LOCKED: libc::c_ulong = 0xef;
    prctl(
        libc::PR_CAP_AMBIENT,
        libc::PR_CAP_AMBIENT_CLEAR_ALL as libc::c_ulong,
    )?;
    // NOROOT/NO_SETUID_FIXUP/NO_AMBIENT_RAISE set and locked; KEEP_CAPS off and locked.
    prctl(libc::PR_SET_SECUREBITS, LOCKED)?;
    let mut count = 0;
    for capability in 0..=64 {
        let result = unsafe {
            libc::prctl(
                libc::PR_CAPBSET_READ,
                capability as libc::c_ulong,
                0 as libc::c_ulong,
                0 as libc::c_ulong,
                0 as libc::c_ulong,
            )
        };
        if result < 0 {
            if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINVAL)
                && capability > 8
            {
                break;
            }
            return Err(failed("cannot inspect test capability bounding set"));
        }
        if capability == 64 {
            return Err("test capability ABI exceeds v3; no test started".into());
        }
        prctl(libc::PR_CAPBSET_DROP, capability as libc::c_ulong)?;
        count += 1;
    }
    let header = CapHeader {
        version: 0x2008_0522,
        pid: 0,
    };
    let data = [CapData::default(); 2];
    // SAFETY: exact v3 capability UAPI; changing only the calling bootstrap thread.
    if unsafe { libc::syscall(libc::SYS_capset, &header as *const CapHeader, data.as_ptr()) } != 0 {
        return Err(failed("cannot drop test capability sets"));
    }
    if capabilities()? != data
        || prctl(libc::PR_GET_NO_NEW_PRIVS, 0)? != 1
        || prctl(libc::PR_GET_SECUREBITS, 0)? != LOCKED as i32
    {
        return Err("test privilege drop was not observed; no test started".into());
    }
    for capability in 0..count {
        if prctl(libc::PR_CAPBSET_READ, capability as libc::c_ulong)? != 0 {
            return Err("test capability bounding set remains nonempty; no test started".into());
        }
    }
    Ok(())
}

fn admitted(root: &Path, args: &[OsString]) -> Result<PathBuf, String> {
    let path = target(args)?;
    crate::tooling_environment::reject_links(path, "test executable cannot follow links")?;
    let target = crate::migration_bundle::resolved(path)?;
    let executable =
        std::env::current_exe().map_err(|_| "cannot locate test bootstrap executable")?;
    let cwd = crate::tooling_environment::physical_root(
        &std::env::current_dir().map_err(|_| "cannot inspect test bootstrap working directory")?,
    )?;
    if !cwd.starts_with(root)
        || crate::tooling_environment::physical_root(&crate::tooling_environment::project_root()?)?
            != root
        || !executable.starts_with(root)
        || !target.starts_with(root)
        || !target.is_file()
        || !root.join("assets/panel.html").is_file()
        || !root.join("pyproject.toml").is_file()
    {
        return Err("test bootstrap root/executable binding failed; use test-runner".into());
    }
    Ok(target)
}

/// Configure only this fresh Linux child, then replace it without changing the outer owner's PID.
///
/// # Safety
/// Call only at the binary's first dispatch, before threads, Python, runtime or panic-hook startup.
/// The caller must return/exit immediately on failure: partial namespace/privilege changes are irreversible.
/// This is network/namespace isolation, not filesystem or host IPC-broker containment.
pub unsafe fn run(root: &Path, args: &[OsString]) -> i32 {
    let prepare = (|| {
        if !cfg!(all(
            target_arch = "x86_64",
            target_pointer_width = "64",
            target_env = "gnu"
        )) {
            return Err("test bootstrap requires Linux GNU x86_64 LP64; no test started".into());
        }
        let root = crate::tooling_environment::physical_root(root)?;
        let executable = admitted(&root, args)?;
        single_thread()?;
        stdio(&root)?;
        let ids = identity()?;
        close_inherited()?;
        Ok::<_, String>((root, executable, ids))
    })();
    let (root, executable, (uid, gid)) = match prepare {
        Ok(value) => value,
        Err(error) => {
            eprintln!("ERROR: {}", crate::logging::redact(&error, &[]));
            return 2;
        }
    };
    let log = RunLog::open(&root, "test-network-setup", vec![]);
    let configured = (|| {
        prctl(libc::PR_SET_NO_NEW_PRIVS, 1)?;
        namespace(uid, gid)?;
        log.event(
            "INFO",
            "test-network",
            "private user/network namespaces created",
        );
        loopback()?;
        drop_privileges()?;
        log.event(
            "INFO",
            "test-network",
            "IPv4/IPv6 private loopback and privilege drop ready; target not started",
        );
        Ok::<_, String>(())
    })();
    if let Err(error) = configured {
        log.event("ERROR", "test-network", &error);
        log.finish(2);
        return 2;
    }
    // The setup log never claims the target passed. No strong clone/panic hook keeps its file open.
    log.event(
        "INFO",
        "test-network",
        "setup finished; handing the same PID to the admitted test",
    );
    log.finish(0);
    drop(log);
    if let Err(error) = close_inherited() {
        eprintln!("ERROR: {}", crate::logging::redact(&error, &[]));
        return 2;
    }
    let error = std::process::Command::new(executable)
        .args(&args[2..])
        .exec();
    eprintln!(
        "ERROR: cannot execute admitted test after isolation: {}; no test started",
        crate::logging::redact(&error.to_string(), &[])
    );
    2
}
