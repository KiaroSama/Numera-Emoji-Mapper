//! Exact legacy lock coordinates: byte4096 on Windows, flock on POSIX.
#![allow(unsafe_code)]
use std::fs::{File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::Path;

pub fn pack_family_path(root: &Path, base: &str) -> std::path::PathBuf {
    let mut safe = String::new();
    let mut invalid = false;
    for ch in base.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-') {
            safe.push(ch);
            invalid = false;
        } else if !invalid {
            safe.push('_');
            invalid = true;
        }
    }
    let safe = safe.trim_matches('_');
    root.join(".locks").join(format!(
        "pack_{}.lock",
        if safe.is_empty() { "default" } else { safe }
    ))
}

pub struct Lease {
    file: File,
}
impl Lease {
    pub fn acquire(path: &Path, started: &str) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        take(&file)?;
        let result = (|| {
            let record = serde_json::to_vec(
                &serde_json::json!({"pid":std::process::id(),"started":started,
                "token":format!("{}:{}",std::process::id(),started)}),
            )?;
            if record.len() > 512 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "lock record exceeds512bytes",
                ));
            }
            let mut padded = vec![b' '; 512];
            padded[..record.len()].copy_from_slice(&record);
            file.seek(SeekFrom::Start(0))?;
            file.write_all(&padded)?;
            file.sync_all()?;
            Ok(())
        })();
        if let Err(e) = result {
            let _ = release(&file);
            return Err(e);
        }
        Ok(Self { file })
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        let _ = release(&self.file);
    }
}

#[cfg(not(windows))]
fn take(file: &File) -> io::Result<()> {
    file.try_lock().map_err(Into::into)
}
#[cfg(not(windows))]
fn release(file: &File) -> io::Result<()> {
    file.unlock()
}

#[cfg(windows)]
fn take(file: &File) -> io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        LOCKFILE_EXCLUSIVE_LOCK, LOCKFILE_FAIL_IMMEDIATELY, LockFileEx,
    };
    use windows_sys::Win32::System::IO::{OVERLAPPED, OVERLAPPED_0, OVERLAPPED_0_0};
    let mut overlapped = OVERLAPPED {
        Internal: 0,
        InternalHigh: 0,
        Anonymous: OVERLAPPED_0 {
            Anonymous: OVERLAPPED_0_0 {
                Offset: 4096,
                OffsetHigh: 0,
            },
        },
        hEvent: std::ptr::null_mut(),
    };
    // Owned synchronous File keeps the handle alive; nonblocking completion never
    // retains this stack pointer. Only the legacy single byte is locked.
    let ok = unsafe {
        LockFileEx(
            file.as_raw_handle(),
            LOCKFILE_EXCLUSIVE_LOCK | LOCKFILE_FAIL_IMMEDIATELY,
            0,
            1,
            0,
            &mut overlapped,
        )
    };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
#[cfg(windows)]
fn release(file: &File) -> io::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::UnlockFile;
    // The exact range is paired with take; the File remains owned until Drop ends.
    let ok = unsafe { UnlockFile(file.as_raw_handle(), 4096, 0, 1, 0) };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
