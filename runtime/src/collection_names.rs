//! Existing ASCII sticker family naming rules checked before durable paths or network work.
pub fn valid_base(base: &str) -> Result<(), String> {
    if !base.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
        || base
            .split('_')
            .any(|part| part.is_empty() || !part.bytes().all(|b| b.is_ascii_alphanumeric()))
    {
        return Err("--base must begin with a letter and contain only letters, digits and single underscores between them".into());
    }
    Ok(())
}
pub fn name_length(base: &str, bot: &str, tag: &str) -> Result<(), String> {
    valid_base(base)?;
    if bot.is_empty() || !bot.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return Err("invalid bot username".into());
    }
    if format!("{base}{tag}999_by_{bot}").len() > 64 {
        return Err("--base is too long for Telegram's 64-character sticker-set name".into());
    }
    Ok(())
}
pub fn formats(raw: &str) -> Result<Vec<String>, String> {
    let mut seen = std::collections::BTreeSet::new();
    raw.split(',').map(str::trim).map(|fmt| {
        if !["static","video","animated"].contains(&fmt) || !seen.insert(fmt) {
            Err("--formats must be a comma list of static/video/animated with no duplicates or blanks".into())
        } else { Ok(fmt.to_owned()) }
    }).collect()
}
