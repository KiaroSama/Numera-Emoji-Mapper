//! Atomically protect the fallback and verify the security descriptor on the actual directory.
#![allow(unsafe_code)]
use std::{
    ffi::c_void,
    fs::File,
    io,
    mem::{size_of, size_of_val},
    os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    path::Path,
    ptr::{addr_of_mut, null, null_mut},
};
use windows_sys::Win32::{
    Foundation::LocalFree,
    Security::{
        ACCESS_ALLOWED_ACE, ACL,
        Authorization::{
            EXPLICIT_ACCESS_W, GetSecurityInfo, SE_FILE_OBJECT, SET_ACCESS, SetEntriesInAclW,
            TRUSTEE_IS_SID, TRUSTEE_IS_USER, TRUSTEE_W,
        },
        DACL_SECURITY_INFORMATION, EqualSid, GetAce, GetSecurityDescriptorControl,
        GetTokenInformation, InitializeSecurityDescriptor, OWNER_SECURITY_INFORMATION, PSID,
        SE_DACL_PROTECTED, SECURITY_ATTRIBUTES, SECURITY_DESCRIPTOR, SECURITY_MAX_SID_SIZE,
        SUB_CONTAINERS_AND_OBJECTS_INHERIT, SetSecurityDescriptorControl,
        SetSecurityDescriptorDacl, SetSecurityDescriptorOwner, TOKEN_QUERY, TOKEN_USER, TokenUser,
    },
    Storage::FileSystem::{
        CreateDirectoryW, FILE_ALL_ACCESS, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
        GetVolumeInformationByHandleW, READ_CONTROL,
    },
    System::{
        SystemServices::{
            ACCESS_ALLOWED_ACE_TYPE, FILE_PERSISTENT_ACLS, SECURITY_DESCRIPTOR_REVISION,
        },
        Threading::{GetCurrentProcess, OpenProcessToken},
    },
};

struct Local(*mut c_void);
impl Drop for Local {
    fn drop(&mut self) {
        // GetSecurityInfo and SetEntriesInAcl return caller-owned LocalAlloc storage.
        unsafe {
            LocalFree(self.0);
        }
    }
}
fn checked(ok: i32) -> io::Result<()> {
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
fn status(error: u32) -> io::Result<()> {
    if error == 0 {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(error as i32))
    }
}

pub(super) fn private_directory(path: &Path, reuse: bool) -> io::Result<()> {
    let mut handle = null_mut();
    // Read only the process token's user SID. The aligned buffer owns both the
    // TOKEN_USER header and the largest possible Windows SID through verification.
    unsafe {
        checked(OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_QUERY,
            &mut handle,
        ))?;
    }
    let token = unsafe { OwnedHandle::from_raw_handle(handle) };
    const WORDS: usize =
        (size_of::<TOKEN_USER>() + SECURITY_MAX_SID_SIZE as usize).div_ceil(size_of::<usize>());
    let mut user = [0usize; WORDS];
    let mut length = 0;
    unsafe {
        checked(GetTokenInformation(
            token.as_raw_handle(),
            TokenUser,
            user.as_mut_ptr().cast(),
            size_of_val(&user) as u32,
            &mut length,
        ))?;
        let sid = (*user.as_ptr().cast::<TOKEN_USER>()).User.Sid;
        match create(path, sid) {
            Err(error) if reuse && error.kind() == io::ErrorKind::AlreadyExists => {}
            result => result?,
        }
        verify(path, sid)
    }
}
fn create(path: &Path, sid: PSID) -> io::Result<()> {
    let entry = EXPLICIT_ACCESS_W {
        grfAccessPermissions: FILE_ALL_ACCESS,
        grfAccessMode: SET_ACCESS,
        grfInheritance: SUB_CONTAINERS_AND_OBJECTS_INHERIT,
        Trustee: TRUSTEE_W {
            TrusteeForm: TRUSTEE_IS_SID,
            TrusteeType: TRUSTEE_IS_USER,
            ptstrName: sid.cast(),
            ..Default::default()
        },
    };
    // Canonicalize the already-checked parent for a long-path-capable UTF-16 name.
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("missing directory parent"))?;
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::other("missing directory name"))?;
    let path: Vec<u16> = parent
        .canonicalize()?
        .join(name)
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let mut acl = null_mut();
    let mut descriptor = SECURITY_DESCRIPTOR::default();
    let sd = addr_of_mut!(descriptor).cast();
    // The ACL and user SID remain live until CreateDirectoryW has consumed the descriptor.
    unsafe {
        status(SetEntriesInAclW(1, &entry, null(), &mut acl))?;
        let _storage = Local(acl.cast());
        checked(InitializeSecurityDescriptor(
            sd,
            SECURITY_DESCRIPTOR_REVISION,
        ))?;
        checked(SetSecurityDescriptorOwner(sd, sid, 0))?;
        checked(SetSecurityDescriptorDacl(sd, 1, acl, 0))?;
        checked(SetSecurityDescriptorControl(
            sd,
            SE_DACL_PROTECTED,
            SE_DACL_PROTECTED,
        ))?;
        let attrs = SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: sd,
            bInheritHandle: 0,
        };
        checked(CreateDirectoryW(path.as_ptr(), &attrs))
    }
}
fn verify(path: &Path, sid: PSID) -> io::Result<()> {
    let file = File::options()
        .access_mode(READ_CONTROL | FILE_READ_ATTRIBUTES)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)?;
    let meta = file.metadata()?;
    let refusal = || {
        io::Error::other(
            "expected a physical directory with a protected inheritable owner-only DACL",
        )
    };
    if !meta.is_dir() || meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(refusal());
    }
    let handle = file.as_raw_handle();
    let (mut owner, mut acl, mut sd) = (null_mut(), null_mut::<ACL>(), Local(null_mut()));
    let (mut flags, mut control, mut revision) = (0, 0, 0);
    // All returned ACE/SID pointers borrow the OS-validated descriptor allocation.
    unsafe {
        checked(GetVolumeInformationByHandleW(
            handle,
            null_mut(),
            0,
            null_mut(),
            null_mut(),
            &mut flags,
            null_mut(),
            0,
        ))?;
        if flags & FILE_PERSISTENT_ACLS == 0 {
            return Err(refusal());
        }
        status(GetSecurityInfo(
            handle,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            &mut acl,
            null_mut(),
            &mut sd.0,
        ))?;
        checked(GetSecurityDescriptorControl(
            sd.0,
            &mut control,
            &mut revision,
        ))?;
        if owner.is_null()
            || EqualSid(owner, sid) == 0
            || control & SE_DACL_PROTECTED == 0
            || acl.is_null()
            || (*acl).AceCount != 1
        {
            return Err(refusal());
        }
        let mut ace = null_mut();
        checked(GetAce(acl, 0, &mut ace))?;
        let header = &*ace.cast::<windows_sys::Win32::Security::ACE_HEADER>();
        if u32::from(header.AceType) != ACCESS_ALLOWED_ACE_TYPE
            || u32::from(header.AceFlags) != SUB_CONTAINERS_AND_OBJECTS_INHERIT
        {
            return Err(refusal());
        }
        let allowed = &*ace.cast::<ACCESS_ALLOWED_ACE>();
        if allowed.Mask != FILE_ALL_ACCESS
            || EqualSid((&raw const allowed.SidStart).cast_mut().cast(), sid) == 0
        {
            return Err(refusal());
        }
    }
    Ok(())
}
