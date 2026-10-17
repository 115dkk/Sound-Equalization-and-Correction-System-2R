//! Python-compatible JSON values, parser, and pretty-printer.

use std::fmt;

/// A JSON value exactly as Python's `json` module reads and writes it.
#[derive(Clone, Debug, PartialEq)]
pub enum PyJson {
    Null,
    Bool(bool),
    /// Any integer, as canonical decimal text.
    Int(String),
    /// Any `f64`, including NaN, infinities, and negative zero.
    Float(f64),
    Str(String),
    Array(Vec<PyJson>),
    /// Object members in insertion order.
    Object(Vec<(String, PyJson)>),
}

impl PyJson {
    pub(crate) fn object(&self) -> Option<&[(String, PyJson)]> {
        match self {
            Self::Object(value) => Some(value),
            _ => None,
        }
    }

    pub(crate) fn array(&self) -> Option<&[PyJson]> {
        match self {
            Self::Array(value) => Some(value),
            _ => None,
        }
    }

    pub(crate) fn str(&self) -> Option<&str> {
        match self {
            Self::Str(value) => Some(value),
            _ => None,
        }
    }

    pub(crate) fn bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub(crate) fn get(&self, key: &str) -> Option<&PyJson> {
        self.object()?
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }

    pub(crate) fn object_mut(&mut self) -> Option<&mut Vec<(String, PyJson)>> {
        match self {
            Self::Object(value) => Some(value),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PyJsonError {
    offset: usize,
    message: String,
}

impl PyJsonError {
    fn new(offset: usize, message: impl Into<String>) -> Self {
        Self {
            offset,
            message: message.into(),
        }
    }
}

impl fmt::Display for PyJsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.message, self.offset)
    }
}

impl std::error::Error for PyJsonError {}

/// Parses the same value language as CPython's `json.loads`.
pub fn loads(text: &str) -> Result<PyJson, PyJsonError> {
    let mut parser = Parser {
        text,
        bytes: text.as_bytes(),
        pos: 0,
    };
    parser.whitespace();
    let value = parser.value(0)?;
    parser.whitespace();
    if parser.pos != parser.bytes.len() {
        return Err(parser.error("extra data"));
    }
    Ok(value)
}

