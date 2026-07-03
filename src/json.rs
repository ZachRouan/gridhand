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

// The extractors below are consumed by the Linux D-Bus backend (and tests);
// other platforms parse native structures instead, so they are dead code there.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
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

/// Find the value of the first occurrence of `key` in *key position*: the
/// quoted key must be followed (after whitespace) by ':'. A plain substring
/// search stops at the first hit, which may be a string *value* — a window
/// titled exactly "id" would otherwise shadow the real "id" key.
/// Returns the remainder of `json` starting at the value (whitespace-trimmed).
fn find_key_value<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let pattern = format!("\"{}\"", key);
    let mut search_from = 0;
    while let Some(rel) = json[search_from..].find(&pattern) {
        let idx = search_from + rel;
        let after_key = &json[idx + pattern.len()..];
        if let Some(value) = after_key.trim_start().strip_prefix(':') {
            return Some(value.trim_start());
        }
        search_from = idx + pattern.len();
    }
    None
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn extract_json_string(json: &str, key: &str) -> Option<String> {
    let value = find_key_value(json, key)?;
    let inner = value.strip_prefix('"')?;
    // Scan by characters (not bytes) so an escape can never split a UTF-8
    // boundary or run past the end of the buffer; an unterminated string is
    // None, not a panic.
    let mut escaped = false;
    for (i, c) in inner.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' => escaped = true,
            '"' => return Some(decode_json_escapes(&inner[..i])),
            _ => {}
        }
    }
    None
}

fn decode_json_escapes(s: &str) -> String {
    if !s.contains('\\') {
        return s.to_string();
    }
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            result.push(c);
            continue;
        }
        match chars.next() {
            Some('"') => result.push('"'),
            Some('\\') => result.push('\\'),
            Some('n') => result.push('\n'),
            Some('r') => result.push('\r'),
            Some('t') => result.push('\t'),
            Some('b') => result.push('\u{0008}'),
            Some('f') => result.push('\u{000C}'),
            Some('/') => result.push('/'),
            Some('u') => match decode_unicode_escape(&mut chars) {
                Some(ch) => result.push(ch),
                None => result.push('\u{FFFD}'),
            },
            Some(other) => { result.push('\\'); result.push(other); }
            None => result.push('\\'),
        }
    }
    result
}

