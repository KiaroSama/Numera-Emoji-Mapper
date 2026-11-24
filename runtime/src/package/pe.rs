//! Bounded PE32+ resource traversal for the approved Windows application icon.
use std::collections::{BTreeMap, BTreeSet};

const APPROVED_LOGO: &[u8] = include_bytes!("../../../assets/numera-emoji-mapper-logo-circle.png");
const APPROVED_ICON: &[u8] = include_bytes!("../../../assets/numera-emoji-mapper.ico");
const DIRECTORY_FLAG: u32 = 0x8000_0000;
const RESOURCE_ENTRY_LIMIT: usize = 4096;

fn range(bytes: &[u8], offset: usize, length: usize) -> Result<&[u8], String> {
    let end = offset.checked_add(length).ok_or("PE offset overflow")?;
    bytes
        .get(offset..end)
        .ok_or_else(|| "PE data exceeds file bounds".into())
}

fn word(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let value = range(bytes, offset, 2)?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn dword(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let value = range(bytes, offset, 4)?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

struct Section {
    rva: u32,
    size: u32,
    offset: u32,
}

struct Image<'a> {
    bytes: &'a [u8],
    sections: Vec<Section>,
}

impl<'a> Image<'a> {
    fn at_rva(&self, rva: u32, size: u32) -> Result<&'a [u8], String> {
        if size == 0 || rva.checked_add(size).is_none() {
            return Err("PE resource has an empty or overflowing RVA range".into());
        }
        let mut found = None;
        for section in &self.sections {
            let Some(relative) = rva.checked_sub(section.rva) else {
                continue;
            };
            let Some(end) = relative.checked_add(size) else {
                continue;
            };
            if end > section.size {
                continue;
            }
            if found.is_some() {
                return Err("PE resource RVA belongs to overlapping sections".into());
            }
            let offset = section
                .offset
                .checked_add(relative)
                .ok_or("PE resource file offset overflow")?;
            found = Some(range(self.bytes, offset as usize, size as usize)?);
        }
        found.ok_or_else(|| "PE resource RVA is outside its file-backed sections".into())
    }
}

#[derive(Clone, Copy)]
struct Entry {
    id: Option<u32>,
    target: u32,
}

fn directory(bytes: &[u8], offset: usize, budget: &mut usize) -> Result<Vec<Entry>, String> {
    let header = range(bytes, offset, 16)?;
    let named = word(header, 12)? as usize;
    let count = named + word(header, 14)? as usize;
    *budget = budget
        .checked_sub(count)
        .ok_or("PE resource directory exceeds its entry bound")?;
    let start = offset
        .checked_add(16)
        .ok_or("PE directory offset overflow")?;
    let entries = range(bytes, start, count * 8)?;
    let mut result = Vec::with_capacity(count);
    let mut names = BTreeSet::new();
    let mut previous_id = None;
    for (index, entry) in entries.as_chunks::<8>().0.iter().enumerate() {
        let name = dword(entry, 0)?;
        if (name & DIRECTORY_FLAG != 0) != (index < named) || !names.insert(name) {
            return Err("PE resource directory has invalid or duplicate names".into());
        }
        if index >= named {
            if previous_id.is_some_and(|previous| name <= previous) {
                return Err("PE numeric resource identifiers must be strictly increasing".into());
            }
            previous_id = Some(name);
        }
        result.push(Entry {
            id: (index >= named).then_some(name),
            target: dword(entry, 4)?,
        });
    }
    Ok(result)
}

fn subdirectory(entry: Entry) -> Result<usize, String> {
    if entry.target & DIRECTORY_FLAG == 0 {
        return Err("PE icon resource is missing a required directory level".into());
    }
    Ok((entry.target & !DIRECTORY_FLAG) as usize)
}

type Resources<'a> = BTreeMap<(u32, u32), &'a [u8]>;

fn resources<'a>(
    image: &Image<'a>,
    root: &[u8],
    entries: &[Entry],
    kind: u32,
    budget: &mut usize,
) -> Result<Resources<'a>, String> {
    let entry = entries
        .iter()
        .find(|entry| entry.id == Some(kind))
        .copied()
        .ok_or("Windows executable is missing RT_GROUP_ICON / RT_ICON embedded logo")?;
    let names = directory(root, subdirectory(entry)?, budget)?;
    let mut result = BTreeMap::new();
    for name in names {
        let id = name.id.ok_or("PE application icon names must be numeric")?;
        for language in directory(root, subdirectory(name)?, budget)? {
            let language_id = language
                .id
                .filter(|id| *id <= u16::MAX.into())
                .ok_or("PE icon language must be a numeric language identifier")?;
            if language.target & DIRECTORY_FLAG != 0 {
                return Err("PE icon resource has an unexpected nested directory".into());
            }
            let data = range(root, language.target as usize, 16)?;
            let payload = image.at_rva(dword(data, 0)?, dword(data, 4)?)?;
            if result.insert((id, language_id), payload).is_some() {
                return Err("PE icon resource has a duplicate identity".into());
            }
        }
    }
    if result.is_empty() {
        return Err("PE icon resource has no image data".into());
    }
    Ok(result)
}

