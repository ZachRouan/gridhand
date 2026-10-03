//! Linux desktop-environment integration facade.
//!
//! Generic Linux callers use this module instead of depending on compositor-
//! specific window/display APIs. GNOME delegates to the existing window-calls
//! extension plus Mutter DisplayConfig. KDE Plasma delegates to KWin scripting.

use super::dbus::DbusConnection;
use super::{display, kwin, windows};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DesktopBackend {
    Gnome,
    Kde,
}

fn desktop_hint_is_kde(value: &str) -> bool {
    value.split([':', ';']).map(str::trim).any(|part| {
        let lower = part.to_ascii_lowercase();
        lower == "kde" || lower.contains("plasma")
    })
}

fn current_backend() -> DesktopBackend {
    for key in [
        "XDG_CURRENT_DESKTOP",
        "XDG_SESSION_DESKTOP",
        "DESKTOP_SESSION",
    ] {
        if let Ok(value) = std::env::var(key)
            && desktop_hint_is_kde(&value)
        {
            return DesktopBackend::Kde;
        }
    }

    if let Ok(value) = std::env::var("KDE_FULL_SESSION")
        && !value.is_empty()
        && value != "0"
        && !value.eq_ignore_ascii_case("false")
    {
        return DesktopBackend::Kde;
    }

    // Preserve the existing behavior for GNOME and unknown desktops.
    DesktopBackend::Gnome
}

fn gnome_window_id(id: &str) -> Result<u32, String> {
    id.parse::<u32>()
        .map_err(|_| format!("Window id '{}' is not a valid window-calls u32 id", id))
}

pub fn list_windows() -> Result<String, String> {
    match current_backend() {
        DesktopBackend::Gnome => windows::list_windows(),
        DesktopBackend::Kde => kwin::list_windows(),
    }
}

pub fn find_window_by_title(title: &str) -> Result<Option<(String, String)>, String> {
    let mut conn = DbusConnection::connect()?;
    find_window_by_title_with_conn(&mut conn, title)
}

pub(crate) fn find_window_by_title_with_conn(
    conn: &mut DbusConnection,
    title: &str,
) -> Result<Option<(String, String)>, String> {
    match current_backend() {
        DesktopBackend::Gnome => windows::find_window_by_title(conn, title)
            .map(|opt| opt.map(|(id, json)| (id.to_string(), json))),
        DesktopBackend::Kde => kwin::find_window_by_title_with_conn(conn, title),
    }
}

pub fn raise_window(id: &str) -> Result<String, String> {
    match current_backend() {
        DesktopBackend::Gnome => windows::raise_window(gnome_window_id(id)?),
        DesktopBackend::Kde => {
            let mut conn = DbusConnection::connect()?;
            kwin::raise_window_with_conn(&mut conn, id)
        }
    }
}

pub(crate) fn raise_window_with_conn(
    conn: &mut DbusConnection,
    id: &str,
) -> Result<String, String> {
    match current_backend() {
        DesktopBackend::Gnome => windows::raise_window_with_conn(conn, gnome_window_id(id)?),
        DesktopBackend::Kde => kwin::raise_window_with_conn(conn, id),
    }
}

pub fn window_details(id: &str) -> Result<String, String> {
    let mut conn = DbusConnection::connect()?;
    window_details_with_conn(&mut conn, id)
}

pub(crate) fn window_details_with_conn(
    conn: &mut DbusConnection,
    id: &str,
) -> Result<String, String> {
    match current_backend() {
        DesktopBackend::Gnome => windows::get_window_details(conn, gnome_window_id(id)?),
        DesktopBackend::Kde => kwin::window_details_with_conn(conn, id),
    }
}

pub fn window_bounds(id: &str) -> Result<(i32, i32, u32, u32), String> {
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

pub(crate) fn screenshot_logical_geometry_with_conn(
    conn: &mut DbusConnection,
) -> Result<Option<(i64, i64, i64, i64)>, String> {
    match current_backend() {
        // The existing GNOME screenshot path already reports window geometry
        // in the pixel space used by its portal screenshot. Keep that behavior.
        DesktopBackend::Gnome => Ok(None),
        DesktopBackend::Kde => kwin::virtual_geometry_with_conn(conn).map(Some),
    }
}

pub fn logical_desktop_size() -> Option<(i32, i32)> {
    match current_backend() {
        DesktopBackend::Gnome => display::logical_desktop_size(),
        DesktopBackend::Kde => kwin::logical_desktop_size().ok(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gnome_window_id_accepts_u32_range() {
        assert_eq!(gnome_window_id("0"), Ok(0));
        assert_eq!(gnome_window_id(&u32::MAX.to_string()), Ok(u32::MAX));
    }

    #[test]
    fn gnome_window_id_rejects_larger_ids() {
        let err = gnome_window_id("4294967296").unwrap_err();
        assert!(err.contains("window-calls"));
    }

    #[test]
    fn kde_desktop_hints_are_detected() {
        assert!(desktop_hint_is_kde("KDE"));
        assert!(desktop_hint_is_kde("KDE:Plasma"));
        assert!(desktop_hint_is_kde("plasma"));
        assert!(!desktop_hint_is_kde("GNOME"));
        assert!(!desktop_hint_is_kde("XFCE"));
    }
}
