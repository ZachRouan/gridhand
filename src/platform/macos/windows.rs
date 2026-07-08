use std::ffi::c_void;
use crate::json::{self, JsonValue};
use super::ffi::*;

const PERMISSION_HINT: &str =
    "No window titles readable — grant Screen Recording permission in System Settings > Privacy & Security";

pub fn list_windows() -> Result<String, String> {
    let (windows, needs_hint) = get_window_list()?;
    let mut fields = vec![("windows", JsonValue::RawJson(windows))];
    if needs_hint {
        fields.push(("hint", JsonValue::Str(PERMISSION_HINT)));
    }
    Ok(json::success_with(fields))
}

pub fn raise_window(id: u64) -> Result<String, String> {
    // Find the PID for this window
    let pid = get_window_pid(id as u32)?;

    // Use NSRunningApplication to activate the app
    unsafe {
        let cls = objc_getClass(b"NSRunningApplication\0".as_ptr());
        if cls.is_null() {
            return Err("Failed to get NSRunningApplication class".to_string());
        }

        let sel = sel_registerName(b"runningApplicationWithProcessIdentifier:\0".as_ptr());
        // Cast objc_msgSend for this specific signature: (Class, SEL, pid_t) -> id
        let msg_send: extern "C" fn(*mut c_void, *mut c_void, i32) -> *mut c_void =
            std::mem::transmute(objc_msgSend as *const c_void);
        let app = msg_send(cls, sel, pid);

        if app.is_null() {
            return Err(format!("No running application found for PID {}", pid));
        }

        // [app activateWithOptions:NSApplicationActivateIgnoringOtherApps]
        let sel = sel_registerName(b"activateWithOptions:\0".as_ptr());
        let msg_send: extern "C" fn(*mut c_void, *mut c_void, u64) -> bool =
            std::mem::transmute(objc_msgSend as *const c_void);
        let activated = msg_send(app, sel, NSApplicationActivateIgnoringOtherApps);
        if !activated {
            // macOS 14+ cooperative activation can refuse a background
            // process's request; reporting success for a raise that did not
            // happen would poison every follow-up screenshot and click.
            return Err(format!(
                "macOS refused activation of PID {} (cooperative activation)", pid
            ));
        }
    }

    // Note: this activates the owning application, which brings its window
    // stack forward — macOS offers no public cross-process API to order one
    // specific window by CGWindowID (CGSOrderWindow is private SPI and is
    // rejected for windows of other processes). For multi-window apps the
    // app's frontmost window comes forward, which may not be `id`.
    Ok(json::success_with(vec![
        ("scope", JsonValue::Str("application")),
    ]))
}

/// Find a window by title substring. Returns (window_number, bounds_json).
pub fn find_window_by_title(title: &str) -> Result<Option<(u32, String)>, String> {
    let (list_json, needs_hint) = get_window_list()?;
    let title_lower = title.to_lowercase();

    for entry in crate::json::split_json_array(&list_json) {
        if let Some(win_title) = crate::json::extract_json_string(entry, "title")
            && win_title.to_lowercase().contains(&title_lower)
            && let Some(win_id) = crate::json::extract_json_number(entry, "id") {
                return Ok(Some((win_id as u32, entry.to_string())));
            }
    }

    if needs_hint {
        return Err(PERMISSION_HINT.to_string());
    }

    Ok(None)
}

// --- Internal helpers ---

