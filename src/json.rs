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
