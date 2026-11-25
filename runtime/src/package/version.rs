//! Strict, portable VERSIONINFO validation against the verifier's Cargo version.
use std::collections::BTreeSet;

const NODE_LIMIT: usize = 4096;

fn word(bytes: &[u8], offset: usize) -> Result<u16, String> {
    let end = offset.checked_add(2).ok_or("VERSIONINFO offset overflow")?;
    let value = bytes.get(offset..end).ok_or("VERSIONINFO truncated WORD")?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn dword(bytes: &[u8], offset: usize) -> Result<u32, String> {
    let end = offset.checked_add(4).ok_or("VERSIONINFO offset overflow")?;
    let value = bytes
        .get(offset..end)
        .ok_or("VERSIONINFO truncated DWORD")?;
    Ok(u32::from_le_bytes(
        value.try_into().map_err(|_| "VERSIONINFO invalid DWORD")?,
    ))
}

fn aligned(offset: usize) -> Result<usize, String> {
    Ok(offset
        .checked_add(3)
        .ok_or("VERSIONINFO alignment overflow")?
        & !3)
}

fn padding(bytes: &[u8], start: usize, end: usize) -> Result<(), String> {
    if bytes
        .get(start..end)
        .is_none_or(|value| value.iter().any(|byte| *byte != 0))
    {
        return Err("VERSIONINFO invalid alignment padding".into());
    }
    Ok(())
}

fn text(bytes: &[u8]) -> Result<String, String> {
    let (words, remainder) = bytes.as_chunks::<2>();
    if !remainder.is_empty() || words.last() != Some(&[0, 0]) {
        return Err("VERSIONINFO string lacks its UTF-16 terminator".into());
    }
    let units = words[..words.len() - 1]
        .iter()
        .map(|value| u16::from_le_bytes(*value))
        .collect::<Vec<_>>();
    if units.contains(&0) {
        return Err("VERSIONINFO string contains an embedded terminator".into());
    }
    String::from_utf16(&units).map_err(|_| "VERSIONINFO string contains invalid UTF-16".into())
}

struct Node<'a> {
    length: usize,
    kind: u16,
    key: String,
    value: &'a [u8],
    children: &'a [u8],
}

fn node<'a>(bytes: &'a [u8], budget: &mut usize) -> Result<Node<'a>, String> {
    *budget = budget
        .checked_sub(1)
        .ok_or("VERSIONINFO exceeds its node bound")?;
    let length = usize::from(word(bytes, 0)?);
    if length < 8 || !length.is_multiple_of(2) {
        return Err("VERSIONINFO invalid block length".into());
    }
    let bytes = bytes
        .get(..length)
        .ok_or("VERSIONINFO block exceeds its parent")?;
    let kind = word(bytes, 4)?;
    if kind > 1 {
        return Err("VERSIONINFO invalid value type".into());
    }
    let mut key_end = 6;
    loop {
        let unit = word(bytes, key_end)?;
        key_end += 2;
        if unit == 0 {
            break;
        }
    }
    let key = text(&bytes[6..key_end])?;
    if key.is_empty() {
        return Err("VERSIONINFO key is empty".into());
    }
    let value_length = usize::from(word(bytes, 2)?) * if kind == 1 { 2 } else { 1 };
    let value_start = if key_end == length && value_length == 0 {
        length
    } else {
        aligned(key_end)?
    };
    padding(bytes, key_end, value_start)?;
    let value_end = value_start
        .checked_add(value_length)
        .ok_or("VERSIONINFO value overflow")?;
    let value = bytes
        .get(value_start..value_end)
        .ok_or("VERSIONINFO value exceeds its block")?;
    // A final value may end before DWORD alignment; sibling padding belongs to
    // its parent, not to this child's wLength (as emitted by Windows RC).
    let children_start = if value_end == length {
        length
    } else {
        aligned(value_end)?
    };
    padding(bytes, value_end, children_start)?;
    Ok(Node {
        length,
        kind,
        key,
        value,
        children: &bytes[children_start..],
    })
}

