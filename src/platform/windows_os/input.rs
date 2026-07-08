use super::ffi::*;

pub fn mouse_move(x: i32, y: i32) -> Result<String, String> {
    // Normalize over the virtual desktop (all monitors). MOUSEEVENTF_ABSOLUTE
    // alone maps 0..65535 onto the primary monitor only, which makes
    // secondary monitors unreachable and negative coordinates inexpressible.
    let vx = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let vy = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let vw = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let vh = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };

    if vw <= 1 || vh <= 1 {
        return Err("Failed to get virtual screen dimensions".to_string());
    }

    // Round like MulDiv(dx, 65535, span): plain truncation lands up to 1px
    // short at the right/bottom edges (scrollbars live there)
    let span_x = (vw - 1) as i64;
    let span_y = (vh - 1) as i64;
    let norm_x = (((x - vx) as i64 * 65535 + span_x / 2) / span_x) as i32;
    let norm_y = (((y - vy) as i64 * 65535 + span_y / 2) / span_y) as i32;

    let input = mouse_input(norm_x, norm_y, MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK);
    let sent = unsafe { SendInput(1, &input, input_size()) };
    if sent != 1 {
        return Err("Failed to send mouse move event".to_string());
    }

    std::thread::sleep(std::time::Duration::from_millis(50));
    Ok(crate::json::success())
}

pub fn mouse_click(button: &str) -> Result<String, String> {
    let (down_flags, up_flags) = match button {
        "left" => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
        "right" => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
        _ => return Err(format!("Unknown button: {}. Use 'left' or 'right'", button)),
    };

    let down = mouse_input(0, 0, down_flags);
    let sent = unsafe { SendInput(1, &down, input_size()) };
    if sent != 1 {
        return Err("Failed to send mouse down event".to_string());
    }

    std::thread::sleep(std::time::Duration::from_millis(50));

    let up = mouse_input(0, 0, up_flags);
    let sent = unsafe { SendInput(1, &up, input_size()) };
    if sent != 1 {
        return Err("Failed to send mouse up event".to_string());
    }

    std::thread::sleep(std::time::Duration::from_millis(50));
    Ok(crate::json::success())
}