/// Decode the 4 hex digits after `\u`, consuming a following `\uXXXX` low
/// surrogate when the first is a high surrogate. None on malformed input.
fn decode_unicode_escape(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<char> {
    let hi = read_hex4(chars)?;
    if (0xD800..=0xDBFF).contains(&hi) {
        // High surrogate: needs a \uXXXX low surrogate to form a code point
        if chars.peek() != Some(&'\\') {
            return None;
        }
        chars.next();
        if chars.next() != Some('u') {
            return None;
        }
        let lo = read_hex4(chars)?;
        if !(0xDC00..=0xDFFF).contains(&lo) {
            return None;
        }
        let cp = 0x10000 + ((hi - 0xD800) << 10) + (lo - 0xDC00);
        return char::from_u32(cp);
    }
    if (0xDC00..=0xDFFF).contains(&hi) {
        return None; // lone low surrogate
    }
    char::from_u32(hi)
}

fn read_hex4(chars: &mut std::iter::Peekable<std::str::Chars>) -> Option<u32> {
    let mut v = 0u32;
    for _ in 0..4 {
        v = v * 16 + chars.next()?.to_digit(16)?;
    }
    Some(v)
}

/// Extract an array value as its raw JSON text (including brackets), tracking
/// nesting and skipping over strings so brackets in titles can't end the scan.
#[cfg_attr(not(test), allow(dead_code))]
pub fn extract_json_array(json: &str, key: &str) -> Option<String> {
    let value = find_key_value(json, key)?;
    if !value.starts_with('[') {
        return None;
    }
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (i, c) in value.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match c {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '[' if !in_string => depth += 1,
            ']' if !in_string => {
                depth -= 1;
                if depth == 0 {
                    return Some(value[..=i].to_string());
                }
            }
            _ => {}
        }
    }
    None // unterminated
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub fn extract_json_number(json: &str, key: &str) -> Option<i64> {
    let value = find_key_value(json, key)?;
    // Take the full JSON number token (sign, digits, fraction, exponent)
    let end = value
        .find(|c: char| !c.is_ascii_digit() && !matches!(c, '-' | '+' | '.' | 'e' | 'E'))
        .unwrap_or(value.len());
    if end == 0 {
        return None;
    }
    let token = &value[..end];
    if let Ok(n) = token.parse::<i64>() {
        return Some(n);
    }
    // Fractional or exponent form (e.g. fractional-scaling geometry):
    // round to nearest rather than silently truncating
    token.parse::<f64>().ok().filter(|f| f.is_finite()).map(|f| f.round() as i64)
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

    // === adversarial inputs ===

    #[test]
    fn test_extract_string_truncated_escape_no_panic() {
        // A backslash as the last byte used to push the scan past the end of
        // the buffer and panic on the slice. Unterminated strings are None.
        assert_eq!(extract_json_string("{\"title\":\"abc\\", "title"), None);
        assert_eq!(extract_json_string("{\"title\":\"abc", "title"), None);
        // Backslash followed by a multibyte char must not split a UTF-8
        // boundary (invalid JSON, but must not panic).
        let _ = extract_json_string("{\"title\":\"a\\é\"}", "title");
    }

    #[test]
    fn test_extract_string_unicode_escapes() {
        assert_eq!(
            extract_json_string("{\"t\":\"caf\\u00e9\"}", "t"),
            Some("café".to_string())
        );
        // Surrogate pair (emoji) — common in window titles
        assert_eq!(
            extract_json_string("{\"t\":\"hi \\ud83d\\ude00\"}", "t"),
            Some("hi 😀".to_string())
        );
        // \b and \f are mandatory JSON escapes
        assert_eq!(
            extract_json_string("{\"t\":\"a\\bb\\fc\"}", "t"),
            Some("a\u{0008}b\u{000C}c".to_string())
        );
        // Lone surrogate: replacement char, not a panic or literal passthrough
        assert_eq!(
            extract_json_string("{\"t\":\"x\\ud800x\"}", "t"),
            Some("x\u{FFFD}x".to_string())
        );
    }

    #[test]
    fn test_extract_key_not_fooled_by_string_values() {
        // A window titled exactly "id": the first occurrence of "id" is a
        // string value, not the key. Lookup must keep scanning.
        let json = "{\"title\":\"id\",\"id\":42}";
        assert_eq!(extract_json_number(json, "id"), Some(42));
        assert_eq!(extract_json_string(json, "title"), Some("id".to_string()));

        // Same shape for string extraction
        let json = "{\"a\":\"wm_class\",\"wm_class\":\"firefox\"}";
        assert_eq!(extract_json_string(json, "wm_class"), Some("firefox".to_string()));
    }

    #[test]
    fn test_extract_number_float_and_exponent() {
        // Fractional geometry (fractional scaling compositors) must round to
        // the nearest integer, not truncate.
        assert_eq!(extract_json_number("{\"x\":100.7}", "x"), Some(101));
        assert_eq!(extract_json_number("{\"x\":-3.2}", "x"), Some(-3));
        assert_eq!(extract_json_number("{\"w\":1e3}", "w"), Some(1000));
        assert_eq!(extract_json_number("{\"w\":1.5e2}", "w"), Some(150));
    }

    // === extract_json_array ===

    #[test]
    fn test_extract_array_basic() {
        let json = "{\"status\":\"ok\",\"windows\":[{\"id\":1},{\"id\":2}]}";
        assert_eq!(
            extract_json_array(json, "windows"),
            Some("[{\"id\":1},{\"id\":2}]".to_string())
        );
    }

    #[test]
    fn test_extract_array_nested_and_strings() {
        // Nested arrays and brackets inside strings must not end the scan early
        let json = "{\"a\":[[1,2],\"x]y\",{\"b\":[3]}],\"c\":9}";
        assert_eq!(
            extract_json_array(json, "a"),
            Some("[[1,2],\"x]y\",{\"b\":[3]}]".to_string())
        );
    }

    #[test]
    fn test_extract_array_missing_or_not_array() {
        assert_eq!(extract_json_array("{\"a\":1}", "a"), None);
        assert_eq!(extract_json_array("{\"a\":[1]}", "b"), None);
        // Unterminated array must be None, not a panic
        assert_eq!(extract_json_array("{\"a\":[1,2", "a"), None);
    }
}