struct Parser<'a> {
    text: &'a str,
    bytes: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn error(&self, message: impl Into<String>) -> PyJsonError {
        PyJsonError::new(self.pos, message)
    }

    fn whitespace(&mut self) {
        while matches!(self.bytes.get(self.pos), Some(b' ' | b'\n' | b'\r' | b'\t')) {
            self.pos += 1;
        }
    }

    fn value(&mut self, depth: usize) -> Result<PyJson, PyJsonError> {
        match self.bytes.get(self.pos).copied() {
            Some(b'n') => self.keyword(b"null", PyJson::Null),
            Some(b't') => self.keyword(b"true", PyJson::Bool(true)),
            Some(b'f') => self.keyword(b"false", PyJson::Bool(false)),
            Some(b'N') => self.keyword(b"NaN", PyJson::Float(f64::NAN)),
            Some(b'I') => self.keyword(b"Infinity", PyJson::Float(f64::INFINITY)),
            Some(b'-')
                if self
                    .bytes
                    .get(self.pos..)
                    .is_some_and(|s| s.starts_with(b"-Infinity")) =>
            {
                self.keyword(b"-Infinity", PyJson::Float(f64::NEG_INFINITY))
            }
            Some(b'"') => self.string().map(PyJson::Str),
            Some(b'[') => self.array_value(depth),
            Some(b'{') => self.object_value(depth),
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => Err(self.error("expecting value")),
        }
    }

    fn keyword(&mut self, word: &[u8], value: PyJson) -> Result<PyJson, PyJsonError> {
        if self
            .bytes
            .get(self.pos..)
            .is_some_and(|rest| rest.starts_with(word))
        {
            self.pos += word.len();
            Ok(value)
        } else {
            Err(self.error("expecting value"))
        }
    }

    fn nested(&self, depth: usize) -> Result<usize, PyJsonError> {
        let next = depth + 1;
        if next > 1000 {
            Err(self.error("maximum JSON nesting depth exceeded"))
        } else {
            Ok(next)
        }
    }

    fn array_value(&mut self, depth: usize) -> Result<PyJson, PyJsonError> {
        let depth = self.nested(depth)?;
        self.pos += 1;
        self.whitespace();
        let mut values = Vec::new();
        if self.bytes.get(self.pos) == Some(&b']') {
            self.pos += 1;
            return Ok(PyJson::Array(values));
        }
        loop {
            values.push(self.value(depth)?);
            self.whitespace();
            match self.bytes.get(self.pos) {
                Some(b',') => {
                    self.pos += 1;
                    self.whitespace();
                }
                Some(b']') => {
                    self.pos += 1;
                    return Ok(PyJson::Array(values));
                }
                _ => return Err(self.error("expecting ',' or ']'")),
            }
        }
    }

    fn object_value(&mut self, depth: usize) -> Result<PyJson, PyJsonError> {
        let depth = self.nested(depth)?;
        self.pos += 1;
        self.whitespace();
        let mut values: Vec<(String, PyJson)> = Vec::new();
        if self.bytes.get(self.pos) == Some(&b'}') {
            self.pos += 1;
            return Ok(PyJson::Object(values));
        }
        loop {
            if self.bytes.get(self.pos) != Some(&b'"') {
                return Err(self.error("expecting property name"));
            }
            let key = self.string()?;
            self.whitespace();
            if self.bytes.get(self.pos) != Some(&b':') {
                return Err(self.error("expecting ':'"));
            }
            self.pos += 1;
            self.whitespace();
            let value = self.value(depth)?;
            if let Some((_, old)) = values.iter_mut().find(|(name, _)| *name == key) {
                *old = value;
            } else {
                values.push((key, value));
            }
            self.whitespace();
            match self.bytes.get(self.pos) {
                Some(b',') => {
                    self.pos += 1;
                    self.whitespace();
                }
                Some(b'}') => {
                    self.pos += 1;
                    return Ok(PyJson::Object(values));
                }
                _ => return Err(self.error("expecting ',' or '}'")),
            }
        }
    }

    fn string(&mut self) -> Result<String, PyJsonError> {
        self.pos += 1;
        let mut out = String::new();
        let mut raw_start = self.pos;
        while let Some(&byte) = self.bytes.get(self.pos) {
            match byte {
                b'"' => {
                    out.push_str(&self.text[raw_start..self.pos]);
                    self.pos += 1;
                    return Ok(out);
                }
                b'\\' => {
                    out.push_str(&self.text[raw_start..self.pos]);
                    self.pos += 1;
                    let escaped = *self
                        .bytes
                        .get(self.pos)
                        .ok_or_else(|| self.error("unterminated string"))?;
                    self.pos += 1;
                    match escaped {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let first = self.hex_unit()?;
                            let scalar = if (0xd800..=0xdbff).contains(&first) {
                                if self.bytes.get(self.pos..self.pos.saturating_add(2))
                                    != Some(b"\\u")
                                {
                                    return Err(self.error("unpaired UTF-16 surrogate"));
                                }
                                self.pos += 2;
                                let second = self.hex_unit()?;
                                if !(0xdc00..=0xdfff).contains(&second) {
                                    return Err(self.error("unpaired UTF-16 surrogate"));
                                }
                                0x10000
                                    + ((u32::from(first) - 0xd800) << 10)
                                    + (u32::from(second) - 0xdc00)
                            } else if (0xdc00..=0xdfff).contains(&first) {
                                return Err(self.error("unpaired UTF-16 surrogate"));
                            } else {
                                u32::from(first)
                            };
                            out.push(
                                char::from_u32(scalar)
                                    .ok_or_else(|| self.error("invalid Unicode escape"))?,
                            );
                        }
                        _ => return Err(self.error("invalid escape")),
                    }
                    raw_start = self.pos;
                }
                0..=0x1f => return Err(self.error("invalid control character")),
                _ => {
                    let ch = self.text[self.pos..]
                        .chars()
                        .next()
                        .ok_or_else(|| self.error("unterminated string"))?;
                    self.pos += ch.len_utf8();
                }
            }
        }
        Err(self.error("unterminated string"))
    }

    fn hex_unit(&mut self) -> Result<u16, PyJsonError> {
        let end = self
            .pos
            .checked_add(4)
            .ok_or_else(|| self.error("invalid Unicode escape"))?;
        let digits = self
            .text
            .get(self.pos..end)
            .ok_or_else(|| self.error("invalid Unicode escape"))?;
        if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(self.error("invalid Unicode escape"));
        }
        self.pos = end;
        u16::from_str_radix(digits, 16).map_err(|_| self.error("invalid Unicode escape"))
    }

    fn number(&mut self) -> Result<PyJson, PyJsonError> {
        let start = self.pos;
        if self.bytes.get(self.pos) == Some(&b'-') {
            self.pos += 1;
        }
        match self.bytes.get(self.pos) {
            Some(b'0') => self.pos += 1,
            Some(b'1'..=b'9') => {
                self.pos += 1;
                while self.bytes.get(self.pos).is_some_and(u8::is_ascii_digit) {
                    self.pos += 1;
                }
            }
            _ => return Err(self.error("invalid number")),
        }
        let mut float = false;
        if self.bytes.get(self.pos) == Some(&b'.') {
            float = true;
            self.pos += 1;
            let before = self.pos;
            while self.bytes.get(self.pos).is_some_and(u8::is_ascii_digit) {
                self.pos += 1;
            }
            if self.pos == before {
                return Err(self.error("invalid number"));
            }
        }
        if matches!(self.bytes.get(self.pos), Some(b'e' | b'E')) {
            float = true;
            self.pos += 1;
            if matches!(self.bytes.get(self.pos), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            let before = self.pos;
            while self.bytes.get(self.pos).is_some_and(u8::is_ascii_digit) {
                self.pos += 1;
            }
            if self.pos == before {
                return Err(self.error("invalid number"));
            }
        }
        let raw = &self.text[start..self.pos];
        if float {
            let value = raw
                .parse::<f64>()
                .map_err(|_| self.error("invalid number"))?;
            Ok(PyJson::Float(value))
        } else if raw == "-0" {
            Ok(PyJson::Int("0".into()))
        } else {
            Ok(PyJson::Int(raw.into()))
        }
    }
}

