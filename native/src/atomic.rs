//! Invocation-owned sibling writes; a failed replace cannot truncate durable state.
#[cfg(unix)]
use std::fs::File;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
static NEXT: AtomicU64 = AtomicU64::new(0);

pub fn publish_new(source: &Path, destination: &Path) -> io::Result<()> {
    #[cfg(windows)]
    {
        move_new_windows(source, destination)
    }
    #[cfg(not(windows))]
    {
        std::fs::hard_link(source, destination)?;
        std::fs::remove_file(source)
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
fn move_new_windows(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{MOVEFILE_WRITE_THROUGH, MoveFileExW};
    let wide = |path: &Path| -> io::Result<Vec<u16>> {
        let mut value = path.as_os_str().encode_wide().collect::<Vec<_>>();
        if value.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "path contains NUL",
            ));
        }
        value.push(0);
        Ok(value)
    };
    let source = wide(source)?;
    let destination = wide(destination)?;
    // Owned NUL-terminated UTF16 buffers live through the call; omitting REPLACE
    // prevents overwriting another writer and does not require NTFS hard links.
    if unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

pub fn write_json(path: &Path, value: &serde_json::Value) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "state path has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "state path has no filename"))?
        .to_string_lossy();
    let (tmp, mut file) = loop {
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let tmp = parent.join(format!(".{name}-{}-{id}.tmp", std::process::id()));
        let mut opts = OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        match opts.open(&tmp) {
            Ok(f) => break (tmp, f),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    };
    let result = (|| {
        let formatter = serde_json::ser::PrettyFormatter::with_indent(b" ");
        let mut serializer = serde_json::Serializer::with_formatter(&mut file, formatter);
        serde::Serialize::serialize(value, &mut serializer).map_err(io::Error::other)?;
        file.flush()?;
        file.sync_all()?;
        match std::fs::metadata(path) {
            Ok(old) => file.set_permissions(old.permissions())?,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(e),
        }
        drop(file);
        let deadline = Instant::now() + Duration::from_secs(1);
        let mut delay = Duration::from_millis(5);
        loop {
            match std::fs::rename(&tmp, path) {
                Ok(()) => break,
                Err(e)
                    if cfg!(windows)
                        && e.kind() == io::ErrorKind::PermissionDenied
                        && Instant::now() < deadline =>
                {
                    std::thread::sleep(
                        delay.min(deadline.saturating_duration_since(Instant::now())),
                    );
                    delay = (delay * 2).min(Duration::from_millis(100));
                }
                Err(e) => return Err(e),
            }
        }
        #[cfg(unix)]
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if tmp.exists() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}