struct ApprovedImage {
    description: &'static [u8],
    payload: &'static [u8],
}

fn approved_images() -> Result<Vec<ApprovedImage>, String> {
    if word(APPROVED_ICON, 0)? != 0 || word(APPROVED_ICON, 2)? != 1 {
        return Err("Compiled approved Windows icon has an invalid header".into());
    }
    let count = word(APPROVED_ICON, 4)? as usize;
    if count == 0 || count > 256 {
        return Err("Compiled approved Windows icon has an invalid image count".into());
    }
    let mut result = Vec::with_capacity(count);
    for entry in range(APPROVED_ICON, 6, count * 16)?.as_chunks::<16>().0 {
        let payload = range(
            APPROVED_ICON,
            dword(entry, 12)? as usize,
            dword(entry, 8)? as usize,
        )?;
        result.push(ApprovedImage {
            description: &entry[..12],
            payload,
        });
    }
    Ok(result)
}

fn verify_icons(icons: &Resources<'_>, groups: &Resources<'_>) -> Result<(), String> {
    let expected = approved_images()?;
    let mut referenced = BTreeSet::new();
    for (&(id, language), group) in groups {
        // build.rs uses winresource's default application icon group, ID 1.
        if id != 1
            || word(group, 0)? != 0
            || word(group, 2)? != 1
            || word(group, 4)? as usize != expected.len()
            || group.len() != 6 + expected.len() * 14
        {
            return Err("PE application icon group differs from the approved icon".into());
        }
        for (entry, approved) in group[6..].as_chunks::<14>().0.iter().zip(&expected) {
            // RC may normalize PNG icon planes from 0 to 1. Image bytes, size,
            // dimensions, colors and bit depth must still match the approved ICO.
            if entry[..4] != approved.description[..4]
                || !matches!(word(entry, 4)?, 0 | 1)
                || entry[6..12] != approved.description[6..12]
            {
                return Err("PE icon image description differs from the approved icon".into());
            }
            let key = (u32::from(word(entry, 12)?), language);
            if !referenced.insert(key) || icons.get(&key).copied() != Some(approved.payload) {
                return Err("PE embedded icon image differs from the approved logo".into());
            }
        }
    }
    if referenced.len() != icons.len() {
        return Err("PE contains unreferenced application icon images".into());
    }
    Ok(())
}

pub(super) fn verify(bytes: &[u8], machine: u16, expected_logo: &[u8]) -> Result<(), String> {
    if expected_logo != APPROVED_LOGO {
        return Err("Windows expected logo differs from the compiled approved logo".into());
    }
    if range(bytes, 0, 2)? != b"MZ" {
        return Err("Windows executable is missing MZ header".into());
    }
    let pe_offset = dword(bytes, 0x3c)? as usize;
    let pe = range(bytes, pe_offset, 24)?;
    if &pe[..4] != b"PE\0\0" || word(pe, 4)? != machine {
        return Err("Windows executable PE signature or target machine is invalid".into());
    }
    let sections = word(pe, 6)? as usize;
    if sections == 0 || sections > 96 {
        return Err("PE executable has an invalid section count".into());
    }
    let optional_size = word(pe, 20)? as usize;
    let optional_offset = pe_offset
        .checked_add(24)
        .ok_or("PE header offset overflow")?;
    let optional = range(bytes, optional_offset, optional_size)?;
    if word(optional, 0)? != 0x20b || dword(optional, 108)? < 3 {
        return Err("Windows executable requires a PE32+ resource data directory".into());
    }
    let section_offset = optional_offset
        .checked_add(optional_size)
        .ok_or("PE section offset overflow")?;
    let mut image = Image {
        bytes,
        sections: Vec::with_capacity(sections),
    };
    for section in range(bytes, section_offset, sections * 40)?
        .as_chunks::<40>()
        .0
    {
        image.sections.push(Section {
            rva: dword(section, 12)?,
            size: dword(section, 16)?,
            offset: dword(section, 20)?,
        });
    }
    let root = image.at_rva(dword(optional, 128)?, dword(optional, 132)?)?;
    let mut budget = RESOURCE_ENTRY_LIMIT;
    let entries = directory(root, 0, &mut budget)?;
    let icons = resources(&image, root, &entries, 3, &mut budget)?;
    let groups = resources(&image, root, &entries, 14, &mut budget)?;
    verify_icons(&icons, &groups)
}
