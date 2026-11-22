use super::{
    LottieError, Result,
    json_value::{Node, Number, Text},
};

pub(super) struct Parser<'a> {
    pub(super) text: &'a str,
    pub(super) at: usize,
}
impl Parser<'_> {
    pub(super) fn error(&self, message: &str) -> LottieError {
        let prefix = &self.text[..self.at];
        let line = prefix.bytes().filter(|&b| b == b'\n').count() + 1;
        let column = prefix.rsplit('\n').next().unwrap_or("").chars().count() + 1;
        let mut result = LottieError::new(
            "JSONDecodeError",
            format!(
                "{message}: line {line} column {column} (char {})",
                prefix.chars().count()
            ),
        );
        result.raw_fallback = true;
        result.handled = true;
        result
    }
    pub(super) fn ws(&mut self) {
        while matches!(
            self.text.as_bytes().get(self.at),
            Some(b' ' | b'\t' | b'\r' | b'\n')
        ) {
            self.at += 1;
        }
    }
    fn consume(&mut self, byte: u8) -> bool {
        if self.text.as_bytes().get(self.at) == Some(&byte) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn string(&mut self) -> Result<Text> {
        if !self.consume(b'"') {
            return Err(self.error("Expecting property name enclosed in double quotes"));
        }
        let start = self.at - 1;
        let mut output = Vec::new();
        loop {
            let Some(&byte) = self.text.as_bytes().get(self.at) else {
                self.at = start;
                return Err(self.error("Unterminated string starting at"));
            };
            if self.consume(b'"') {
                break;
            }
            if self.consume(b'\\') {
                let Some(&escape) = self.text.as_bytes().get(self.at) else {
                    self.at = start;
                    return Err(self.error("Unterminated string starting at"));
                };
                self.at += 1;
                let c = match escape {
                    b'"' => 0x22,
                    b'\\' => 0x5c,
                    b'/' => 0x2f,
                    b'b' => 8,
                    b'f' => 12,
                    b'n' => 10,
                    b'r' => 13,
                    b't' => 9,
                    b'u' => {
                        let begin = self.at;
                        let c = self.hex4()?;
                        if (0xd800..=0xdbff).contains(&c) && self.text[self.at..].starts_with("\\u")
                        {
                            self.at += 2;
                            let low = self.hex4()?;
                            if (0xdc00..=0xdfff).contains(&low) {
                                0x10000 + ((c - 0xd800) << 10) + low - 0xdc00
                            } else {
                                self.at = begin + 4;
                                c
                            }
                        } else {
                            c
                        }
                    }
                    _ => {
                        self.at -= 1;
                        return Err(self.error("Invalid \\escape"));
                    }
                };
                output.push(c);
            } else {
                if byte < 32 {
                    return Err(self.error("Invalid control character at"));
                }
                let c = self.text[self.at..].chars().next().expect("nonempty UTF-8");
                self.at += c.len_utf8();
                output.push(c as u32);
            }
        }
        Ok(output)
    }
    fn hex4(&mut self) -> Result<u32> {
        let Some(text) = self.text.get(self.at..self.at + 4) else {
            return Err(self.error("Invalid \\uXXXX escape"));
        };
        let value =
            u32::from_str_radix(text, 16).map_err(|_| self.error("Invalid \\uXXXX escape"))?;
        self.at += 4;
        Ok(value)
    }
    pub(super) fn node(&mut self, depth: usize) -> Result<Node> {
        if depth > 900 {
            return Err(LottieError::new(
                "RecursionError",
                "maximum recursion depth exceeded while decoding a JSON object",
            ));
        }
        self.ws();
        let start = self.at;
        match self.text.as_bytes().get(self.at).copied() {
            Some(b'{') => {
                self.at += 1;
                self.ws();
                let mut entries: Vec<(Text, Node)> = Vec::new();
                if !self.consume(b'}') {
                    loop {
                        self.ws();
                        let key = self.string()?;
                        self.ws();
                        if !self.consume(b':') {
                            return Err(self.error("Expecting ':' delimiter"));
                        }
                        let value = self.node(depth + 1)?;
                        if let Some((_, old)) = entries.iter_mut().find(|(k, _)| *k == key) {
                            *old = value;
                        } else {
                            entries.push((key, value));
                        }
                        self.ws();
                        if self.consume(b'}') {
                            break;
                        }
                        if !self.consume(b',') {
                            return Err(self.error("Expecting ',' delimiter"));
                        }
                    }
                }
                Ok(Node::Object(entries))
            }
            Some(b'[') => {
                self.at += 1;
                self.ws();
                let mut items = Vec::new();
                if !self.consume(b']') {
                    loop {
                        items.push(self.node(depth + 1)?);
                        self.ws();
                        if self.consume(b']') {
                            break;
                        }
                        if !self.consume(b',') {
                            return Err(self.error("Expecting ',' delimiter"));
                        }
                    }
                }
                Ok(Node::Array(items))
            }
            Some(b'"') => self.string().map(Node::String),
            _ => {
                for (literal, node) in [
                    ("true", Node::Bool(true)),
                    ("false", Node::Bool(false)),
                    ("null", Node::Null),
                    ("NaN", Node::Number(Number::Float(f64::NAN))),
                    ("Infinity", Node::Number(Number::Float(f64::INFINITY))),
                    ("-Infinity", Node::Number(Number::Float(f64::NEG_INFINITY))),
                ] {
                    if self.text[self.at..].starts_with(literal) {
                        self.at += literal.len();
                        return Ok(node);
                    }
                }
                self.consume(b'-');
                if !self.consume(b'0') {
                    let begin = self.at;
                    while self
                        .text
                        .as_bytes()
                        .get(self.at)
                        .is_some_and(u8::is_ascii_digit)
                    {
                        self.at += 1;
                    }
                    if self.at == begin {
                        self.at = start;
                        return Err(self.error("Expecting value"));
                    }
                }
                let mut float = false;
                if self.text.as_bytes().get(self.at) == Some(&b'.')
                    && self
                        .text
                        .as_bytes()
                        .get(self.at + 1)
                        .is_some_and(u8::is_ascii_digit)
                {
                    float = true;
                    self.at += 1;
                    while self
                        .text
                        .as_bytes()
                        .get(self.at)
                        .is_some_and(u8::is_ascii_digit)
                    {
                        self.at += 1;
                    }
                }
                if matches!(self.text.as_bytes().get(self.at), Some(b'e') | Some(b'E')) {
                    let begin = self.at;
                    self.at += 1;
                    if matches!(self.text.as_bytes().get(self.at), Some(b'+') | Some(b'-')) {
                        self.at += 1;
                    }
                    let digits = self.at;
                    while self
                        .text
                        .as_bytes()
                        .get(self.at)
                        .is_some_and(u8::is_ascii_digit)
                    {
                        self.at += 1;
                    }
                    if self.at == digits {
                        self.at = begin;
                    } else {
                        float = true;
                    }
                }
                let token = &self.text[start..self.at];
                if float {
                    Ok(Node::Number(Number::Float(
                        token.parse().map_err(|_| self.error("Expecting value"))?,
                    )))
                } else if token.trim_start_matches('-').len() > 4300 {
                    let mut error = LottieError::new(
                        "ValueError",
                        "Exceeds the limit (4300 digits) for integer string conversion",
                    );
                    error.raw_fallback = true;
                    Err(error)
                } else {
                    Ok(Node::Number(Number::Integer(
                        if token == "-0" { "0" } else { token }.into(),
                    )))
                }
            }
        }
    }
}
