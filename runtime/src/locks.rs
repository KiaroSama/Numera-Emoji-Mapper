//! Exact legacy lock coordinates: byte4096 on Windows, flock on POSIX.
#![allow(unsafe_code)]
use std::fs::{File, OpenOptions};
use std::io::{self, Seek, SeekFrom, Write};
use std::path::Path;

/// Every canonical-map writer uses this project-wide OS lease coordinate.
pub fn canonical_map_path(root: &Path) -> std::path::PathBuf {
    root.join(".locks/canonical_map.lock")
}

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

pub(crate) fn existing_file_identity(path: &Path) -> io::Result<Option<(u64, u128)>> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => return Ok(None),
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    }
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = file.metadata()?;
        Ok(Some((metadata.dev(), u128::from(metadata.ino()))))
    }
    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_ID_INFO, FileIdInfo, GetFileInformationByHandleEx,
        };
        let mut info = std::mem::MaybeUninit::<FILE_ID_INFO>::uninit();
        // The owned handle and correctly sized output buffer live through the call.
        // FileIdInfo supplies the full 128-bit identity on both NTFS and ReFS.
        if unsafe {
            GetFileInformationByHandleEx(
                file.as_raw_handle(),
                FileIdInfo,
                info.as_mut_ptr().cast(),
                std::mem::size_of::<FILE_ID_INFO>() as u32,
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        // A successful call initialized every field of FILE_ID_INFO.
        let info = unsafe { info.assume_init() };
        Ok(Some((
            info.VolumeSerialNumber,
            u128::from_le_bytes(info.FileId.Identifier),
        )))
    }
}

pub struct Lease {
    file: File,
    unlock_on_drop: bool,
}
impl Lease {
    /// Refresh diagnostic modification time; ownership remains solely the OS lease.
    pub fn heartbeat(&self) -> io::Result<()> {
        self.file
            .try_clone()?
            .set_times(std::fs::FileTimes::new().set_modified(std::time::SystemTime::now()))
    }
    pub fn clone_file(&self) -> io::Result<File> {
        self.file.try_clone()
    }
    /// Transfer a shared POSIX lifetime to close-only ownership before child inheritance.
    #[cfg(unix)]
    pub fn into_inherited_file(mut self) -> io::Result<File> {
        let file = self.file.try_clone()?;
        // LOCK_UN on any duplicate would release the child's shared open-file-description lock.
        self.unlock_on_drop = false;
        Ok(file)
    }
    pub fn acquire(path: &Path, started: &str) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|error| {
                io::Error::new(error.kind(), format!("lock file open failed: {error}"))
            })?;
        take(&file).map_err(|error| {
            #[cfg(windows)]
            let contention = error.raw_os_error() == Some(33); // ERROR_LOCK_VIOLATION only.
            #[cfg(not(windows))]
            let contention = error.kind() == io::ErrorKind::WouldBlock;
            let detail = if contention {
                crate::lock_diagnostics::busy_message(path, &error)
            } else {
                error.to_string()
            };
            io::Error::new(
                error.kind(),
                format!("OS lock acquisition failed: {detail}"),
            )
        })?;
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
            return Err(io::Error::new(
                e.kind(),
                format!("lock holder record write failed: {e}"),
            ));
        }
        Ok(Self {
            file,
            unlock_on_drop: true,
        })
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        if self.unlock_on_drop {
            let _ = release(&self.file);
        }
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
