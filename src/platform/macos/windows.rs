use std::ffi::c_void;
use crate::json::{self, JsonValue};
use super::ffi::*;

pub fn list_windows() -> Result<String, String> {
    let windows = get_window_list()?;
    Ok(json::success_with(vec![
        ("windows", JsonValue::RawJson(windows)),
    ]))
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
        msg_send(app, sel, NSApplicationActivateIgnoringOtherApps);
    }

    Ok(json::success())
}

/// Find a window by title substring. Returns (window_number, bounds_json).
pub fn find_window_by_title(title: &str) -> Result<Option<(u32, String)>, String> {
    let list_json = get_window_list()?;
    let title_lower = title.to_lowercase();

    for entry in crate::json::split_json_array(&list_json) {
        if let Some(win_title) = crate::json::extract_json_string(entry, "title") {
            if win_title.to_lowercase().contains(&title_lower) {
                if let Some(win_id) = crate::json::extract_json_number(entry, "id") {
                    return Ok(Some((win_id, entry.to_string())));
                }
            }
        }
    }

    Ok(None)
}

// --- Internal helpers ---

fn get_window_list() -> Result<String, String> {
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

        for i in 0..count {
            let dict = CFArrayGetValueAtIndex(list, i);
            if dict.is_null() { continue; }

            // Get window layer — skip non-normal windows (layer != 0)
            let layer_val = CFDictionaryGetValue(dict, kCGWindowLayer);
            if let Some(layer) = cfnumber_to_i32(layer_val) {
                if layer != 0 { continue; }
            }

            // Get window number (id)
            let id_val = CFDictionaryGetValue(dict, kCGWindowNumber);
            let id = cfnumber_to_i32(id_val).unwrap_or(0) as u32;

            // Get window title
            let title_val = CFDictionaryGetValue(dict, kCGWindowName);
            let title = cfstring_to_string(title_val).unwrap_or_default();

            // Skip windows with no title
            if title.is_empty() { continue; }

            // Get owner name
            let owner_val = CFDictionaryGetValue(dict, kCGWindowOwnerName);
            let owner = cfstring_to_string(owner_val).unwrap_or_default();

            // Get PID
            let pid_val = CFDictionaryGetValue(dict, kCGWindowOwnerPID);
            let pid = cfnumber_to_i32(pid_val).unwrap_or(0);

            // Build JSON for this window
            let win_json = JsonValue::Object(vec![
                ("id", JsonValue::Int(id as i64)),
                ("title", JsonValue::OwnedStr(title)),
                ("owner", JsonValue::OwnedStr(owner)),
                ("pid", JsonValue::Int(pid as i64)),
            ]);
            windows.push(win_json.to_string());
        }

        CFRelease(list);

        let array = format!("[{}]", windows.join(","));
        Ok(array)
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
