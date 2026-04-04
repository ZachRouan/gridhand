use std::ffi::c_void;
use crate::json::{self, JsonValue};
use super::ffi::*;

pub fn list_windows() -> Result<String, String> {
    let infos = enum_visible_windows();
    let mut entries = Vec::new();

    for info in &infos {
        let win = JsonValue::Object(vec![
            ("id", JsonValue::Int(info.hwnd as i64)),
            ("title", JsonValue::OwnedStr(info.title.clone())),
            ("pid", JsonValue::Int(info.pid as i64)),
            ("x", JsonValue::Int(info.rect.left as i64)),
            ("y", JsonValue::Int(info.rect.top as i64)),
            ("width", JsonValue::Int((info.rect.right - info.rect.left) as i64)),
            ("height", JsonValue::Int((info.rect.bottom - info.rect.top) as i64)),
        ]);
        entries.push(win.to_string());
    }

    let array = format!("[{}]", entries.join(","));
    Ok(json::success_with(vec![
        ("windows", JsonValue::RawJson(array)),
    ]))
}

pub fn raise_window(id: u64) -> Result<String, String> {
    let hwnd = id as HWND;

    // Alt-key hack: briefly press Alt to unlock SetForegroundWindow
    let alt_down = keyboard_input(VK_MENU, 0);
    let alt_up = keyboard_input(VK_MENU, KEYEVENTF_KEYUP);
    unsafe {
        SendInput(1, &alt_down, input_size());
        SendInput(1, &alt_up, input_size());
    }

    unsafe {
        ShowWindow(hwnd, SW_RESTORE);
        let result = SetForegroundWindow(hwnd);
        if result == 0 {
            return Err(format!("Failed to set foreground window (id={})", id));
        }
    }

    Ok(json::success())
}

/// Find a window by title substring. Returns (hwnd_as_u64, title).
pub fn find_window_by_title(title: &str) -> Result<Option<(u64, String)>, String> {
    let title_lower = title.to_lowercase();
    let infos = enum_visible_windows();

    for info in &infos {
        if info.title.to_lowercase().contains(&title_lower) {
            return Ok(Some((info.hwnd as u64, info.title.clone())));
        }
    }

    Ok(None)
}

/// Get window bounds by HWND.
pub fn get_window_rect(id: u64) -> Result<RECT, String> {
    let hwnd = id as HWND;
    let mut rect = RECT::default();
    let ok = unsafe { GetWindowRect(hwnd, &mut rect) };
    if ok == 0 {
        return Err(format!("Failed to get window rect (id={})", id));
    }
    Ok(rect)
}

// --- Internal ---

struct WindowInfo {
    hwnd: u64,
    title: String,
    pid: u32,
    rect: RECT,
}

fn enum_visible_windows() -> Vec<WindowInfo> {
    let mut windows: Vec<WindowInfo> = Vec::new();

    unsafe extern "system" fn callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let windows = &mut *(lparam as *mut Vec<WindowInfo>);

        // Skip invisible windows
        if IsWindowVisible(hwnd) == 0 {
            return 1;
        }

        // Skip windows with no title
        let title = match get_window_title(hwnd) {
            Some(t) if !t.is_empty() => t,
            _ => return 1,
        };

        // Get PID
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);

        // Get bounds
        let mut rect = RECT::default();
        GetWindowRect(hwnd, &mut rect);

        windows.push(WindowInfo {
            hwnd: hwnd as u64,
            title,
            pid,
            rect,
        });

        1 // continue enumeration
    }

    unsafe {
        EnumWindows(callback, &mut windows as *mut Vec<WindowInfo> as LPARAM);
    }

    windows
}
