use crate::json::{self, JsonValue};
use super::dbus::DbusConnection;
use super::dbus::types::{MarshalBuffer, UnmarshalBuffer};

const DEST: &str = "org.gnome.Shell";
const PATH: &str = "/org/gnome/Shell/Extensions/Windows";
const IFACE: &str = "org.gnome.Shell.Extensions.Windows";

pub fn list_windows() -> Result<String, String> {
    let mut conn = DbusConnection::connect()?;

    let reply = conn.call_method(
        DEST, PATH, IFACE,
        "List",
        None,
        &[],
    )?;

    let mut ubuf = UnmarshalBuffer::new(&reply.body);
    let windows_json = ubuf.read_string()?;

    Ok(json::success_with(vec![
        ("windows", JsonValue::RawJson(windows_json)),
    ]))
}

pub fn raise_window(id: u32) -> Result<String, String> {
    let mut conn = DbusConnection::connect()?;

    let mut body = MarshalBuffer::new();
    body.write_u32(id);

    conn.call_method(
        DEST, PATH, IFACE,
        "Activate",
        Some("u"),
        &body.into_bytes(),
    )?;

    Ok(json::success())
}

/// Get window details by ID (used internally for bounds).
#[allow(dead_code)]
pub fn get_window_details(conn: &mut DbusConnection, id: u32) -> Result<String, String> {
    let mut body = MarshalBuffer::new();
    body.write_u32(id);

    let reply = conn.call_method(
        DEST, PATH, IFACE,
        "Details",
        Some("u"),
        &body.into_bytes(),
    )?;

    let mut ubuf = UnmarshalBuffer::new(&reply.body);
    ubuf.read_string()
}

/// Find a window by title substring. Returns (id, details_json) or None.
#[allow(dead_code)]
pub fn find_window_by_title(conn: &mut DbusConnection, title: &str) -> Result<Option<(u32, String)>, String> {
    let reply = conn.call_method(
        DEST, PATH, IFACE,
        "List",
        None,
        &[],
    )?;

    let mut ubuf = UnmarshalBuffer::new(&reply.body);
    let windows_json = ubuf.read_string()?;

    let title_lower = title.to_lowercase();
    for window in split_json_array(&windows_json) {
        if let (Some(win_title), Some(win_id)) = (extract_json_string(window, "title"), extract_json_number(window, "id")) {
            if win_title.to_lowercase().contains(&title_lower) {
                return Ok(Some((win_id, window.to_string())));
            }
        }
    }

    Ok(None)
}

fn split_json_array(json: &str) -> Vec<&str> {
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

fn extract_json_string<'a>(json: &'a str, key: &str) -> Option<&'a str> {
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

fn extract_json_number(json: &str, key: &str) -> Option<u32> {
    let pattern = format!("\"{}\"", key);
    let idx = json.find(&pattern)?;
    let after_key = &json[idx + pattern.len()..];
    let after_colon = after_key.trim_start().strip_prefix(':')?;
    let after_colon = after_colon.trim_start();
    let end = after_colon.find(|c: char| !c.is_ascii_digit()).unwrap_or(after_colon.len());
    after_colon[..end].parse().ok()
}
