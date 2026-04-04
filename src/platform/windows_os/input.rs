use super::ffi::*;

pub fn mouse_move(x: i32, y: i32) -> Result<String, String> {
    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };

    if screen_w <= 0 || screen_h <= 0 {
        return Err("Failed to get screen dimensions".to_string());
    }

    // Normalize to 0-65535 range for MOUSEEVENTF_ABSOLUTE
    let norm_x = (x as i64 * 65535 / screen_w as i64) as i32;
    let norm_y = (y as i64 * 65535 / screen_h as i64) as i32;

    let input = mouse_input(norm_x, norm_y, MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE);
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
    for c in text.chars() {
        let (vk, shift) = char_to_vk(c)?;

        if shift {
            send_key(VK_SHIFT, false)?;
        }
        send_key(vk, false)?;
        std::thread::sleep(std::time::Duration::from_millis(10));
        send_key(vk, true)?;
        if shift {
            send_key(VK_SHIFT, true)?;
        }

        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    std::thread::sleep(std::time::Duration::from_millis(50));
    Ok(crate::json::success())
}

pub fn key_press(combo: &str) -> Result<String, String> {
    let parts: Vec<&str> = combo.split('+').collect();
    let mut keycodes: Vec<u16> = Vec::new();
    for part in &parts {
        let vk = modifier_to_vk(&part.to_lowercase())
            .ok_or_else(|| format!("Unknown key: {}", part))?;
        keycodes.push(vk);
    }

    // Press all keys down in order
    for &vk in &keycodes {
        send_key(vk, false)?;
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    // Release in reverse order
    for &vk in keycodes.iter().rev() {
        send_key(vk, true)?;
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    std::thread::sleep(std::time::Duration::from_millis(50));
    Ok(crate::json::success())
}

// --- Helpers ---

fn send_key(vk: u16, key_up: bool) -> Result<(), String> {
    let flags = if key_up { KEYEVENTF_KEYUP } else { 0 };
    let input = keyboard_input(vk, flags);
    let sent = unsafe { SendInput(1, &input, input_size()) };
    if sent != 1 {
        Err(format!("Failed to send key event (vk=0x{:02X})", vk))
    } else {
        Ok(())
    }
}

/// Map a character to (virtual keycode, needs_shift) using VkKeyScanW.
fn char_to_vk(c: char) -> Result<(u16, bool), String> {
    let result = unsafe { VkKeyScanW(c as u16) };
    if result == -1 {
        return Err(format!("Cannot map character '{}' to a virtual key", c));
    }
    let vk = (result & 0xFF) as u16;
    let shift = (result >> 8) & 1 != 0;
    Ok((vk, shift))
}

/// Map a modifier/key name to a Windows virtual keycode.
fn modifier_to_vk(name: &str) -> Option<u16> {
    match name {
        "ctrl" | "control" => Some(VK_CONTROL),
        "shift" => Some(VK_SHIFT),
        "alt" => Some(VK_MENU),
        "super" | "meta" | "win" => Some(VK_LWIN),
        "tab" => Some(VK_TAB),
        "enter" | "return" => Some(VK_RETURN),
        "space" => Some(VK_SPACE),
        "backspace" => Some(VK_BACK),
        "delete" | "del" => Some(VK_DELETE),
        "escape" | "esc" => Some(VK_ESCAPE),
        "up" => Some(VK_UP),
        "down" => Some(VK_DOWN),
        "left" => Some(VK_LEFT),
        "right" => Some(VK_RIGHT),
        "home" => Some(VK_HOME),
        "end" => Some(VK_END),
        "pageup" => Some(VK_PRIOR),
        "pagedown" => Some(VK_NEXT),
        "f1" => Some(VK_F1),
        "f2" => Some(VK_F2),
        "f3" => Some(VK_F3),
        "f4" => Some(VK_F4),
        "f5" => Some(VK_F5),
        "f6" => Some(VK_F6),
        "f7" => Some(VK_F7),
        "f8" => Some(VK_F8),
        "f9" => Some(VK_F9),
        "f10" => Some(VK_F10),
        "f11" => Some(VK_F11),
        "f12" => Some(VK_F12),
        // Single printable character — use VkKeyScanW
        s if s.len() == 1 => {
            let c = s.chars().next().unwrap();
            let result = unsafe { VkKeyScanW(c as u16) };
            if result == -1 { None } else { Some((result & 0xFF) as u16) }
        }
        _ => None,
    }
}
