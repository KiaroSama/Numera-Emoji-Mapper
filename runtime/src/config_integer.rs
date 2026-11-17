//! Original decimal environment grammar, without bootstrapping an interpreter.
pub(crate) fn decimal(raw: &str) -> Option<String> {
    let (negative, body) = match raw.as_bytes().first()? {
        b'-' => (true, &raw[1..]),
        b'+' => (false, &raw[1..]),
        _ => (false, raw),
    };
    let mut normalized = String::new();
    let mut previous_digit = false;
    let mut count = 0;
    for ch in body.chars() {
        if ch == '_' && previous_digit {
            previous_digit = false;
        } else {
            let digit = crate::emoji_ids::decimal_value(ch)?;
            previous_digit = true;
            count += 1;
            // The pinned CPython 3.11 oracle uses its default decimal digit limit.
            if count > 4300 {
                return None;
            }
            if !normalized.is_empty() || digit != 0 {
                normalized.push(char::from(b'0' + digit as u8));
            }
        }
    }
    if !previous_digit {
        return None;
    }
    if normalized.is_empty() {
        normalized.push('0');
    } else if negative {
        normalized.insert(0, '-');
    }
    Some(normalized)
}

/// Preserve the source's decimal result and warning before each caller narrows its range.
pub fn resolve(
    raw: Option<&str>,
    key: &str,
    default: i64,
    minimum: Option<i64>,
    maximum: Option<i64>,
) -> (String, Option<String>) {
    let raw = raw.unwrap_or("").trim_matches(crate::emoji_ids::whitespace);
    if raw.is_empty() {
        return (default.to_string(), None);
    }
    let Some(normalized) = decimal(raw) else {
        return (
            default.to_string(),
            Some(format!(
                "WARNING: {key} is not a whole number; using {default}.\n"
            )),
        );
    };
    let value = normalized.parse::<i64>();
    // A valid decimal outside i64 still compares exactly against an i64 bound.
    if let Some(minimum) = minimum
        && value
            .as_ref()
            .map_or(normalized.starts_with('-'), |n| *n < minimum)
    {
        return (
            minimum.to_string(),
            Some(format!(
                "WARNING: {key}={normalized} below {minimum}; using {minimum}.\n"
            )),
        );
    }
    if let Some(maximum) = maximum
        && value
            .as_ref()
            .map_or(!normalized.starts_with('-'), |n| *n > maximum)
    {
        return (
            maximum.to_string(),
            Some(format!(
                "WARNING: {key}={normalized} above {maximum}; using {maximum}.\n"
            )),
        );
    }
    (normalized, None)
}
