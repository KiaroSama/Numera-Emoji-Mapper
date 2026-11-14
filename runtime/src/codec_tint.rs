//! Native colour parsing shared by static, vector and video codecs.
pub fn parse_tint(text: &str) -> Result<[u8; 3], String> {
    let value = text
        .trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
        .trim_start_matches('#');
    if value.len() != 6 || !value.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err(format!(
            "tint must be #RRGGBB, got {}",
            crate::python_repr::quoted(text)
        ));
    }
    let mut rgb = [0; 3];
    for (index, channel) in rgb.iter_mut().enumerate() {
        *channel = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).expect("validated hex");
    }
    Ok(rgb)
}