/// Returns the JSON array of visible, normal-layer windows (id/title/owner/
/// pid/bounds) plus whether the Screen Recording permission hint should be
/// surfaced: true when the raw CGWindowList was non-empty but no window's
/// title was readable — exactly what happens without that permission,
/// since kCGWindowName comes back empty for every window owned by another
/// process.
fn get_window_list() -> Result<(String, bool), String> {
    unsafe {
        let list = CGWindowListCopyWindowInfo(
            kCGWindowListOptionOnScreenOnly,
            kCGNullWindowID,
        );
        if list.is_null() {
            return Err("Failed to get window list".to_string());
        }

        let count = CFArrayGetCount(list);
        let mut windows = Vec::new();
        let mut saw_any_title = false;

        for i in 0..count {
            let dict = CFArrayGetValueAtIndex(list, i);
            if dict.is_null() { continue; }

            // Read the title before the layer filter so saw_any_title
            // reflects every window in the raw list, not just the ones
            // that also pass the layer check — the permission hint should
            // fire whenever titles are unreadable, regardless of layer.
            let title_val = CFDictionaryGetValue(dict, kCGWindowName);
            let title = cfstring_to_string(title_val).unwrap_or_default();
            if !title.is_empty() {
                saw_any_title = true;
            }

            // Skip non-normal windows (layer != 0)
            let layer_val = CFDictionaryGetValue(dict, kCGWindowLayer);
            if let Some(layer) = cfnumber_to_i32(layer_val)
                && layer != 0 {
                    continue;
                }

            // Skip windows with no title
            if title.is_empty() { continue; }

            // Get window number (id)
            let id_val = CFDictionaryGetValue(dict, kCGWindowNumber);
            let id = cfnumber_to_i32(id_val).unwrap_or(0) as u32;

            // Get owner name
            let owner_val = CFDictionaryGetValue(dict, kCGWindowOwnerName);
            let owner = cfstring_to_string(owner_val).unwrap_or_default();

            // Get PID
            let pid_val = CFDictionaryGetValue(dict, kCGWindowOwnerPID);
            let pid = cfnumber_to_i32(pid_val).unwrap_or(0);

            // Get bounds — best-effort; 0/0/0/0 if the dictionary entry is
            // missing or malformed rather than dropping the window entirely.
            let mut rect = CGRect {
                origin: CGPoint { x: 0.0, y: 0.0 },
                size: CGSize { width: 0.0, height: 0.0 },
            };
            let bounds_val = CFDictionaryGetValue(dict, kCGWindowBounds);
            if !bounds_val.is_null() {
                CGRectMakeWithDictionaryRepresentation(bounds_val, &mut rect);
            }

            // Build JSON for this window
            let win_json = JsonValue::Object(vec![
                ("id", JsonValue::Int(id as i64)),
                ("title", JsonValue::OwnedStr(title)),
                ("owner", JsonValue::OwnedStr(owner)),
                ("pid", JsonValue::Int(pid as i64)),
                ("x", JsonValue::Int(rect.origin.x as i64)),
                ("y", JsonValue::Int(rect.origin.y as i64)),
                ("width", JsonValue::Int(rect.size.width as i64)),
                ("height", JsonValue::Int(rect.size.height as i64)),
            ]);
            windows.push(win_json.to_string());
        }

        CFRelease(list);

        let needs_hint = count > 0 && !saw_any_title;
        let array = format!("[{}]", windows.join(","));
        Ok((array, needs_hint))
    }
}

pub fn get_window_bounds(window_id: u32) -> Result<(i32, i32, u32, u32), String> {
    unsafe {
        let list = CGWindowListCopyWindowInfo(
            kCGWindowListOptionOnScreenOnly,
            kCGNullWindowID,
        );
        if list.is_null() {
            return Err("Failed to get window list".to_string());
        }

        let count = CFArrayGetCount(list);
        for i in 0..count {
            let dict = CFArrayGetValueAtIndex(list, i);
            if dict.is_null() { continue; }

            let id_val = CFDictionaryGetValue(dict, kCGWindowNumber);
            let id = cfnumber_to_i32(id_val).unwrap_or(0) as u32;

            if id == window_id {
                let bounds_val = CFDictionaryGetValue(dict, kCGWindowBounds);
                if !bounds_val.is_null() {
                    let mut rect = CGRect::null();
                    if CGRectMakeWithDictionaryRepresentation(bounds_val, &mut rect) {
                        CFRelease(list);
                        return Ok((
                            rect.origin.x as i32,
                            rect.origin.y as i32,
                            rect.size.width as u32,
                            rect.size.height as u32,
                        ));
                    }
                }
                CFRelease(list);
                return Err(format!("Window {} has no bounds", window_id));
            }
        }

        CFRelease(list);
        Err(format!("Window {} not found", window_id))
    }
}

fn get_window_pid(window_id: u32) -> Result<i32, String> {
    unsafe {
        let list = CGWindowListCopyWindowInfo(
            kCGWindowListOptionOnScreenOnly,
            kCGNullWindowID,
        );
        if list.is_null() {
            return Err("Failed to get window list".to_string());
        }

        let count = CFArrayGetCount(list);
        for i in 0..count {
            let dict = CFArrayGetValueAtIndex(list, i);
            if dict.is_null() { continue; }

            let id_val = CFDictionaryGetValue(dict, kCGWindowNumber);
            let id = cfnumber_to_i32(id_val).unwrap_or(0) as u32;

            if id == window_id {
                let pid_val = CFDictionaryGetValue(dict, kCGWindowOwnerPID);
                let pid = cfnumber_to_i32(pid_val).unwrap_or(0);
                CFRelease(list);
                return Ok(pid);
            }
        }

        CFRelease(list);
        Err(format!("Window {} not found", window_id))
    }
}
