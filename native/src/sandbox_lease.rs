//! Keep the wrapper's inherited Linux flock until all clone access has stopped.
#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
pub fn inherit(data: &std::path::Path) -> Result<Option<std::fs::File>, String> {
    use sha2::{Digest, Sha256};
    use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd};
    use std::os::unix::fs::MetadataExt;
    let raw = std::env::var("NUMERA_SANDBOX_LEASE_FD").ok();
    let parent = std::env::var("NUMERA_SANDBOX_PARENT_PID").ok();
    let birth = std::env::var("NUMERA_SANDBOX_PARENT_START").ok();
    if raw.is_none() && parent.is_none() && birth.is_none() {
        return Ok(None);
    }
    if parent
        .as_deref()
        .and_then(|v| v.parse::<u32>().ok())
        .is_none_or(|n| n == 0)
        || birth
            .as_deref()
            .and_then(|v| v.parse::<u64>().ok())
            .is_none_or(|n| n == 0)
    {
        return Err("sandbox parent lifetime is missing or invalid".into());
    }
    let fd = raw
        .and_then(|v| v.parse::<i32>().ok())
        .filter(|n| *n > 2)
        .ok_or("sandbox lifetime descriptor is missing or invalid")?;
    // Dup uses CLOEXEC: codecs must never inherit this lease. Borrow only after
    // verifying that the descriptor exists; malformed environment is fail-closed.
    unsafe extern "C" {
        fn fcntl(fd: i32, command: i32, ...) -> i32;
    }
    let flags = unsafe { fcntl(fd, 3) }; // F_GETFL on Linux.
    if flags < 0 || flags & 3 != 2 {
        // O_ACCMODE / O_RDWR.
        return Err("sandbox lifetime descriptor is not an open read-write file".into());
    }
    let duplicate = unsafe { BorrowedFd::borrow_raw(fd) }
        .try_clone_to_owned()
        .map_err(|_| "cannot retain sandbox lifetime descriptor")?;
    let file = std::fs::File::from(duplicate);
    // Transfer immediately; this module owns only the CLOEXEC duplicate from here.
    drop(unsafe { OwnedFd::from_raw_fd(fd) });
    let directory = data
        .canonicalize()
        .map_err(|_| "cannot resolve sandbox clone")?;
    if data.is_symlink() || !directory.is_dir() {
        return Err("unsupported sandbox clone path".into());
    }
    let digest = Sha256::digest(
        directory
            .to_str()
            .ok_or("sandbox path is not UTF-8")?
            .as_bytes(),
    );
    let name = digest
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let leases = directory
        .parent()
        .ok_or("sandbox has no parent")?
        .join(".panel-sandbox-leases");
    let path = leases.join(format!("{name}.sandbox-lifetime.lock"));
    let expected =
        std::fs::symlink_metadata(&path).map_err(|_| "sandbox lifetime file is missing")?;
    let actual = file
        .metadata()
        .map_err(|_| "cannot inspect sandbox lifetime descriptor")?;
    if leases.is_symlink()
        || !expected.is_file()
        || !actual.is_file()
        || actual.dev() != expected.dev()
        || actual.ino() != expected.ino()
    {
        return Err("sandbox descriptor does not own this clone's lifetime file".into());
    }
    let marker_path = directory.join(".sandbox-owner.json");
    if !std::fs::symlink_metadata(&marker_path).is_ok_and(|m| m.is_file()) {
        return Err("unsupported sandbox marker path".into());
    }
    let marker: serde_json::Value = serde_json::from_slice(
        &std::fs::read(marker_path).map_err(|_| "sandbox marker is missing")?,
    )
    .map_err(|_| "invalid sandbox marker")?;
    if marker["version"] != 2 || marker["directory"].as_str() != directory.to_str() {
        return Err("sandbox marker does not describe this clone".into());
    }
    // A separate open-file description must be excluded by the inherited flock.
    let contender = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|_| "cannot verify sandbox lifetime exclusion")?;
    unsafe extern "C" {
        fn flock(fd: i32, operation: i32) -> i32;
    }
    if unsafe { flock(contender.as_raw_fd(), 2 | 4) } == 0 {
        return Err("sandbox lifetime descriptor has no active exclusive lease".into());
    }
    if std::io::Error::last_os_error().kind() != std::io::ErrorKind::WouldBlock {
        return Err("cannot verify sandbox lifetime exclusion".into());
    }
    // Reassertion succeeds only on the locked open-file description, not an
    // independently opened descriptor for the same inode held by the wrapper.
    if unsafe { flock(file.as_raw_fd(), 2 | 4) } != 0 {
        return Err("sandbox descriptor does not share the active lifetime lease".into());
    }
    // Never LOCK_UN: all duplicates share the wrapper's open-file description.
    Ok(Some(file))
}

#[cfg(not(target_os = "linux"))]
pub fn inherit(_data: &std::path::Path) -> Result<Option<std::fs::File>, String> {
    Ok(None)
}
