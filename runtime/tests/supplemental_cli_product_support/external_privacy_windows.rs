//! Observe real NTFS owner/DACL strings independently of the application's binary ACL checks.
use std::{ffi::c_void, os::windows::ffi::OsStrExt, path::Path, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::{
        Authorization::*, DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl,
        OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    },
};
fn wide(path: &Path) -> Vec<u16> {
    path.canonicalize()
        .unwrap()
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect()
}
struct Local(*mut c_void);
impl Drop for Local {
    fn drop(&mut self) {
        unsafe {
            LocalFree(self.0);
        }
    }
}
fn permissions(path: &Path, text: &str) {
    let text = text.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let mut descriptor = Local(null_mut());
    let (mut present, mut defaulted, mut acl) = (0, 0, null_mut());
    // The fixture owns these directories and both native allocations outlive the API calls.
    unsafe {
        assert_ne!(
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                text.as_ptr(),
                SDDL_REVISION_1,
                &mut descriptor.0,
                null_mut()
            ),
            0
        );
        assert_ne!(
            GetSecurityDescriptorDacl(descriptor.0, &mut present, &mut acl, &mut defaulted),
            0
        );
        assert_ne!(present, 0);
        assert!(!acl.is_null());
        assert_eq!(
            SetNamedSecurityInfoW(
                wide(path).as_ptr(),
                SE_FILE_OBJECT,
                DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
                null_mut(),
                null_mut(),
                acl,
                null_mut()
            ),
            0
        );
    }
}
pub fn share(path: &Path) {
    permissions(path, "D:P(A;OICI;FA;;;OW)(A;OICI;FR;;;WD)");
}
pub fn restrict(path: &Path) {
    permissions(path, "D:P(A;OICI;FA;;;OW)");
}
pub fn snapshot(path: &Path) -> String {
    let info = DACL_SECURITY_INFORMATION | OWNER_SECURITY_INFORMATION;
    let mut sd = Local(null_mut());
    let mut string = null_mut();
    let mut length = 0;
    unsafe {
        assert_eq!(
            GetNamedSecurityInfoW(
                wide(path).as_ptr(),
                SE_FILE_OBJECT,
                info,
                null_mut(),
                null_mut(),
                null_mut(),
                null_mut(),
                &mut sd.0
            ),
            0
        );
        assert_ne!(
            ConvertSecurityDescriptorToStringSecurityDescriptorW(
                sd.0,
                SDDL_REVISION_1,
                info,
                &mut string,
                &mut length
            ),
            0
        );
        let _storage = Local(string.cast());
        let chars = std::slice::from_raw_parts(string, length as usize);
        String::from_utf16(chars.strip_suffix(&[0]).unwrap_or(chars)).unwrap()
    }
}
pub fn verify_private(base: &Path, _: &Path, retained: &Path) {
    let descriptor = snapshot(base);
    let (owner, dacl) = descriptor
        .strip_prefix("O:")
        .unwrap()
        .split_once("D:")
        .unwrap();
    let (flags, ace) = dacl.split_once('(').unwrap();
    assert!(flags.contains('P'), "{descriptor}");
    assert_eq!(ace, format!("A;OICI;FA;;;{owner})"));
    let child = snapshot(retained);
    let (_, child_dacl) = child.split_once("D:").unwrap();
    let (_, child_ace) = child_dacl.split_once('(').unwrap();
    assert!(
        child_ace == format!("A;ID;FA;;;{owner})") || child_ace == format!("A;;FA;;;{owner})"),
        "{child}"
    );
}
