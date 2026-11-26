//! Independent access checks for real persisted codec evidence.
use std::path::Path;
#[cfg(windows)]
#[allow(dead_code, unsafe_code)]
#[path = "../supplemental_cli_product_support/external_privacy_windows.rs"]
mod windows;

pub fn assert_private(directory: &Path, file: &Path) {
    assert!(file.starts_with(directory) && file.is_file());
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let meta = std::fs::symlink_metadata(directory).unwrap();
        assert!(meta.is_dir());
        assert_eq!(meta.mode() & 0o777, 0o700, "{}", directory.display());
        assert_eq!(meta.uid(), std::fs::metadata(file).unwrap().uid());
    }
    #[cfg(windows)]
    windows::verify_private(directory, directory, file);
}
