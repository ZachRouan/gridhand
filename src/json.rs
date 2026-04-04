use std::fmt::Write;

#[allow(dead_code)]
pub enum JsonValue<'a> {
    Null,
    Bool(bool),
    Int(i64),
    Str(&'a str),
    OwnedStr(String),
    /// Raw JSON string — written verbatim without escaping
    RawJson(String),
    Array(Vec<JsonValue<'a>>),
    Object(Vec<(&'a str, JsonValue<'a>)>),
}

impl<'a> JsonValue<'a> {
    pub fn to_string(&self) -> String {
        let mut buf = String::new();
        self.write_to(&mut buf);
        buf
    }

    fn write_to(&self, buf: &mut String) {
        match self {
            JsonValue::Null => buf.push_str("null"),
            JsonValue::Bool(b) => buf.push_str(if *b { "true" } else { "false" }),
            JsonValue::Int(n) => write!(buf, "{}", n).unwrap(),
            JsonValue::Str(s) => write_json_string(buf, s),
            JsonValue::OwnedStr(s) => write_json_string(buf, s),
            JsonValue::RawJson(s) => buf.push_str(s),
            JsonValue::Array(items) => {
                buf.push('[');
                for (i, item) in items.iter().enumerate() {
                    if i > 0 { buf.push(','); }
                    item.write_to(buf);
                }
                buf.push(']');
            }
            JsonValue::Object(fields) => {
                buf.push('{');
                for (i, (key, val)) in fields.iter().enumerate() {
                    if i > 0 { buf.push(','); }
                    write_json_string(buf, key);
                    buf.push(':');
                    val.write_to(buf);
                }
                buf.push('}');
            }
        }
    }
}

fn write_json_string(buf: &mut String, s: &str) {
    buf.push('"');
    for ch in s.chars() {
        match ch {
            '"' => buf.push_str("\\\""),
            '\\' => buf.push_str("\\\\"),
            '\n' => buf.push_str("\\n"),
            '\r' => buf.push_str("\\r"),
            '\t' => buf.push_str("\\t"),
            c if c < '\x20' => write!(buf, "\\u{:04x}", c as u32).unwrap(),
            c => buf.push(c),
        }
    }
    buf.push('"');
}

#[allow(dead_code)]
pub fn success() -> String {
    JsonValue::Object(vec![("status", JsonValue::Str("success"))]).to_string()
}

#[allow(dead_code)]
pub fn success_with(fields: Vec<(&str, JsonValue)>) -> String {
    let mut f = vec![("status", JsonValue::Str("success"))];
    f.extend(fields);
    JsonValue::Object(f).to_string()
}

pub fn error(msg: &str) -> String {
    JsonValue::Object(vec![
        ("status", JsonValue::Str("error")),
        ("message", JsonValue::Str(msg)),
    ]).to_string()
}

pub fn split_json_array(json: &str) -> Vec<&str> {
    let json = json.trim();
    if !json.starts_with('[') || !json.ends_with(']') {
        return Vec::new();
    }
    let inner = &json[1..json.len()-1];
    let mut results = Vec::new();
    let mut depth = 0;
    let mut start = 0;
    let mut in_string = false;
    let mut escape_next = false;

    for (i, ch) in inner.char_indices() {
        if escape_next {
            escape_next = false;
            continue;
        }
        if ch == '\\' && in_string {
            escape_next = true;
            continue;
        }
        if ch == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string { continue; }
        match ch {
            '{' => depth += 1,
            '}' => depth -= 1,
            ',' if depth == 0 => {
                results.push(inner[start..i].trim());
                start = i + 1;
            }
            _ => {}
        }
    }
    let last = inner[start..].trim();
    if !last.is_empty() {
        results.push(last);
    }
    results
}

pub fn extract_json_string<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let pattern = format!("\"{}\"", key);
    let idx = json.find(&pattern)?;
    let after_key = &json[idx + pattern.len()..];
    let after_colon = after_key.trim_start().strip_prefix(':')?;
    let after_colon = after_colon.trim_start();
    if !after_colon.starts_with('"') { return None; }
    let start = 1;
    let mut end = start;
    let bytes = after_colon.as_bytes();
    while end < bytes.len() {
        if bytes[end] == b'\\' { end += 2; continue; }
        if bytes[end] == b'"' { break; }
        end += 1;
    }
    Some(&after_colon[start..end])
}

pub fn extract_json_number(json: &str, key: &str) -> Option<u32> {
    let pattern = format!("\"{}\"", key);
    let idx = json.find(&pattern)?;
    let after_key = &json[idx + pattern.len()..];
    let after_colon = after_key.trim_start().strip_prefix(':')?;
    let after_colon = after_colon.trim_start();
    let end = after_colon.find(|c: char| !c.is_ascii_digit()).unwrap_or(after_colon.len());
    after_colon[..end].parse().ok()
}
