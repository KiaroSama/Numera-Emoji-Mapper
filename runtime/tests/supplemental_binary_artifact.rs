use numera_runtime::package::verify_binary_artifact;
use std::path::{Path, PathBuf};

#[path = "support/test_admission.rs"]
mod test_admission;

fn project() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap()
}

fn logo() -> PathBuf {
    project().join("assets/numera-emoji-mapper-logo-circle.png")
}

fn u16_at(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn u32_at(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn directory(bytes: &mut [u8], offset: usize, entries: &[(u32, u32)]) {
    u16_at(bytes, offset + 14, entries.len().try_into().unwrap());
    for (index, &(name, target)) in entries.iter().enumerate() {
        u32_at(bytes, offset + 16 + index * 8, name);
        u32_at(bytes, offset + 20 + index * 8, target);
    }
}

// Independent PE/COFF fixture: one resource section with standard Type/Name/Language
// directories. Resource payloads come from the committed approved ICO, not the verifier.
fn pe_fixture(machine: u16, foreign_icon: bool) -> Vec<u8> {
    let ico = std::fs::read(project().join("assets/numera-emoji-mapper.ico")).unwrap();
    let count = u16::from_le_bytes([ico[4], ico[5]]) as usize;
    assert_eq!(count, 7, "fixture expects the seven reviewed ICO images");
    let mut images = Vec::new();
    let mut descriptions = Vec::new();
    for index in 0..count {
        let entry = &ico[6 + index * 16..6 + (index + 1) * 16];
        let length = u32::from_le_bytes(entry[8..12].try_into().unwrap()) as usize;
        let offset = u32::from_le_bytes(entry[12..16].try_into().unwrap()) as usize;
        descriptions.push(entry[..12].to_vec());
        images.push(ico[offset..offset + length].to_vec());
    }
    if foreign_icon {
        let mut png = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(16, 16)
            .write_to(&mut png, image::ImageFormat::Png)
            .unwrap();
        images[0] = png.into_inner();
        let length = images[0].len().try_into().unwrap();
        u32_at(&mut descriptions[0], 8, length);
    }
    let icon_names = 32;
    let group_names = icon_names + 16 + count * 8;
    let icon_languages = group_names + 24;
    let group_languages = icon_languages + count * 24;
    let data_entries = group_languages + 24;
    let group_payload = data_entries + (count + 1) * 16;
    let group_size = 6 + count * 14;
    let mut resource = vec![0_u8; group_payload + group_size];
    directory(
        &mut resource,
        0,
        &[
            (3, 0x8000_0000 | icon_names as u32),
            (14, 0x8000_0000 | group_names as u32),
        ],
    );
    let icons = (0..count)
        .map(|index| {
            (
                index as u32 + 1,
                0x8000_0000 | (icon_languages + index * 24) as u32,
            )
        })
        .collect::<Vec<_>>();
    directory(&mut resource, icon_names, &icons);
    directory(
        &mut resource,
        group_names,
        &[(1, 0x8000_0000 | group_languages as u32)],
    );
    u16_at(&mut resource, group_payload + 2, 1);
    u16_at(&mut resource, group_payload + 4, count as u16);
    for index in 0..count {
        directory(
            &mut resource,
            icon_languages + index * 24,
            &[(1033, (data_entries + index * 16) as u32)],
        );
        let entry = group_payload + 6 + index * 14;
        resource[entry..entry + 12].copy_from_slice(&descriptions[index]);
        u16_at(&mut resource, entry + 12, index as u16 + 1);
        let offset = resource.len();
        resource.extend(&images[index]);
        u32_at(
            &mut resource,
            data_entries + index * 16,
            0x1000 + offset as u32,
        );
        u32_at(
            &mut resource,
            data_entries + index * 16 + 4,
            images[index].len() as u32,
        );
    }
    directory(
        &mut resource,
        group_languages,
        &[(1033, (data_entries + count * 16) as u32)],
    );
    u32_at(
        &mut resource,
        data_entries + count * 16,
        0x1000 + group_payload as u32,
    );
    u32_at(
        &mut resource,
        data_entries + count * 16 + 4,
        group_size as u32,
    );

    let mut pe = vec![0_u8; 512];
    pe[..2].copy_from_slice(b"MZ");
    u32_at(&mut pe, 0x3c, 0x80);
    pe[0x80..0x84].copy_from_slice(b"PE\0\0");
    u16_at(&mut pe, 0x84, machine);
    u16_at(&mut pe, 0x86, 1);
    u16_at(&mut pe, 0x94, 240);
    u16_at(&mut pe, 0x96, 0x22);
    u16_at(&mut pe, 0x98, 0x20b);
    u32_at(&mut pe, 0x98 + 32, 0x1000);
    u32_at(&mut pe, 0x98 + 36, 0x200);
    u32_at(&mut pe, 0x98 + 56, 0x1000 + resource.len() as u32);
    u32_at(&mut pe, 0x98 + 60, 512);
    u32_at(&mut pe, 0x98 + 108, 16);
    u32_at(&mut pe, 0x98 + 128, 0x1000);
    u32_at(&mut pe, 0x98 + 132, resource.len() as u32);
    let section = 0x98 + 240;
    pe[section..section + 8].copy_from_slice(b".rsrc\0\0\0");
    u32_at(&mut pe, section + 8, resource.len() as u32);
    u32_at(&mut pe, section + 12, 0x1000);
    u32_at(&mut pe, section + 16, resource.len() as u32);
    u32_at(&mut pe, section + 20, 512);
    pe.extend(resource);
    pe
}

#[test]
fn valid_native_artifact_and_complete_windows_icon_fixture_are_accepted() {
    verify_binary_artifact(
        Path::new(env!("CARGO_BIN_EXE_numera-emoji")),
        &numera_runtime::package::platform().unwrap(),
        &logo(),
    )
    .unwrap();
    let root = tempfile::tempdir().unwrap();
    let executable = root.path().join("complete.exe");
    for (machine, target) in [(0x8664, "windows-x64"), (0xaa64, "windows-arm64")] {
        std::fs::write(&executable, pe_fixture(machine, false)).unwrap();
        verify_binary_artifact(&executable, target, &logo()).unwrap();
        let mut normalized = pe_fixture(machine, false);
        // The fixture's group payload begins at resource offset 448. Both real
        // baseline Windows artifacts normalize these seven plane fields to 1.
        for index in 0..7 {
            u16_at(&mut normalized, 512 + 448 + 6 + index * 14 + 4, 1);
        }
        std::fs::write(&executable, normalized).unwrap();
        verify_binary_artifact(&executable, target, &logo()).unwrap();
    }
}

#[test]
fn unsorted_windows_resource_identifiers_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let executable = root.path().join("unsorted.exe");
    let mut bytes = pe_fixture(0x8664, false);
    let first = bytes[528..536].to_vec();
    let second = bytes[536..544].to_vec();
    bytes[528..536].copy_from_slice(&second);
    bytes[536..544].copy_from_slice(&first);
    std::fs::write(&executable, bytes).unwrap();
    assert!(
        verify_binary_artifact(&executable, "windows-x64", &logo()).is_err(),
        "Windows resource lookup requires sorted numeric identifiers"
    );
}

#[test]
fn dangling_windows_icon_resource_offsets_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let executable = root.path().join("dangling.exe");
    let mut bytes = pe_fixture(0x8664, false);
    u32_at(&mut bytes, 512 + 20, 0x8fff_ffff);
    std::fs::write(&executable, bytes).unwrap();
    assert!(
        verify_binary_artifact(&executable, "windows-x64", &logo()).is_err(),
        "resource type labels do not prove that an embedded icon exists"
    );
}

