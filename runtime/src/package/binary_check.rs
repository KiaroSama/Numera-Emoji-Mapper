//! Byte-level PE/ELF architecture and embedded-logo verifier (zero unsafe).
use std::path::Path;

#[path = "pe.rs"]
mod pe;

const PE_MACHINE_X64: u16 = 0x8664;
const PE_MACHINE_ARM64: u16 = 0xAA64;
const ELF_MACHINE_X64: u16 = 62;
const ELF_MACHINE_ARM64: u16 = 183;
const LINUX_LOGO_SECTION: &[u8] = b".numera_logo";

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
    Err("Linux executable is missing .numera_logo embedded logo section".into())
}

pub fn verify_binary_artifact(
    executable: &Path,
    target: &str,
    expected_logo_png: &Path,
) -> Result<(), String> {
    let bytes = std::fs::read(executable).map_err(|e| format!("cannot read binary: {e}"))?;
    match target {
        "windows-x64" | "windows-arm64" => {
            let logo = std::fs::read(expected_logo_png)
                .map_err(|e| format!("cannot read approved logo: {e}"))?;
            let machine = if target == "windows-x64" {
                PE_MACHINE_X64
            } else {
                PE_MACHINE_ARM64
            };
            pe::verify(&bytes, machine, &logo)
        }
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
