//! Byte-level PE/ELF architecture and embedded-logo verifier (zero unsafe).
use std::path::Path;

const PE_MACHINE_X64: u16 = 0x8664;
const PE_MACHINE_ARM64: u16 = 0xAA64;
const ELF_MACHINE_X64: u16 = 62;
const ELF_MACHINE_ARM64: u16 = 183;
const RT_ICON: u32 = 3;
const RT_GROUP_ICON: u32 = 14;
const LINUX_LOGO_SECTION: &[u8] = b".rodata.numera_logo";

fn u16_le(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let slice = bytes
        .get(offset..offset + 2)
        .ok_or("binary truncated while reading u16")?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn u32_le(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let slice = bytes
        .get(offset..offset + 4)
        .ok_or("binary truncated while reading u32")?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn u64_le(bytes: &[u8], offset: usize) -> Result<u64, String> {
    let slice = bytes
        .get(offset..offset + 8)
        .ok_or("binary truncated while reading u64")?;
    Ok(u64::from_le_bytes([
        slice[0], slice[1], slice[2], slice[3], slice[4], slice[5], slice[6], slice[7],
    ]))
}

fn verify_pe(bytes: &[u8], expected_machine: u16) -> Result<(), String> {
    if bytes.get(0..2) != Some(b"MZ") {
        return Err("Windows executable is missing MZ header".into());
    }
    let pe_offset = u32_le(bytes, 0x3C)? as usize;
    if bytes.get(pe_offset..pe_offset + 4) != Some(b"PE\0\0") {
        return Err("Windows executable is missing PE signature".into());
    }
    let coff = pe_offset + 4;
    let machine = u16_le(bytes, coff)?;
    if machine != expected_machine {
        return Err(format!(
            "PE machine 0x{machine:04X} does not match expected 0x{expected_machine:04X}"
        ));
    }
    let num_sections = u16_le(bytes, coff + 2)? as usize;
    let opt_size = u16_le(bytes, coff + 16)? as usize;
    let sections_start = coff + 20 + opt_size;
    let mut rsrc_raw: Option<(usize, usize)> = None;
    for index in 0..num_sections {
        let off = sections_start + index * 40;
        let name = bytes
            .get(off..off + 8)
            .ok_or("PE section table truncated")?;
        if name.starts_with(b".rsrc\0") {
            let raw_size = u32_le(bytes, off + 16)? as usize;
            let raw_ptr = u32_le(bytes, off + 20)? as usize;
            rsrc_raw = Some((raw_ptr, raw_size));
            break;
        }
    }
    let (rsrc_ptr, rsrc_size) =
        rsrc_raw.ok_or("Windows executable is missing .rsrc section (no embedded icon)")?;
    let rsrc = bytes
        .get(rsrc_ptr..rsrc_ptr.saturating_add(rsrc_size))
        .ok_or("PE .rsrc section exceeds file bounds")?;
    let named_entries = u16_le(rsrc, 12)? as usize;
    let id_entries = u16_le(rsrc, 14)? as usize;
    let mut has_icon = false;
    let mut has_group_icon = false;
    for idx in 0..(named_entries + id_entries) {
        let entry_off = 16 + idx * 8;
        let id = u32_le(rsrc, entry_off)?;
        let data_or_subdir = u32_le(rsrc, entry_off + 4)?;
        if id == RT_ICON && data_or_subdir != 0 {
            has_icon = true;
        }
        if id == RT_GROUP_ICON && data_or_subdir != 0 {
            has_group_icon = true;
        }
    }
    if !has_icon || !has_group_icon {
        return Err("Windows executable is missing RT_GROUP_ICON / RT_ICON embedded logo".into());
    }
    Ok(())
}

fn verify_elf(bytes: &[u8], expected_machine: u16, expected_logo: &[u8]) -> Result<(), String> {
    if bytes.get(0..4) != Some(b"\x7fELF") {
        return Err("Linux executable is missing ELF magic".into());
    }
    if bytes.get(4) != Some(&2) || bytes.get(5) != Some(&1) {
        return Err("Linux executable must be 64-bit little-endian ELF".into());
    }
    let machine = u16_le(bytes, 18)?;
    if machine != expected_machine {
        return Err(format!(
            "ELF machine {machine} does not match expected {expected_machine}"
        ));
    }
    let shoff = u64_le(bytes, 40)? as usize;
    let shentsize = u16_le(bytes, 58)? as usize;
    let shnum = u16_le(bytes, 60)? as usize;
    let shstrndx = u16_le(bytes, 62)? as usize;
    if shoff == 0 || shentsize < 64 || shstrndx >= shnum {
        return Err("ELF section header table is missing or invalid".into());
    }
    let shstr_hdr = shoff + shstrndx * shentsize;
    let shstr_off = u64_le(bytes, shstr_hdr + 24)? as usize;
    let shstr_size = u64_le(bytes, shstr_hdr + 32)? as usize;
    let shstr = bytes
        .get(shstr_off..shstr_off.saturating_add(shstr_size))
        .ok_or("ELF section string table exceeds file bounds")?;
    for idx in 0..shnum {
        let hdr = shoff + idx * shentsize;
        let name_off = u32_le(bytes, hdr)? as usize;
        let Some(tail) = shstr.get(name_off..) else {
            continue;
        };
        let end = tail.iter().position(|b| *b == 0).unwrap_or(tail.len());
        if &tail[..end] == LINUX_LOGO_SECTION {
            let sec_off = u64_le(bytes, hdr + 24)? as usize;
            let sec_size = u64_le(bytes, hdr + 32)? as usize;
            let sec_bytes = bytes
                .get(sec_off..sec_off.saturating_add(sec_size))
                .ok_or("ELF logo section exceeds file bounds")?;
            if sec_bytes != expected_logo {
                return Err(
                    "Linux executable embedded logo section bytes differ from approved logo".into(),
                );
            }
            return Ok(());
        }
    }
    Err("Linux executable is missing .rodata.numera_logo embedded logo section".into())
}

pub fn verify_binary_artifact(
    executable: &Path,
    target: &str,
    expected_logo_png: &Path,
) -> Result<(), String> {
    let bytes = std::fs::read(executable).map_err(|e| format!("cannot read binary: {e}"))?;
    match target {
        "windows-x64" => verify_pe(&bytes, PE_MACHINE_X64),
        "windows-arm64" => verify_pe(&bytes, PE_MACHINE_ARM64),
        "linux-x64" => {
            let logo = std::fs::read(expected_logo_png)
                .map_err(|e| format!("cannot read approved logo: {e}"))?;
            verify_elf(&bytes, ELF_MACHINE_X64, &logo)
        }
        "linux-arm64" => {
            let logo = std::fs::read(expected_logo_png)
                .map_err(|e| format!("cannot read approved logo: {e}"))?;
            verify_elf(&bytes, ELF_MACHINE_ARM64, &logo)
        }
        _ => Err("Unsupported package platform or architecture".into()),
    }
}

pub fn run_verify_binary_cli(
    values: &std::collections::BTreeMap<String, String>,
    resolve: &dyn Fn(&str) -> std::path::PathBuf,
) -> Result<(), String> {
    let executable = values
        .get("--executable")
        .ok_or("--executable is required")?;
    let target = values
        .get("--target")
        .map(String::as_str)
        .ok_or("--target is required")?;
    let logo = values
        .get("--logo")
        .map(|v| resolve(v))
        .unwrap_or_else(|| resolve("assets/numera-emoji-mapper-logo-circle.png"));
    verify_binary_artifact(&resolve(executable), target, &logo)
}