fn children<'a>(bytes: &'a [u8], budget: &mut usize) -> Result<Vec<Node<'a>>, String> {
    let mut result = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        let child = node(&bytes[offset..], budget)?;
        let end = offset
            .checked_add(child.length)
            .ok_or("VERSIONINFO child overflow")?;
        result.push(child);
        if end == bytes.len() {
            break;
        }
        let next = aligned(end)?;
        padding(bytes, end, next)?;
        offset = next;
    }
    Ok(result)
}

fn container(node: &Node<'_>) -> Result<(), String> {
    if node.kind != 1 || !node.value.is_empty() {
        return Err("VERSIONINFO container has an invalid type or value".into());
    }
    Ok(())
}

fn string_tables(
    info: &Node<'_>,
    expected: &str,
    budget: &mut usize,
) -> Result<BTreeSet<(u16, u16)>, String> {
    container(info)?;
    let tables = children(info.children, budget)?;
    if tables.is_empty() {
        return Err("VERSIONINFO has no string tables".into());
    }
    let mut identities = BTreeSet::new();
    for table in tables {
        container(&table)?;
        if table.key.len() != 8 || !table.key.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("VERSIONINFO string table identity is invalid or duplicated".into());
        }
        let identity = u32::from_str_radix(&table.key, 16)
            .map_err(|_| "VERSIONINFO string table identity is invalid")?;
        if !identities.insert(((identity >> 16) as u16, identity as u16)) {
            return Err("VERSIONINFO string table identity is invalid or duplicated".into());
        }
        let mut keys = BTreeSet::new();
        for string in children(table.children, budget)? {
            if string.kind != 1 || !string.children.is_empty() || !keys.insert(string.key.clone()) {
                return Err("VERSIONINFO string is malformed or duplicated".into());
            }
            let value = text(string.value)?;
            if matches!(string.key.as_str(), "FileVersion" | "ProductVersion") && value != expected
            {
                return Err(format!(
                    "VERSIONINFO {} differs from the compiled package version",
                    string.key
                ));
            }
        }
        if !keys.contains("FileVersion") || !keys.contains("ProductVersion") {
            return Err("VERSIONINFO string table is missing FileVersion or ProductVersion".into());
        }
    }
    Ok(identities)
}

fn translations(info: &Node<'_>, budget: &mut usize) -> Result<BTreeSet<(u16, u16)>, String> {
    container(info)?;
    let entries = children(info.children, budget)?;
    if entries.len() != 1 {
        return Err("VERSIONINFO requires one Translation entry in VarFileInfo".into());
    }
    let entry = &entries[0];
    if entry.key != "Translation"
        || entry.kind != 0
        || !entry.children.is_empty()
        || entry.value.is_empty()
        || !entry.value.len().is_multiple_of(4)
    {
        return Err("VERSIONINFO Translation entry is malformed".into());
    }
    let mut unique = BTreeSet::new();
    for pair in entry.value.as_chunks::<4>().0 {
        if !unique.insert((word(pair, 0)?, word(pair, 2)?)) {
            return Err("VERSIONINFO Translation pair is duplicated".into());
        }
    }
    Ok(unique)
}

