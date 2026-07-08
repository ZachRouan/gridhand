#![allow(dead_code)]
#![allow(non_upper_case_globals)]

use std::ffi::c_void;
use super::ffi::*;

pub fn mouse_move(x: i32, y: i32) -> Result<String, String> {
    let point = CGPoint { x: x as f64, y: y as f64 };
    unsafe {
        let event = CGEventCreateMouseEvent(
            std::ptr::null(),
            kCGEventMouseMoved,
            point,
            kCGMouseButtonLeft,
        );
        if event.is_null() {
            return Err("Failed to create mouse move event".to_string());
        }
        CGEventPost(kCGHIDEventTap, event);
        CFRelease(event);
    }
    std::thread::sleep(std::time::Duration::from_millis(50));
    Ok(crate::json::success())
}

pub fn mouse_click(button: &str) -> Result<String, String> {
    let (down_type, up_type, btn) = match button {
        "left" => (kCGEventLeftMouseDown, kCGEventLeftMouseUp, kCGMouseButtonLeft),
        "right" => (kCGEventRightMouseDown, kCGEventRightMouseUp, kCGMouseButtonRight),
        _ => return Err(format!("Unknown button: {}. Use 'left' or 'right'", button)),
    };

    // Get current mouse position
    let point = get_cursor_position()?;

    unsafe {
        let down = CGEventCreateMouseEvent(std::ptr::null(), down_type, point, btn);
        if down.is_null() {
            return Err("Failed to create mouse down event".to_string());
        }
        CGEventPost(kCGHIDEventTap, down);
        CFRelease(down);

        std::thread::sleep(std::time::Duration::from_millis(50));

        let up = CGEventCreateMouseEvent(std::ptr::null(), up_type, point, btn);
        if up.is_null() {
            return Err("Failed to create mouse up event".to_string());
        }
        CGEventPost(kCGHIDEventTap, up);
        CFRelease(up);
    }

    std::thread::sleep(std::time::Duration::from_millis(50));
    Ok(crate::json::success())
}

