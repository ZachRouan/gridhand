use std::ffi::c_void;
use crate::json::{self, JsonValue};
use super::ffi::*;

pub fn list_windows() -> Result<String, String> {
    let infos = enum_visible_windows();
    let mut entries = Vec::new();

    for info in &infos {
        let win = JsonValue::Object(vec![
            // HWNDs are 32-bit values sign-extended into the pointer's upper
            // bits on x64; truncate to u32 before widening to i64 so a
            // handle with bit 31 set still serializes as a non-negative
            // number (`--window-id` parses as u64 and rejects a negative
            // string).
            ("id", JsonValue::Int((info.hwnd as usize as u32) as i64)),
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
    let hwnd = id_to_hwnd(id);

    // A minimized window can never be the foreground window, so the
    // IsIconic/SW_RESTORE path below still runs whenever it's actually
    // needed even though we return early here.
    if unsafe { GetForegroundWindow() } == hwnd {
        return Ok(json::success());
    }

    // Alt-key hack: briefly press Alt to unlock SetForegroundWindow. Only
    // reachable when the target isn't already foreground — on an
    // already-foreground window this arms the menu bar and the next
    // keystroke navigates the menu instead of typing.
    let alt_down = keyboard_input(VK_MENU, 0);
    let alt_up = keyboard_input(VK_MENU, KEYEVENTF_KEYUP);
    unsafe {
        SendInput(1, &alt_down, input_size());
        SendInput(1, &alt_up, input_size());
    }

    unsafe {
        // Only restore when minimized: SW_RESTORE on a *maximized* window
        // pops it back to windowed mode, silently changing its geometry
        // between the screenshot and subsequent clicks.
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_RESTORE);
        }
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
            return Ok(Some((info.hwnd, info.title.clone())));
        }
    }

    Ok(None)
}

/// Get a window's captureable rect by id.
pub fn get_window_rect(id: u64) -> Result<RECT, String> {
    let hwnd = id_to_hwnd(id);
    window_rect(hwnd).ok_or_else(|| format!("Failed to get window rect (id={})", id))
}

/// Reconstruct an HWND from a CLI/JSON-round-tripped u64 id. HWNDs are
/// 32-bit values sign-extended into the pointer's upper bits on x64;
/// truncating to u32 first discards any high-bit noise picked up on the way
/// through JSON, then re-sign-extending through i32 reconstructs the same
/// pointer GetForegroundWindow/EnumWindows handed us, including for handles
/// with bit 31 set. Ids at or below u32::MAX round-trip identically to a
/// plain cast.
fn id_to_hwnd(id: u64) -> HWND {
    id as u32 as i32 as isize as HWND
}

/// Resolve a window's captureable rect: DWM's extended-frame-bounds
/// (excludes the invisible resize-border/shadow strips most windows carry)
/// when available, falling back to GetWindowRect. Used for both the
/// screenshot capture region (via `get_window_rect`, which every screenshot
/// path calls) and `get_window_bounds` — they must stay consistent or grid
/// clicks computed against one land off the image captured via the other.
fn window_rect(hwnd: HWND) -> Option<RECT> {
    let mut rect = RECT::default();
    let hr = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut rect as *mut RECT as *mut c_void,
            std::mem::size_of::<RECT>() as u32,
        )
    };
    if hr == 0 {
        return Some(rect);
    }

    let mut rect = RECT::default();
    let ok = unsafe { GetWindowRect(hwnd, &mut rect) };
    if ok == 0 { None } else { Some(rect) }
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
        unsafe {
            let windows = &mut *(lparam as *mut Vec<WindowInfo>);

            // Skip invisible windows
            if IsWindowVisible(hwnd) == 0 {
                return 1;
            }

            // Skip DWM-cloaked windows: suspended UWP ghosts / windows
            // parked on another virtual desktop report IsWindowVisible true
            // but are cloaked, and would otherwise show up as raisable,
            // clickable phantoms.
            let mut cloaked: u32 = 0;
            if DwmGetWindowAttribute(hwnd, DWMWA_CLOAKED,
                &mut cloaked as *mut u32 as *mut c_void,
                std::mem::size_of::<u32>() as u32) == 0
                && cloaked != 0
            {
                return 1; // suspended UWP ghosts / other virtual desktops
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
    }

    unsafe {
        EnumWindows(callback, &mut windows as *mut Vec<WindowInfo> as LPARAM);
    }

    windows
}