fn expected_words(expected: &str) -> Result<[u32; 2], String> {
    let invalid = "Compiled package version is not representable as a Windows semantic version";
    let (without_build, build) = expected
        .split_once('+')
        .map_or((expected, None), |(core, tail)| (core, Some(tail)));
    let (core, pre) = without_build
        .split_once('-')
        .map_or((without_build, None), |(core, tail)| (core, Some(tail)));
    for (suffix, prerelease) in [(build, false), (pre, true)] {
        if let Some(suffix) = suffix {
            for part in suffix.split('.') {
                if part.is_empty()
                    || !part
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                    || prerelease
                        && part.len() > 1
                        && part.starts_with('0')
                        && part.bytes().all(|byte| byte.is_ascii_digit())
                {
                    return Err(invalid.into());
                }
            }
        }
    }
    let parts = core.split('.').collect::<Vec<_>>();
    if parts.len() != 3 {
        return Err(invalid.into());
    }
    let mut values = [0_u16; 3];
    for (part, value) in parts.into_iter().zip(&mut values) {
        if part.is_empty()
            || !part.bytes().all(|byte| byte.is_ascii_digit())
            || part.len() > 1 && part.starts_with('0')
        {
            return Err(invalid.into());
        }
        *value = part.parse().map_err(|_| invalid)?;
    }
    Ok([
        (u32::from(values[0]) << 16) | u32::from(values[1]),
        u32::from(values[2]) << 16,
    ])
}

fn resource(
    bytes: &[u8],
    expected: &str,
    words: [u32; 2],
    budget: &mut usize,
) -> Result<(), String> {
    let root = node(bytes, budget)?;
    if root.length != bytes.len() {
        if bytes.len() != aligned(root.length)? {
            return Err("VERSIONINFO has trailing resource data".into());
        }
        padding(bytes, root.length, bytes.len())?;
    }
    if root.key != "VS_VERSION_INFO"
        || root.kind != 0
        || root.value.len() != 52
        || dword(root.value, 0)? != 0xfeef04bd
        || dword(root.value, 4)? != 0x10000
    {
        return Err("VERSIONINFO fixed information header is invalid".into());
    }
    for offset in [8, 16] {
        if [dword(root.value, offset)?, dword(root.value, offset + 4)?] != words {
            return Err("VERSIONINFO numeric FileVersion or ProductVersion differs from the compiled package version".into());
        }
    }
    let mut keys = BTreeSet::new();
    let mut tables = BTreeSet::new();
    let mut advertised = BTreeSet::new();
    for info in children(root.children, budget)? {
        if !keys.insert(info.key.clone()) {
            return Err("VERSIONINFO contains a duplicate information block".into());
        }
        match info.key.as_str() {
            "StringFileInfo" => tables = string_tables(&info, expected, budget)?,
            "VarFileInfo" => advertised = translations(&info, budget)?,
            _ => return Err("VERSIONINFO contains an unknown information block".into()),
        }
    }
    if !keys.contains("StringFileInfo") {
        return Err("VERSIONINFO is missing StringFileInfo".into());
    }
    if !advertised.is_subset(&tables) {
        return Err("VERSIONINFO Translation refers to a missing string table".into());
    }
    Ok(())
}

pub(super) fn verify(bytes: &[u8], expected: &str) -> Result<(), String> {
    let words = expected_words(expected)?;
    let mut budget = NODE_LIMIT;
    for ((id, language), payload) in super::pe::version_resources(bytes)? {
        resource(payload, expected, words, &mut budget)
            .map_err(|error| format!("RT_VERSION {id}/{language}: {error}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::expected_words;

    #[test]
    fn expected_version_requires_valid_bounded_semver_without_changing_suffixes() {
        for (version, expected) in [
            ("3.1.0", [0x0003_0001, 0]),
            ("1.2.3-rc.1+build.007", [0x0001_0002, 0x0003_0000]),
            ("65535.65535.65535", [u32::MAX, 0xffff_0000]),
        ] {
            assert_eq!(expected_words(version).unwrap(), expected);
        }
        for version in [
            "",
            "3",
            "3.1",
            "3.1.0.0",
            "03.1.0",
            "3.1.65536",
            "-1.2.3",
            "3.1.0-",
            "3.1.0+",
            "3.1.0-01",
            "3.1.0+a+b",
            "3.1.0 ",
            "3.1.0+💠",
        ] {
            assert!(expected_words(version).is_err(), "{version}");
        }
    }
}