/// Formats like `json.dumps(value, ensure_ascii=False, indent=2)`.
pub fn dumps(value: &PyJson) -> String {
    let mut out = String::new();
    write_value(value, 0, &mut out);
    out
}

/// Formats like `json.dumps(value, ensure_ascii=False)`.
pub fn dumps_compact(value: &PyJson) -> String {
    let mut out = String::new();
    write_compact(value, &mut out);
    out
}

fn write_compact(value: &PyJson, out: &mut String) {
    match value {
        PyJson::Null => out.push_str("null"),
        PyJson::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        PyJson::Int(value) => out.push_str(value),
        PyJson::Float(value) => out.push_str(&python_float(*value)),
        PyJson::Str(value) => write_string(value, out),
        PyJson::Array(values) => {
            out.push('[');
            for (index, value) in values.iter().enumerate() {
                if index != 0 {
                    out.push_str(", ");
                }
                write_compact(value, out);
            }
            out.push(']');
        }
        PyJson::Object(values) => {
            out.push('{');
            for (index, (key, value)) in values.iter().enumerate() {
                if index != 0 {
                    out.push_str(", ");
                }
                write_string(key, out);
                out.push_str(": ");
                write_compact(value, out);
            }
            out.push('}');
        }
    }
}

fn write_value(value: &PyJson, depth: usize, out: &mut String) {
    match value {
        PyJson::Null => out.push_str("null"),
        PyJson::Bool(value) => out.push_str(if *value { "true" } else { "false" }),
        PyJson::Int(value) => out.push_str(value),
        PyJson::Float(value) => out.push_str(&python_float(*value)),
        PyJson::Str(value) => write_string(value, out),
        PyJson::Array(values) => {
            if values.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push_str("[\n");
            for (index, value) in values.iter().enumerate() {
                out.push_str(&" ".repeat((depth + 1) * 2));
                write_value(value, depth + 1, out);
                if index + 1 != values.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&" ".repeat(depth * 2));
            out.push(']');
        }
        PyJson::Object(values) => {
            if values.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push_str("{\n");
            for (index, (key, value)) in values.iter().enumerate() {
                out.push_str(&" ".repeat((depth + 1) * 2));
                write_string(key, out);
                out.push_str(": ");
                write_value(value, depth + 1, out);
                if index + 1 != values.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&" ".repeat(depth * 2));
            out.push('}');
        }
    }
}

