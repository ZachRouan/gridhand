//! Linux desktop-environment integration facade.
//!
//! Callers use this module instead of depending directly on GNOME-specific
//! window or display APIs. The current implementation delegates to the
//! existing GNOME `window-calls` and Mutter backends; a later backend can
//! replace that dispatch without changing screenshot/input call sites.

use super::dbus::DbusConnection;
use super::{display, windows};

/// window-calls ids are u32 on the wire; a larger value would silently wrap
/// and act on a different window. Keep that backend-specific constraint here.
fn gnome_window_id(id: u64) -> Result<u32, String> {
    u32::try_from(id).map_err(|_| {
        format!(
            "Window id {} is out of range for the window-calls extension (u32)",
            id
        )
    })
}

pub fn list_windows() -> Result<String, String> {
    windows::list_windows()
}

pub fn find_window_by_title(title: &str) -> Result<Option<(u64, String)>, String> {
    let mut conn = DbusConnection::connect()?;
    find_window_by_title_with_conn(&mut conn, title)
}

pub(crate) fn find_window_by_title_with_conn(
    conn: &mut DbusConnection,
    title: &str,
) -> Result<Option<(u64, String)>, String> {
    windows::find_window_by_title(conn, title).map(|opt| opt.map(|(id, json)| (id as u64, json)))
}

pub fn raise_window(id: u64) -> Result<String, String> {
    windows::raise_window(gnome_window_id(id)?)
}

pub(crate) fn raise_window_with_conn(conn: &mut DbusConnection, id: u64) -> Result<String, String> {
    windows::raise_window_with_conn(conn, gnome_window_id(id)?)
}

pub fn window_details(id: u64) -> Result<String, String> {
    let mut conn = DbusConnection::connect()?;
    window_details_with_conn(&mut conn, id)
}

pub(crate) fn window_details_with_conn(
    conn: &mut DbusConnection,
    id: u64,
) -> Result<String, String> {
    windows::get_window_details(conn, gnome_window_id(id)?)
}

pub fn window_bounds(id: u64) -> Result<(i32, i32, u32, u32), String> {
    let details = window_details(id)?;
    let x =
        crate::json::extract_json_number(&details, "x").ok_or("Window details missing 'x'")? as i32;
    let y =
        crate::json::extract_json_number(&details, "y").ok_or("Window details missing 'y'")? as i32;
    let w = crate::json::extract_json_number(&details, "width")
        .ok_or("Window details missing 'width'")? as u32;
    let h = crate::json::extract_json_number(&details, "height")
        .ok_or("Window details missing 'height'")? as u32;
    Ok((x, y, w, h))
}

pub fn logical_desktop_size() -> Option<(i32, i32)> {
    display::logical_desktop_size()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gnome_window_id_accepts_u32_range() {
        assert_eq!(gnome_window_id(0), Ok(0));
        assert_eq!(gnome_window_id(u32::MAX as u64), Ok(u32::MAX));
    }

    #[test]
    fn gnome_window_id_rejects_larger_ids() {
        let err = gnome_window_id(u32::MAX as u64 + 1).unwrap_err();
        assert!(err.contains("window-calls"));
    }
}