pub fn key_type(text: &str) -> Result<String, String> {
    // KEYEVENTF_UNICODE types the character itself, independent of the
    // active keyboard layout — this covers accents, AltGr characters, and
    // non-BMP chars (each UTF-16 unit, including surrogate halves, is sent
    // as its own down/up pair, which receivers reassemble). Bare U+000A is
    // dropped by most Windows edit controls, so newlines are sent as an
    // explicit Enter keypress instead of encoded as unicode input.
    for ch in text.chars() {
        match ch {
            '\r' => continue,
            '\n' => {
                send_key(VK_RETURN, false)?;
                std::thread::sleep(std::time::Duration::from_millis(10));
                send_key(VK_RETURN, true)?;
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            _ => {
                let mut buf = [0u16; 2];
                for &mut unit in ch.encode_utf16(&mut buf) {
                    send_unicode(unit, false)?;
                    std::thread::sleep(std::time::Duration::from_millis(10));
                    send_unicode(unit, true)?;
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
        }
    }

    std::thread::sleep(std::time::Duration::from_millis(50));
    Ok(crate::json::success())
}

pub fn key_press(combo: &str) -> Result<String, String> {
    let parts = crate::keycombo::split_combo(combo)?;
    let mut keycodes: Vec<u16> = Vec::new();
    for part in &parts {
        let (vk, state) = modifier_to_vk(&part.to_lowercase())
            .ok_or_else(|| format!("Unknown key: {}", part))?;
        // VkKeyScanW's shift-state byte tells us which modifiers the target
        // key needs (e.g. '!' needs shift, '%' needs shift too on a US
        // layout); push any not already present earlier in the sequence so
        // an explicit "shift+1" doesn't press shift twice.
        push_required_modifiers(&mut keycodes, state);
        keycodes.push(vk);
    }

    // Press all keys down in order
    for (idx, &vk) in keycodes.iter().enumerate() {
        if let Err(e) = send_key(vk, false) {
            // Release anything already pressed so modifiers don't stay stuck
            // down machine-wide, then report the failure.
            for &pressed in keycodes[..idx].iter().rev() {
                let _ = send_key(pressed, true);
            }
            return Err(e);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    // Release in reverse order, attempting every key even if one fails
    let mut release_err = None;
    for &vk in keycodes.iter().rev() {
        if let Err(e) = send_key(vk, true) {
            release_err.get_or_insert(e);
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    if let Some(e) = release_err {
        return Err(e);
    }

    std::thread::sleep(std::time::Duration::from_millis(50));
    Ok(crate::json::success())
}

// --- Helpers ---

fn send_key(vk: u16, key_up: bool) -> Result<(), String> {
    let mut flags = if key_up { KEYEVENTF_KEYUP } else { 0 };
    if is_extended(vk) {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    let input = keyboard_input(vk, flags);
    let sent = unsafe { SendInput(1, &input, input_size()) };
    if sent != 1 {
        Err(format!("Failed to send key event (vk=0x{:02X})", vk))
    } else {
        Ok(())
    }
}

/// Nav-cluster keys live on the "extended" keyboard (originally the
/// separate arrow/ins-del block on 101-key keyboards) and must carry
/// KEYEVENTF_EXTENDEDKEY or some receivers read them as their numpad
/// counterparts instead (e.g. VK_DELETE without the flag can be interpreted
/// as numpad-period).
fn is_extended(vk: u16) -> bool {
    matches!(
        vk,
        VK_UP | VK_DOWN | VK_LEFT | VK_RIGHT | VK_HOME | VK_END | VK_PRIOR | VK_NEXT | VK_DELETE
    )
}

/// Push VK_SHIFT/VK_CONTROL/VK_MENU implied by a VkKeyScanW shift-state byte
/// (bit 0 = shift, bit 1 = ctrl, bit 2 = alt), skipping any modifier already
/// present earlier in the sequence.
fn push_required_modifiers(keycodes: &mut Vec<u16>, state: u8) {
    const IMPLIED: [(u8, u16); 3] = [(0b001, VK_SHIFT), (0b010, VK_CONTROL), (0b100, VK_MENU)];
    for (bit, vk) in IMPLIED {
        if state & bit != 0 && !keycodes.contains(&vk) {
            keycodes.push(vk);
        }
    }
}

fn send_unicode(unit: u16, key_up: bool) -> Result<(), String> {
    let flags = if key_up { KEYEVENTF_KEYUP } else { 0 };
    let input = keyboard_unicode_input(unit, flags);
    let sent = unsafe { SendInput(1, &input, input_size()) };
    if sent != 1 {
        Err(format!("Failed to send unicode key event (U+{:04X})", unit))
    } else {
        Ok(())
    }
}

/// Map a modifier/key name to a Windows virtual keycode and the
/// VkKeyScanW shift-state byte it requires (0 for named keys, which never
/// need an implied modifier to reach the key itself).
fn modifier_to_vk(name: &str) -> Option<(u16, u8)> {
    match name {
        "ctrl" | "control" => Some((VK_CONTROL, 0)),
        "shift" => Some((VK_SHIFT, 0)),
        "alt" => Some((VK_MENU, 0)),
        "super" | "meta" | "win" => Some((VK_LWIN, 0)),
        "tab" => Some((VK_TAB, 0)),
        "enter" | "return" => Some((VK_RETURN, 0)),
        "space" => Some((VK_SPACE, 0)),
        "backspace" => Some((VK_BACK, 0)),
        "delete" | "del" => Some((VK_DELETE, 0)),
        "escape" | "esc" => Some((VK_ESCAPE, 0)),
        "up" => Some((VK_UP, 0)),
        "down" => Some((VK_DOWN, 0)),
        "left" => Some((VK_LEFT, 0)),
        "right" => Some((VK_RIGHT, 0)),
        "home" => Some((VK_HOME, 0)),
        "end" => Some((VK_END, 0)),
        "pageup" => Some((VK_PRIOR, 0)),
        "pagedown" => Some((VK_NEXT, 0)),
        "f1" => Some((VK_F1, 0)),
        "f2" => Some((VK_F2, 0)),
        "f3" => Some((VK_F3, 0)),
        "f4" => Some((VK_F4, 0)),
        "f5" => Some((VK_F5, 0)),
        "f6" => Some((VK_F6, 0)),
        "f7" => Some((VK_F7, 0)),
        "f8" => Some((VK_F8, 0)),
        "f9" => Some((VK_F9, 0)),
        "f10" => Some((VK_F10, 0)),
        "f11" => Some((VK_F11, 0)),
        "f12" => Some((VK_F12, 0)),
        // Single printable character — use VkKeyScanW. Match on char count,
        // not byte length, so a single multi-byte-UTF-8 character (e.g. an
        // accented letter) still takes this arm instead of falling to the
        // catch-all.
        s if s.chars().count() == 1 => {
            let c = s.chars().next().unwrap();
            let result = unsafe { VkKeyScanW(c as u16) };
            if result == -1 {
                None
            } else {
                Some(((result & 0xFF) as u16, ((result >> 8) & 0xFF) as u8))
            }
        }
        _ => None,
    }
}

// This whole module is `#[cfg(target_os = "windows")]`-gated at
// `platform/mod.rs`, so these tests only compile and run in CI's windows
// job (or on Windows hardware) — never under a Linux-host `cargo test`.
// They cover pure keycode/flag mapping only; nothing here touches SendInput.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_modifiers_never_need_extra_state() {
        for name in ["ctrl", "control", "shift", "alt", "super", "meta", "win", "enter", "delete"] {
            let (_, state) = modifier_to_vk(name).unwrap_or_else(|| panic!("{} should resolve", name));
            assert_eq!(state, 0, "{} should not carry shift-state bits", name);
        }
    }

    #[test]
    fn unknown_name_is_none() {
        assert!(modifier_to_vk("not_a_real_key").is_none());
    }

    #[test]
    fn is_extended_covers_nav_cluster() {
        for vk in [VK_UP, VK_DOWN, VK_LEFT, VK_RIGHT, VK_HOME, VK_END, VK_PRIOR, VK_NEXT, VK_DELETE] {
            assert!(is_extended(vk));
        }
    }

    #[test]
    fn is_extended_excludes_ordinary_keys() {
        for vk in [VK_RETURN, VK_SPACE, VK_TAB, VK_SHIFT, VK_CONTROL, VK_MENU, VK_F1] {
            assert!(!is_extended(vk));
        }
    }

    #[test]
    fn push_required_modifiers_dedupes_already_present() {
        // Simulates "shift+1": VK_SHIFT is already in the sequence from the
        // explicit "shift" part, so a shifted second key must not add it again.
        let mut keycodes = vec![VK_SHIFT];
        push_required_modifiers(&mut keycodes, 0b001);
        assert_eq!(keycodes, vec![VK_SHIFT]);
    }

    #[test]
    fn push_required_modifiers_adds_missing_bits() {
        // Simulates "ctrl+%": ctrl is explicit, shift is implied by '%' and
        // must be inserted since it isn't already in the sequence.
        let mut keycodes = vec![VK_CONTROL];
        push_required_modifiers(&mut keycodes, 0b001);
        assert_eq!(keycodes, vec![VK_CONTROL, VK_SHIFT]);
    }

    #[test]
    fn push_required_modifiers_handles_multiple_bits() {
        let mut keycodes = vec![];
        push_required_modifiers(&mut keycodes, 0b111);
        assert_eq!(keycodes, vec![VK_SHIFT, VK_CONTROL, VK_MENU]);
    }

    #[test]
    fn push_required_modifiers_noop_for_zero_state() {
        let mut keycodes = vec![];
        push_required_modifiers(&mut keycodes, 0);
        assert!(keycodes.is_empty());
    }
}