fn write_string(value: &str, out: &mut String) {
    out.push('"');
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\0'..='\u{1f}' => {
                use std::fmt::Write as _;
                write!(out, "\\u{:04x}", ch as u32).expect("writing to String cannot fail");
            }
            _ => out.push(ch),
        }
    }
    out.push('"');
}

fn python_float(value: f64) -> String {
    if value.is_nan() {
        return "NaN".into();
    }
    if value == f64::INFINITY {
        return "Infinity".into();
    }
    if value == f64::NEG_INFINITY {
        return "-Infinity".into();
    }
    if value == 0.0 && value.is_sign_negative() {
        return "-0.0".into();
    }
    let mut text = ryu::Buffer::new().format_finite(value).to_owned();
    let absolute = value.abs();
    if !text.contains(['e', 'E']) && absolute != 0.0 && (absolute < 1e-4 || absolute >= 1e16) {
        let negative = text.starts_with('-');
        let unsigned = text.trim_start_matches('-');
        let (digits, exponent) = if absolute < 1.0 {
            let fractional = unsigned.trim_start_matches("0.");
            let zeroes = fractional.chars().take_while(|ch| *ch == '0').count();
            (
                fractional.trim_start_matches('0').to_owned(),
                -(zeroes as i32) - 1,
            )
        } else {
            let whole = unsigned.find('.').unwrap_or(unsigned.len());
            (unsigned.replace('.', ""), whole as i32 - 1)
        };
        let (first, rest) = digits.split_at(1);
        let mantissa = if rest.is_empty() {
            first.into()
        } else {
            format!("{first}.{rest}")
        };
        text = format!("{}{mantissa}e{exponent:+}", if negative { "-" } else { "" });
    }
    if !text.contains(['.', 'e', 'E']) {
        text.push_str(".0");
    }
    if let Some(index) = text.find('e') {
        let exponent = text[index + 1..]
            .parse::<i32>()
            .expect("ryu exponent is an integer");
        text = format!(
            "{}e{}{:02}",
            &text[..index],
            if exponent >= 0 { "+" } else { "-" },
            exponent.abs()
        );
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scanner_and_encoder_edge_cases() {
        let input = r#" {"a":-0,"b":-0.0,"c":1e400,"d":340282366920938463463374607431768211456,"a":2,"s":"한글 😀\n😀"} "#;
        let value = loads(input).unwrap();
        assert_eq!(value.get("a"), Some(&PyJson::Int("2".into())));
        assert_eq!(value.get("b"), Some(&PyJson::Float(-0.0)));
        assert_eq!(value.get("c"), Some(&PyJson::Float(f64::INFINITY)));
        assert!(dumps(&value).contains("340282366920938463463374607431768211456"));
        assert!(dumps(&value).contains("한글 😀\\n😀"));
    }

    #[test]
    fn rejects_controls_surrogates_and_excess_depth() {
        assert!(loads("\"line\n\"").is_err());
        assert!(loads(r#""\ud800""#).is_err());
        assert!(loads(&format!("{}0{}", "[".repeat(1000), "]".repeat(1000))).is_ok());
        assert!(loads(&format!("{}0{}", "[".repeat(1001), "]".repeat(1001))).is_err());
    }
}