pub fn key_type(text: &str) -> Result<String, String> {
    // Attach the character to the event as a unicode string instead of
    // synthesizing keycodes: this types any character (accents, symbols,
    // emoji via surrogate pairs) and is independent of the active keyboard
    // layout. The keycode map below remains only for key_press combos.
    for c in text.chars() {
        let mut buf = [0u16; 2];
        let units = c.encode_utf16(&mut buf);
        unsafe {
            let down = CGEventCreateKeyboardEvent(std::ptr::null(), 0, true);
            if down.is_null() {
                return Err(format!("Failed to create keyboard event while typing '{}'", c));
            }
            CGEventKeyboardSetUnicodeString(down, units.len(), units.as_ptr());
            CGEventPost(kCGHIDEventTap, down);
            CFRelease(down);

            std::thread::sleep(std::time::Duration::from_millis(10));

            let up = CGEventCreateKeyboardEvent(std::ptr::null(), 0, false);
            if up.is_null() {
                return Err(format!("Failed to create keyboard event while typing '{}'", c));
            }
            CGEventKeyboardSetUnicodeString(up, units.len(), units.as_ptr());
            CGEventPost(kCGHIDEventTap, up);
            CFRelease(up);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    std::thread::sleep(std::time::Duration::from_millis(50));
    Ok(crate::json::success())
}

pub fn key_press(combo: &str) -> Result<String, String> {
    let parts = crate::keycombo::split_combo(combo)?;
    let mut keycodes: Vec<u16> = Vec::new();
    let mut flags: u64 = 0;

    for part in &parts {
        let (key, needs_shift) = modifier_to_keycode(&part.to_lowercase())
            .ok_or_else(|| format!("Unknown key: {}", part))?;

        if let Some(mask) = modifier_flag_for_keycode(key) {
            flags |= mask;
        }

        if needs_shift {
            flags |= kCGEventFlagMaskShift;
            if !keycodes.contains(&kVK_Shift) {
                keycodes.push(kVK_Shift);
            }
        }
        keycodes.push(key);
    }

    unsafe {
        // Press all keys down in order
        for (idx, &kc) in keycodes.iter().enumerate() {
            let event = make_key_event(kc, true, flags);
            if event.is_null() {
                // Release anything already pressed so modifiers don't stay
                // stuck down machine-wide, then report the failure.
                for &pressed in keycodes[..idx].iter().rev() {
                    let up = make_key_event(pressed, false, flags);
                    if !up.is_null() {
                        CGEventPost(kCGHIDEventTap, up);
                        CFRelease(up);
                    }
                }
                return Err(format!("Failed to create key-down event for '{}'", combo));
            }
            CGEventPost(kCGHIDEventTap, event);
            CFRelease(event);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        // Release in reverse order
        let release_order: Vec<u16> = keycodes.iter().rev().copied().collect();
        for (idx, &kc) in release_order.iter().enumerate() {
            let event = make_key_event(kc, false, flags);
            if event.is_null() {
                // A mid-release allocation failure must not leave a modifier
                // (e.g. cmd) held down machine-wide: best-effort release
                // everything still down — this key plus everything after it
                // in release order — before reporting the error.
                for &remaining in &release_order[idx..] {
                    let up = make_key_event(remaining, false, flags);
                    if !up.is_null() {
                        CGEventPost(kCGHIDEventTap, up);
                        CFRelease(up);
                    }
                }
                return Err(format!("Failed to create key-up event for '{}'", combo));
            }
            CGEventPost(kCGHIDEventTap, event);
            CFRelease(event);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    std::thread::sleep(std::time::Duration::from_millis(50));
    Ok(crate::json::success())
}

// --- Helpers ---

fn get_cursor_position() -> Result<CGPoint, String> {
    // Create a dummy mouse event to read current position
    // CGEventCreate returns an event at the current cursor position
    unsafe extern "C" {
        fn CGEventCreate(source: *const c_void) -> *mut c_void;
        fn CGEventGetLocation(event: *const c_void) -> CGPoint;
    }
    unsafe {
        let event = CGEventCreate(std::ptr::null());
        if event.is_null() {
            return Err("Failed to read cursor position (missing Accessibility permission?)".to_string());
        }
        let point = CGEventGetLocation(event);
        CFRelease(event);
        Ok(point)
    }
}

// --- macOS virtual keycodes ---

const kVK_ANSI_A: u16 = 0x00;
const kVK_ANSI_S: u16 = 0x01;
const kVK_ANSI_D: u16 = 0x02;
const kVK_ANSI_F: u16 = 0x03;
const kVK_ANSI_H: u16 = 0x04;
const kVK_ANSI_G: u16 = 0x05;
const kVK_ANSI_Z: u16 = 0x06;
const kVK_ANSI_X: u16 = 0x07;
const kVK_ANSI_C: u16 = 0x08;
const kVK_ANSI_V: u16 = 0x09;
const kVK_ANSI_B: u16 = 0x0B;
const kVK_ANSI_Q: u16 = 0x0C;
const kVK_ANSI_W: u16 = 0x0D;
const kVK_ANSI_E: u16 = 0x0E;
const kVK_ANSI_R: u16 = 0x0F;
const kVK_ANSI_Y: u16 = 0x10;
const kVK_ANSI_T: u16 = 0x11;
const kVK_ANSI_1: u16 = 0x12;
const kVK_ANSI_2: u16 = 0x13;
const kVK_ANSI_3: u16 = 0x14;
const kVK_ANSI_4: u16 = 0x15;
const kVK_ANSI_6: u16 = 0x16;
const kVK_ANSI_5: u16 = 0x17;
const kVK_ANSI_Equal: u16 = 0x18;
const kVK_ANSI_9: u16 = 0x19;
const kVK_ANSI_7: u16 = 0x1A;
const kVK_ANSI_Minus: u16 = 0x1B;
const kVK_ANSI_8: u16 = 0x1C;
const kVK_ANSI_0: u16 = 0x1D;
const kVK_ANSI_RightBracket: u16 = 0x1E;
const kVK_ANSI_O: u16 = 0x1F;
const kVK_ANSI_U: u16 = 0x20;
const kVK_ANSI_LeftBracket: u16 = 0x21;
const kVK_ANSI_I: u16 = 0x22;
const kVK_ANSI_P: u16 = 0x23;
const kVK_ANSI_L: u16 = 0x25;
const kVK_ANSI_J: u16 = 0x26;
const kVK_ANSI_Quote: u16 = 0x27;
const kVK_ANSI_K: u16 = 0x28;
const kVK_ANSI_Semicolon: u16 = 0x29;
const kVK_ANSI_Backslash: u16 = 0x2A;
const kVK_ANSI_Comma: u16 = 0x2B;
const kVK_ANSI_Slash: u16 = 0x2C;
const kVK_ANSI_N: u16 = 0x2D;
const kVK_ANSI_M: u16 = 0x2E;
const kVK_ANSI_Period: u16 = 0x2F;
const kVK_ANSI_Grave: u16 = 0x32;

const kVK_Return: u16 = 0x24;
const kVK_Tab: u16 = 0x30;
const kVK_Space: u16 = 0x31;
const kVK_Delete: u16 = 0x33;
const kVK_Escape: u16 = 0x35;
const kVK_Command: u16 = 0x37;
const kVK_Shift: u16 = 0x38;
const kVK_CapsLock: u16 = 0x39;
const kVK_Option: u16 = 0x3A;
const kVK_Control: u16 = 0x3B;
const kVK_RightShift: u16 = 0x3C;
const kVK_RightOption: u16 = 0x3D;
const kVK_RightControl: u16 = 0x3E;

const kVK_F1: u16 = 0x7A;
const kVK_F2: u16 = 0x78;
const kVK_F3: u16 = 0x63;
const kVK_F4: u16 = 0x76;
const kVK_F5: u16 = 0x60;
const kVK_F6: u16 = 0x61;
const kVK_F7: u16 = 0x62;
const kVK_F8: u16 = 0x64;
const kVK_F9: u16 = 0x65;
const kVK_F10: u16 = 0x6D;
const kVK_F11: u16 = 0x67;
const kVK_F12: u16 = 0x6F;

const kVK_ForwardDelete: u16 = 0x75;
const kVK_Home: u16 = 0x73;
const kVK_End: u16 = 0x77;
const kVK_PageUp: u16 = 0x74;
const kVK_PageDown: u16 = 0x79;
const kVK_UpArrow: u16 = 0x7E;
const kVK_DownArrow: u16 = 0x7D;
const kVK_LeftArrow: u16 = 0x7B;
const kVK_RightArrow: u16 = 0x7C;

/// Map a character to (macOS virtual keycode, needs_shift)
fn char_to_keycode(c: char) -> Option<(u16, bool)> {
    match c {
        'a' => Some((kVK_ANSI_A, false)), 'A' => Some((kVK_ANSI_A, true)),
        'b' => Some((kVK_ANSI_B, false)), 'B' => Some((kVK_ANSI_B, true)),
        'c' => Some((kVK_ANSI_C, false)), 'C' => Some((kVK_ANSI_C, true)),
        'd' => Some((kVK_ANSI_D, false)), 'D' => Some((kVK_ANSI_D, true)),
        'e' => Some((kVK_ANSI_E, false)), 'E' => Some((kVK_ANSI_E, true)),
        'f' => Some((kVK_ANSI_F, false)), 'F' => Some((kVK_ANSI_F, true)),
        'g' => Some((kVK_ANSI_G, false)), 'G' => Some((kVK_ANSI_G, true)),
        'h' => Some((kVK_ANSI_H, false)), 'H' => Some((kVK_ANSI_H, true)),
        'i' => Some((kVK_ANSI_I, false)), 'I' => Some((kVK_ANSI_I, true)),
        'j' => Some((kVK_ANSI_J, false)), 'J' => Some((kVK_ANSI_J, true)),
        'k' => Some((kVK_ANSI_K, false)), 'K' => Some((kVK_ANSI_K, true)),
        'l' => Some((kVK_ANSI_L, false)), 'L' => Some((kVK_ANSI_L, true)),
        'm' => Some((kVK_ANSI_M, false)), 'M' => Some((kVK_ANSI_M, true)),
        'n' => Some((kVK_ANSI_N, false)), 'N' => Some((kVK_ANSI_N, true)),
        'o' => Some((kVK_ANSI_O, false)), 'O' => Some((kVK_ANSI_O, true)),
        'p' => Some((kVK_ANSI_P, false)), 'P' => Some((kVK_ANSI_P, true)),
        'q' => Some((kVK_ANSI_Q, false)), 'Q' => Some((kVK_ANSI_Q, true)),
        'r' => Some((kVK_ANSI_R, false)), 'R' => Some((kVK_ANSI_R, true)),
        's' => Some((kVK_ANSI_S, false)), 'S' => Some((kVK_ANSI_S, true)),
        't' => Some((kVK_ANSI_T, false)), 'T' => Some((kVK_ANSI_T, true)),
        'u' => Some((kVK_ANSI_U, false)), 'U' => Some((kVK_ANSI_U, true)),
        'v' => Some((kVK_ANSI_V, false)), 'V' => Some((kVK_ANSI_V, true)),
        'w' => Some((kVK_ANSI_W, false)), 'W' => Some((kVK_ANSI_W, true)),
        'x' => Some((kVK_ANSI_X, false)), 'X' => Some((kVK_ANSI_X, true)),
        'y' => Some((kVK_ANSI_Y, false)), 'Y' => Some((kVK_ANSI_Y, true)),
        'z' => Some((kVK_ANSI_Z, false)), 'Z' => Some((kVK_ANSI_Z, true)),
        '0' => Some((kVK_ANSI_0, false)), ')' => Some((kVK_ANSI_0, true)),
        '1' => Some((kVK_ANSI_1, false)), '!' => Some((kVK_ANSI_1, true)),
        '2' => Some((kVK_ANSI_2, false)), '@' => Some((kVK_ANSI_2, true)),
        '3' => Some((kVK_ANSI_3, false)), '#' => Some((kVK_ANSI_3, true)),
        '4' => Some((kVK_ANSI_4, false)), '$' => Some((kVK_ANSI_4, true)),
        '5' => Some((kVK_ANSI_5, false)), '%' => Some((kVK_ANSI_5, true)),
        '6' => Some((kVK_ANSI_6, false)), '^' => Some((kVK_ANSI_6, true)),
        '7' => Some((kVK_ANSI_7, false)), '&' => Some((kVK_ANSI_7, true)),
        '8' => Some((kVK_ANSI_8, false)), '*' => Some((kVK_ANSI_8, true)),
        '9' => Some((kVK_ANSI_9, false)), '(' => Some((kVK_ANSI_9, true)),
        ' ' => Some((kVK_Space, false)),
        '\n' => Some((kVK_Return, false)),
        '\t' => Some((kVK_Tab, false)),
        '-' => Some((kVK_ANSI_Minus, false)), '_' => Some((kVK_ANSI_Minus, true)),
        '=' => Some((kVK_ANSI_Equal, false)), '+' => Some((kVK_ANSI_Equal, true)),
        '[' => Some((kVK_ANSI_LeftBracket, false)), '{' => Some((kVK_ANSI_LeftBracket, true)),
        ']' => Some((kVK_ANSI_RightBracket, false)), '}' => Some((kVK_ANSI_RightBracket, true)),
        ';' => Some((kVK_ANSI_Semicolon, false)), ':' => Some((kVK_ANSI_Semicolon, true)),
        '\'' => Some((kVK_ANSI_Quote, false)), '"' => Some((kVK_ANSI_Quote, true)),
        '`' => Some((kVK_ANSI_Grave, false)), '~' => Some((kVK_ANSI_Grave, true)),
        '\\' => Some((kVK_ANSI_Backslash, false)), '|' => Some((kVK_ANSI_Backslash, true)),
        ',' => Some((kVK_ANSI_Comma, false)), '<' => Some((kVK_ANSI_Comma, true)),
        '.' => Some((kVK_ANSI_Period, false)), '>' => Some((kVK_ANSI_Period, true)),
        '/' => Some((kVK_ANSI_Slash, false)), '?' => Some((kVK_ANSI_Slash, true)),
        _ => None,
    }
}

/// Map a modifier/key name to (macOS virtual keycode, needs_shift). Every
/// named key returns `needs_shift: false` — only the single-char arm (which
/// delegates to `char_to_keycode`) can require shift, e.g. "%" is shift+5.
fn modifier_to_keycode(name: &str) -> Option<(u16, bool)> {
    match name {
        "ctrl" | "control" => Some((kVK_Control, false)),
        "shift" => Some((kVK_Shift, false)),
        "alt" | "option" => Some((kVK_Option, false)),
        "super" | "meta" | "cmd" | "command" | "win" => Some((kVK_Command, false)),
        "tab" => Some((kVK_Tab, false)),
        "enter" | "return" => Some((kVK_Return, false)),
        "space" => Some((kVK_Space, false)),
        "backspace" | "delete" => Some((kVK_Delete, false)),
        "forwarddelete" | "del" => Some((kVK_ForwardDelete, false)),
        "escape" | "esc" => Some((kVK_Escape, false)),
        "up" => Some((kVK_UpArrow, false)),
        "down" => Some((kVK_DownArrow, false)),
        "left" => Some((kVK_LeftArrow, false)),
        "right" => Some((kVK_RightArrow, false)),
        "home" => Some((kVK_Home, false)),
        "end" => Some((kVK_End, false)),
        "pageup" => Some((kVK_PageUp, false)),
        "pagedown" => Some((kVK_PageDown, false)),
        "f1" => Some((kVK_F1, false)),
        "f2" => Some((kVK_F2, false)),
        "f3" => Some((kVK_F3, false)),
        "f4" => Some((kVK_F4, false)),
        "f5" => Some((kVK_F5, false)),
        "f6" => Some((kVK_F6, false)),
        "f7" => Some((kVK_F7, false)),
        "f8" => Some((kVK_F8, false)),
        "f9" => Some((kVK_F9, false)),
        "f10" => Some((kVK_F10, false)),
        "f11" => Some((kVK_F11, false)),
        "f12" => Some((kVK_F12, false)),
        s if s.len() == 1 => char_to_keycode(s.chars().next().unwrap()),
        _ => None,
    }
}

/// The CGEventFlags mask contributed by a modifier keycode, or None if `kc`
/// is not one of the four base modifiers. Deriving the mask from the
/// resolved keycode (rather than re-matching the source name) keeps the
/// name→flag mapping in one place: `modifier_to_keycode` above.
fn modifier_flag_for_keycode(kc: u16) -> Option<u64> {
    match kc {
        kVK_Command => Some(kCGEventFlagMaskCommand),
        kVK_Shift => Some(kCGEventFlagMaskShift),
        kVK_Control => Some(kCGEventFlagMaskControl),
        kVK_Option => Some(kCGEventFlagMaskAlternate),
        _ => None,
    }
}

/// Create a keyboard event and, for a non-modifier ("action") key, stamp it
/// with the combo's flags mask before returning. This is Apple's documented
/// pattern for synthetic hotkeys: session-state propagation between
/// separately posted modifier down/up events is timing-dependent, and
/// without it `cmd+a` occasionally typed a bare "a". Modifier key events
/// (cmd/shift/ctrl/option themselves) keep default flags. Returns a null
/// pointer on allocation failure, same as `CGEventCreateKeyboardEvent`.
unsafe fn make_key_event(kc: u16, key_down: bool, flags: u64) -> *mut c_void {
    unsafe {
        let event = CGEventCreateKeyboardEvent(std::ptr::null(), kc, key_down);
        if !event.is_null() && modifier_flag_for_keycode(kc).is_none() {
            CGEventSetFlags(event, flags);
        }
        event
    }
}

// This whole module is `#[cfg(target_os = "macos")]`-gated at
// `platform/mod.rs`, so these tests only compile and run in CI's macos job
// (or on macOS hardware) — never under a Linux-host `cargo test`. They cover
// pure keycode/flag mapping only; nothing here touches the CoreGraphics FFI.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_modifiers_never_need_shift() {
        for name in ["ctrl", "control", "shift", "alt", "option", "cmd", "command", "super", "meta", "win"] {
            let (_, needs_shift) = modifier_to_keycode(name).unwrap_or_else(|| panic!("{} should resolve", name));
            assert!(!needs_shift, "{} should not need shift", name);
        }
    }

    #[test]
    fn win_and_cmd_resolve_to_the_same_keycode() {
        // "win" is accepted as a Command alias so combos behave the same as
        // the Linux and Windows backends, which both treat win/super/meta
        // as the platform "super" key.
        assert_eq!(modifier_to_keycode("win"), modifier_to_keycode("cmd"));
        assert_eq!(modifier_to_keycode("win").unwrap().0, kVK_Command);
    }

    #[test]
    fn uppercase_letter_needs_shift() {
        let (key, needs_shift) = modifier_to_keycode("A").unwrap();
        assert_eq!(key, kVK_ANSI_A);
        assert!(needs_shift);
    }

    #[test]
    fn lowercase_letter_does_not_need_shift() {
        let (key, needs_shift) = modifier_to_keycode("a").unwrap();
        assert_eq!(key, kVK_ANSI_A);
        assert!(!needs_shift);
    }

    #[test]
    fn shifted_symbol_needs_shift() {
        // "+" is shift+= on a US layout.
        let (key, needs_shift) = modifier_to_keycode("+").unwrap();
        assert_eq!(key, kVK_ANSI_Equal);
        assert!(needs_shift);
    }

    #[test]
    fn unknown_key_name_is_none() {
        assert!(modifier_to_keycode("nonexistent_key").is_none());
    }

    #[test]
    fn modifier_flag_for_keycode_covers_all_four_modifiers() {
        assert_eq!(modifier_flag_for_keycode(kVK_Command), Some(kCGEventFlagMaskCommand));
        assert_eq!(modifier_flag_for_keycode(kVK_Shift), Some(kCGEventFlagMaskShift));
        assert_eq!(modifier_flag_for_keycode(kVK_Control), Some(kCGEventFlagMaskControl));
        assert_eq!(modifier_flag_for_keycode(kVK_Option), Some(kCGEventFlagMaskAlternate));
    }

    #[test]
    fn modifier_flag_for_keycode_is_none_for_action_keys() {
        assert_eq!(modifier_flag_for_keycode(kVK_ANSI_A), None);
        assert_eq!(modifier_flag_for_keycode(kVK_Return), None);
    }
}
