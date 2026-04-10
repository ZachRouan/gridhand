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

pub fn raise_window(id: u64) -> Result<String, String> {
    let mut conn = DbusConnection::connect()?;

    let mut body = MarshalBuffer::new();
    body.write_u32(id as u32);

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
    for window in crate::json::split_json_array(&windows_json) {
        if let (Some(win_title), Some(win_id)) = (crate::json::extract_json_string(window, "title"), crate::json::extract_json_number(window, "id"))
            && win_title.to_lowercase().contains(&title_lower) {
                return Ok(Some((win_id as u32, window.to_string())));
            }
    }

    Ok(None)
}

