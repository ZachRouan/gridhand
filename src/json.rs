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
    #[allow(clippy::inherent_to_string)]
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
            '{' | '[' => depth += 1,
            '}' | ']' => depth -= 1,
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

pub fn extract_json_string(json: &str, key: &str) -> Option<String> {
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
    Some(decode_json_escapes(&after_colon[start..end]))
}

fn decode_json_escapes(s: &str) -> String {
    if !s.contains('\\') {
        return s.to_string();
    }
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('"') => result.push('"'),
                Some('\\') => result.push('\\'),
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('/') => result.push('/'),
                Some(other) => { result.push('\\'); result.push(other); }
                None => result.push('\\'),
            }
        } else {
            result.push(c);
        }
    }
    result
}

pub fn extract_json_number(json: &str, key: &str) -> Option<i64> {
    let pattern = format!("\"{}\"", key);
    let idx = json.find(&pattern)?;
    let after_key = &json[idx + pattern.len()..];
    let after_colon = after_key.trim_start().strip_prefix(':')?;
    let after_colon = after_colon.trim_start();
    // Allow optional leading '-' followed by digits
    let end = after_colon
        .find(|c: char| c != '-' && !c.is_ascii_digit())
        .unwrap_or(after_colon.len());
    if end == 0 {
        return None;
    }
    after_colon[..end].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    // === JsonValue serialization ===

    #[test]
    fn test_null() {
        assert_eq!(JsonValue::Null.to_string(), "null");
    }

    #[test]
    fn test_bool() {
        assert_eq!(JsonValue::Bool(true).to_string(), "true");
        assert_eq!(JsonValue::Bool(false).to_string(), "false");
    }

    #[test]
    fn test_int() {
        assert_eq!(JsonValue::Int(42).to_string(), "42");
        assert_eq!(JsonValue::Int(-1).to_string(), "-1");
        assert_eq!(JsonValue::Int(0).to_string(), "0");
    }

    #[test]
    fn test_str() {
        assert_eq!(JsonValue::Str("hello").to_string(), "\"hello\"");
    }

    #[test]
    fn test_str_escaping() {
        assert_eq!(JsonValue::Str("a\"b").to_string(), "\"a\\\"b\"");
        assert_eq!(JsonValue::Str("a\\b").to_string(), "\"a\\\\b\"");
        assert_eq!(JsonValue::Str("a\nb").to_string(), "\"a\\nb\"");
        assert_eq!(JsonValue::Str("a\rb").to_string(), "\"a\\rb\"");
        assert_eq!(JsonValue::Str("a\tb").to_string(), "\"a\\tb\"");
    }

    #[test]
    fn test_str_control_chars() {
        // Control char below 0x20 (not \n, \r, \t) should be \uXXXX escaped
        assert_eq!(JsonValue::Str("\x01").to_string(), "\"\\u0001\"");
        assert_eq!(JsonValue::Str("\x1f").to_string(), "\"\\u001f\"");
    }

    #[test]
    fn test_owned_str() {
        assert_eq!(JsonValue::OwnedStr("owned".to_string()).to_string(), "\"owned\"");
    }

    #[test]
    fn test_raw_json() {
        assert_eq!(JsonValue::RawJson("[1,2,3]".to_string()).to_string(), "[1,2,3]");
        assert_eq!(JsonValue::RawJson("{\"a\":1}".to_string()).to_string(), "{\"a\":1}");
    }

    #[test]
    fn test_array() {
        let arr = JsonValue::Array(vec![JsonValue::Int(1), JsonValue::Int(2)]);
        assert_eq!(arr.to_string(), "[1,2]");
    }

    #[test]
    fn test_empty_array() {
        let arr = JsonValue::Array(vec![]);
        assert_eq!(arr.to_string(), "[]");
    }

    #[test]
    fn test_object() {
        let obj = JsonValue::Object(vec![
            ("name", JsonValue::Str("test")),
            ("val", JsonValue::Int(5)),
        ]);
        assert_eq!(obj.to_string(), "{\"name\":\"test\",\"val\":5}");
    }

    #[test]
    fn test_empty_object() {
        let obj = JsonValue::Object(vec![]);
        assert_eq!(obj.to_string(), "{}");
    }

    #[test]
    fn test_nested_object() {
        let obj = JsonValue::Object(vec![
            ("outer", JsonValue::Object(vec![
                ("inner", JsonValue::Bool(true)),
            ])),
        ]);
        assert_eq!(obj.to_string(), "{\"outer\":{\"inner\":true}}");
    }

    // === Helper functions ===

    #[test]
    fn test_success() {
        assert_eq!(success(), "{\"status\":\"success\"}");
    }

    #[test]
    fn test_success_with() {
        let result = success_with(vec![("path", JsonValue::Str("/tmp/test.png"))]);
        assert_eq!(result, "{\"status\":\"success\",\"path\":\"/tmp/test.png\"}");
    }

    #[test]
    fn test_error() {
        let result = error("something failed");
        assert_eq!(result, "{\"status\":\"error\",\"message\":\"something failed\"}");
    }

    #[test]
    fn test_error_with_special_chars() {
        let result = error("path \"foo\" not found");
        assert_eq!(result, "{\"status\":\"error\",\"message\":\"path \\\"foo\\\" not found\"}");
    }

    // === split_json_array ===

    #[test]
    fn test_split_empty_array() {
        assert_eq!(split_json_array("[]"), Vec::<&str>::new());
    }

    #[test]
    fn test_split_single_object() {
        let result = split_json_array("[{\"id\":1}]");
        assert_eq!(result, vec!["{\"id\":1}"]);
    }

    #[test]
    fn test_split_multiple_objects() {
        let result = split_json_array("[{\"id\":1},{\"id\":2},{\"id\":3}]");
        assert_eq!(result, vec!["{\"id\":1}", "{\"id\":2}", "{\"id\":3}"]);
    }

    #[test]
    fn test_split_nested_braces() {
        let result = split_json_array("[{\"a\":{\"b\":1}},{\"c\":2}]");
        assert_eq!(result, vec!["{\"a\":{\"b\":1}}", "{\"c\":2}"]);
    }

    #[test]
    fn test_split_with_escaped_quotes() {
        let result = split_json_array("[{\"title\":\"hello \\\"world\\\"\"},{\"id\":2}]");
        assert_eq!(result, vec!["{\"title\":\"hello \\\"world\\\"\"}", "{\"id\":2}"]);
    }

    #[test]
    fn test_split_with_commas_in_strings() {
        let result = split_json_array("[{\"name\":\"a,b,c\"},{\"id\":2}]");
        assert_eq!(result, vec!["{\"name\":\"a,b,c\"}", "{\"id\":2}"]);
    }

    #[test]
    fn test_split_with_whitespace() {
        let result = split_json_array("  [ {\"id\":1} , {\"id\":2} ]  ");
        assert_eq!(result, vec!["{\"id\":1}", "{\"id\":2}"]);
    }

    #[test]
    fn test_split_not_array() {
        assert_eq!(split_json_array("{\"id\":1}"), Vec::<&str>::new());
        assert_eq!(split_json_array(""), Vec::<&str>::new());
        assert_eq!(split_json_array("null"), Vec::<&str>::new());
    }

    #[test]
    fn test_split_with_brackets_in_strings() {
        let result = split_json_array("[{\"val\":\"[not,an,array]\"}]");
        assert_eq!(result, vec!["{\"val\":\"[not,an,array]\"}"]);
    }

    #[test]
    fn test_split_with_nested_arrays() {
        let result = split_json_array("[{\"tags\":[\"pinned\",\"ws-2\"]},{\"id\":2}]");
        assert_eq!(result, vec!["{\"tags\":[\"pinned\",\"ws-2\"]}", "{\"id\":2}"]);
    }

    // === extract_json_string ===

    #[test]
    fn test_extract_string_basic() {
        assert_eq!(extract_json_string("{\"title\":\"Firefox\"}", "title"), Some("Firefox".to_string()));
    }

    #[test]
    fn test_extract_string_with_spaces() {
        assert_eq!(
            extract_json_string("{\"title\" : \"My Window\"}", "title"),
            Some("My Window".to_string())
        );
    }

    #[test]
    fn test_extract_string_escaped_quotes() {
        // Escape sequences are decoded: \" becomes "
        assert_eq!(
            extract_json_string("{\"title\":\"say \\\"hello\\\"\"}", "title"),
            Some("say \"hello\"".to_string())
        );
    }

    #[test]
    fn test_extract_string_missing_key() {
        assert_eq!(extract_json_string("{\"title\":\"Firefox\"}", "name"), None);
    }

    #[test]
    fn test_extract_string_not_a_string_value() {
        // Value is a number, not a string
        assert_eq!(extract_json_string("{\"id\":42}", "id"), None);
    }

    #[test]
    fn test_extract_string_empty_value() {
        assert_eq!(extract_json_string("{\"title\":\"\"}", "title"), Some(String::new()));
    }

    #[test]
    fn test_extract_string_multiple_keys() {
        let json = "{\"id\":1,\"title\":\"Test\",\"owner\":\"App\"}";
        assert_eq!(extract_json_string(json, "title"), Some("Test".to_string()));
        assert_eq!(extract_json_string(json, "owner"), Some("App".to_string()));
    }

    #[test]
    fn test_extract_string_with_backslash_in_value() {
        // Escape sequences are decoded: \\ becomes \
        assert_eq!(
            extract_json_string("{\"path\":\"C:\\\\Users\\\\test\"}", "path"),
            Some("C:\\Users\\test".to_string())
        );
    }

    // === extract_json_number ===

    #[test]
    fn test_extract_number_basic() {
        assert_eq!(extract_json_number("{\"id\":42}", "id"), Some(42));
    }

    #[test]
    fn test_extract_number_zero() {
        assert_eq!(extract_json_number("{\"id\":0}", "id"), Some(0));
    }

    #[test]
    fn test_extract_number_negative() {
        assert_eq!(extract_json_number("{\"x\":-500}", "x"), Some(-500));
        assert_eq!(extract_json_number("{\"x\":-500,\"y\":-200}", "y"), Some(-200));
    }

    #[test]
    fn test_extract_number_large() {
        assert_eq!(extract_json_number("{\"id\":4294967295}", "id"), Some(4294967295));
    }

    #[test]
    fn test_extract_number_with_spaces() {
        assert_eq!(extract_json_number("{\"id\" : 42}", "id"), Some(42));
    }

    #[test]
    fn test_extract_number_missing_key() {
        assert_eq!(extract_json_number("{\"id\":42}", "pid"), None);
    }

    #[test]
    fn test_extract_number_string_value() {
        // Value is a string, not a number — should return None
        assert_eq!(extract_json_number("{\"id\":\"not_a_number\"}", "id"), None);
    }

    #[test]
    fn test_extract_number_among_other_fields() {
        let json = "{\"title\":\"Test\",\"pid\":1234,\"id\":5678}";
        assert_eq!(extract_json_number(json, "pid"), Some(1234));
        assert_eq!(extract_json_number(json, "id"), Some(5678));
    }

    #[test]
    fn test_extract_number_at_end_of_object() {
        assert_eq!(extract_json_number("{\"id\":99}", "id"), Some(99));
    }

    #[test]
    fn test_extract_number_followed_by_comma() {
        assert_eq!(extract_json_number("{\"id\":99,\"name\":\"x\"}", "id"), Some(99));
    }
}