#[test]
fn foreign_windows_icon_pixels_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let executable = root.path().join("foreign-icon.exe");
    std::fs::write(&executable, pe_fixture(0x8664, true)).unwrap();
    assert!(
        verify_binary_artifact(&executable, "windows-x64", &logo()).is_err(),
        "a complete resource tree containing a different icon is not the approved logo"
    );
}

#[test]
fn same_length_windows_icon_payload_changes_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    let executable = root.path().join("changed-payload.exe");
    let approved = pe_fixture(0x8664, false);
    let mut changed = approved.clone();
    // The first image follows the seven-entry group at resource offset 552.
    // Mutate only its PNG payload; every directory, description and length stays intact.
    let image_offset = 512 + 552;
    assert_eq!(
        &changed[image_offset..image_offset + 8],
        b"\x89PNG\r\n\x1a\n"
    );
    changed[image_offset + 40] ^= 1;
    assert_eq!(changed.len(), approved.len());
    assert_eq!(changed[..image_offset], approved[..image_offset]);
    std::fs::write(&executable, changed).unwrap();
    let error = verify_binary_artifact(&executable, "windows-x64", &logo()).unwrap_err();
    assert!(error.contains("embedded icon image differs"), "{error}");
}

#[test]
fn windows_logo_verification_requires_the_supplied_approved_logo() {
    let root = tempfile::tempdir().unwrap();
    let executable = root.path().join("approved.exe");
    std::fs::write(&executable, pe_fixture(0x8664, false)).unwrap();
    let expected = root.path().join("expected.png");
    assert!(
        verify_binary_artifact(&executable, "windows-x64", &expected).is_err(),
        "a missing expected logo cannot be reported as verified"
    );
    std::fs::write(&expected, b"not the approved logo").unwrap();
    assert!(
        verify_binary_artifact(&executable, "windows-x64", &expected).is_err(),
        "the supplied expected logo must participate in Windows verification"
    );
}
