//! Test-only pre-main admission. Included by each real libtest crate root.
//! No Rust runtime, logger, configuration or allocator is required here.
#![allow(unsafe_code)]

#[cfg(not(all(target_arch = "x86_64", any(target_os = "linux", windows))))]
compile_error!("Native test admission supports Linux/Windows x64 only");

#[ctor::ctor(unsafe)]
fn admit_before_main() {
    if !platform::benign_listing() && !platform::admitted() {
        platform::refuse();
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use core::ffi::CStr;
    const MESSAGE: &[u8] = b"ERROR: Use the isolated Rust test runner; direct test execution was refused before libtest main.\n";

    pub(super) fn benign_listing() -> bool {
        let mut bytes = [0_u8; 32768];
        // SAFETY: fixed NUL-terminated path, owned descriptor and live buffer;
        // read-only process arguments are needed before std's argument setup.
        unsafe {
            let fd = libc::open(
                c"/proc/self/cmdline".as_ptr(),
                libc::O_RDONLY | libc::O_CLOEXEC,
            );
            if fd < 0 {
                return false;
            }
            let count = libc::read(fd, bytes.as_mut_ptr().cast(), bytes.len());
            libc::close(fd);
            if count <= 0 || count as usize >= bytes.len() || bytes[count as usize - 1] != 0 {
                return false;
            }
            let mut arguments = bytes[..count as usize - 1].split(|byte| *byte == 0);
            let _ = arguments.next();
            matches!(arguments.next(), Some(b"--list" | b"--help" | b"-h"))
                && arguments.next().is_none()
        }
    }
    fn environment(name: &CStr, expected: &[u8]) -> bool {
        // SAFETY: C runtime is initialized, name is static and valid; process
        // startup has no Rust test threads mutating its environment.
        unsafe {
            let value = libc::getenv(name.as_ptr());
            !value.is_null() && CStr::from_ptr(value).to_bytes() == expected
        }
    }
    fn query(option: i32, argument: libc::c_ulong) -> i32 {
        // SAFETY: read-only GNU prctl query, all four variadic words supplied.
        unsafe {
            libc::prctl(
                option,
                argument,
                0 as libc::c_ulong,
                0 as libc::c_ulong,
                0 as libc::c_ulong,
            )
        }
    }
    #[repr(C)]
    struct Header {
        version: u32,
        pid: i32,
    }
    #[repr(C)]
    #[derive(Clone, Copy, PartialEq, Eq)]
    struct Capability {
        effective: u32,
        permitted: u32,
        inheritable: u32,
    }
    const _: () = {
        assert!(core::mem::size_of::<Header>() == 8);
        assert!(core::mem::size_of::<Capability>() == 12);
    };
    fn privileges() -> bool {
        let (mut uid, mut effective, mut saved, mut gid, mut effective_gid, mut saved_gid) =
            (0, 0, 0, 0, 0, 0);
        let mut header = Header {
            version: 0x2008_0522,
            pid: 0,
        };
        let empty = Capability {
            effective: 0,
            permitted: 0,
            inheritable: 0,
        };
        let mut capabilities = [empty; 2];
        // SAFETY: Linux LP64 kernel UAPI layouts match the fixed initialized
        // stack buffers; every operation inspects only the calling process.
        unsafe {
            if libc::getresuid(&mut uid, &mut effective, &mut saved) != 0
                || libc::getresgid(&mut gid, &mut effective_gid, &mut saved_gid) != 0
                || uid == 0
                || uid != effective
                || uid != saved
                || gid != effective_gid
                || gid != saved_gid
                || libc::syscall(
                    libc::SYS_capget,
                    &mut header as *mut Header,
                    capabilities.as_mut_ptr(),
                ) != 0
                || capabilities != [empty; 2]
            {
                return false;
            }
        }
        if query(libc::PR_GET_NO_NEW_PRIVS, 0) != 1 || query(libc::PR_GET_SECUREBITS, 0) != 0xef {
            return false;
        }
        for capability in 0..=64 {
            let result = query(libc::PR_CAPBSET_READ, capability);
            if result == 0 {
                continue;
            }
            // SAFETY: errno belongs to this startup thread; EINVAL terminates
            // the actual kernel capability ABI after its known base range.
            return result < 0
                && capability > 8
                && unsafe { *libc::__errno_location() } == libc::EINVAL;
        }
        false
    }
    fn private_loopback() -> bool {
        // SAFETY: libc owns terminated interface/address lists, which remain
        // live until explicitly freed. Only read-only kernel inspection occurs.
        unsafe {
            let names = libc::if_nameindex();
            if names.is_null() {
                return false;
            }
            let first = &*names;
            let valid = first.if_index != 0
                && !first.if_name.is_null()
                && CStr::from_ptr(first.if_name).to_bytes() == b"lo";
            let only = valid && {
                let end = &*names.add(1);
                end.if_index == 0 && end.if_name.is_null()
            };
            libc::if_freenameindex(names);
            if !only {
                return false;
            }
            let mut addresses = core::ptr::null_mut();
            if libc::getifaddrs(&mut addresses) != 0 {
                return false;
            }
            let mut current = addresses;
            let (mut ipv4, mut ipv6, mut valid) = (false, false, true);
            while !current.is_null() {
                let row = &*current;
                if row.ifa_name.is_null()
                    || CStr::from_ptr(row.ifa_name).to_bytes() != b"lo"
                    || row.ifa_flags & (libc::IFF_LOOPBACK | libc::IFF_UP) as u32
                        != (libc::IFF_LOOPBACK | libc::IFF_UP) as u32
                {
                    valid = false;
                    break;
                }
                if !row.ifa_addr.is_null() {
                    match i32::from((*row.ifa_addr).sa_family) {
                        libc::AF_INET => {
                            let address = &*row.ifa_addr.cast::<libc::sockaddr_in>();
                            if address.sin_addr.s_addr != u32::from_be_bytes([127, 0, 0, 1]).to_be()
                            {
                                valid = false;
                                break;
                            }
                            ipv4 = true;
                        }
                        libc::AF_INET6 => {
                            let address = &*row.ifa_addr.cast::<libc::sockaddr_in6>();
                            let mut loopback = [0_u8; 16];
                            loopback[15] = 1;
                            if address.sin6_addr.s6_addr != loopback {
                                valid = false;
                                break;
                            }
                            ipv6 = true;
                        }
                        libc::AF_PACKET => {}
                        _ => {
                            valid = false;
                            break;
                        }
                    }
                }
                current = row.ifa_next;
            }
            libc::freeifaddrs(addresses);
            valid && ipv4 && ipv6
        }
    }
    pub(super) fn admitted() -> bool {
        environment(c"NUMERA_RUNTIME_TEST_ADMISSION", b"isolated-v1")
            && environment(c"NUMERA_EMOJI_MAPPER_NO_DOTENV", b"1")
            && privileges()
            && private_loopback()
    }
    pub(super) fn refuse() -> ! {
        // SAFETY: constant bounded diagnostics and immediate process exit; no
        // Rust panic/unwind/destructor/runtime facility can run a test body.
        unsafe {
            libc::write(2, MESSAGE.as_ptr().cast(), MESSAGE.len());
            libc::_exit(2)
        }
    }
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Storage::FileSystem::WriteFile,
        System::{
            Console::{GetStdHandle, STD_ERROR_HANDLE},
            Environment::{GetCommandLineW, GetEnvironmentVariableW},
            Threading::ExitProcess,
        },
        UI::Shell::CommandLineToArgvW,
    };
    const MESSAGE:&[u8]=b"ERROR: Use the isolated Rust test runner; direct test execution was refused before libtest main.\n";
    fn wide(value: *const u16, expected: &[u8]) -> bool {
        if value.is_null() {
            return false;
        }
        // SAFETY: OS argv pointers are NUL terminated; expected bounds every
        // read and ASCII-only required values fit one UTF-16 word per byte.
        unsafe {
            expected
                .iter()
                .enumerate()
                .all(|(index, byte)| *value.add(index) == u16::from(*byte))
                && *value.add(expected.len()) == 0
        }
    }
    pub(super) fn benign_listing() -> bool {
        // SAFETY: Windows supplies the process command line; Shell32 returns
        // its own argv allocation, paired exactly once with LocalFree.
        unsafe {
            let mut count = 0;
            let argv = CommandLineToArgvW(GetCommandLineW(), &mut count);
            if argv.is_null() {
                return false;
            }
            let mut benign = false;
            if count == 2 {
                let value = *argv.add(1);
                benign = wide(value, b"--list") || wide(value, b"--help") || wide(value, b"-h");
            }
            LocalFree(argv.cast());
            benign
        }
    }
    fn environment(name: &[u16], expected: &[u8]) -> bool {
        let mut value = [0_u16; 64];
        // SAFETY: static terminated name and initialized bounded output buffer.
        let count = unsafe {
            GetEnvironmentVariableW(name.as_ptr(), value.as_mut_ptr(), value.len() as u32)
        };
        count as usize == expected.len()
            && expected
                .iter()
                .zip(value)
                .all(|(left, right)| u16::from(*left) == right)
    }
    pub(super) fn admitted() -> bool {
        const MARKER: &[u16] = &[
            78, 85, 77, 69, 82, 65, 95, 82, 85, 78, 84, 73, 77, 69, 95, 84, 69, 83, 84, 95, 65, 68,
            77, 73, 83, 83, 73, 79, 78, 0,
        ];
        const DOTENV: &[u16] = &[
            78, 85, 77, 69, 82, 65, 95, 69, 77, 79, 74, 73, 95, 77, 65, 80, 80, 69, 82, 95, 78, 79,
            95, 68, 79, 84, 69, 78, 86, 0,
        ];
        environment(MARKER, b"isolated-v1") && environment(DOTENV, b"1")
    }
    pub(super) fn refuse() -> ! {
        // SAFETY: fixed diagnostic, OS stderr handle and immediate termination;
        // no Rust standard runtime services are used before libtest main.
        unsafe {
            let mut written = 0;
            WriteFile(
                GetStdHandle(STD_ERROR_HANDLE),
                MESSAGE.as_ptr(),
                MESSAGE.len() as u32,
                &mut written,
                core::ptr::null_mut(),
            );
            ExitProcess(2)
        }
    }
}
