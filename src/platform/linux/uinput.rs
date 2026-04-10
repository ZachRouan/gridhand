use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;

#[cfg(target_arch = "x86_64")]
unsafe fn ioctl(fd: i32, request: u64, arg: u64) -> i64 {
    let ret: i64;
    unsafe {
        std::arch::asm!(
            "syscall",
            in("rax") 16u64, // __NR_ioctl on x86_64
            in("rdi") fd as u64,
            in("rsi") request,
            in("rdx") arg,
            lateout("rax") ret,
            out("rcx") _,
            out("r11") _,
            options(nostack),
        );
    }
    ret
}

#[cfg(target_arch = "aarch64")]
unsafe fn ioctl(fd: i32, request: u64, arg: u64) -> i64 {
    let ret: i64;
    unsafe {
        std::arch::asm!(
            "svc #0",
            in("x8") 29u64, // __NR_ioctl on aarch64
            in("x0") fd as u64,
            in("x1") request,
            in("x2") arg,
            lateout("x0") ret,
            options(nostack),
        );
    }
    ret
}

// uinput ioctl commands
const UI_SET_EVBIT: u64 = 0x40045564;
const UI_SET_KEYBIT: u64 = 0x40045565;
const UI_SET_RELBIT: u64 = 0x40045566;
const UI_SET_ABSBIT: u64 = 0x40045567;
const UI_DEV_CREATE: u64 = 0x5501;
const UI_DEV_DESTROY: u64 = 0x5502;

// Event types
const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_REL: u16 = 0x02;
const EV_ABS: u16 = 0x03;

// Sync
const SYN_REPORT: u16 = 0;

// Relative axes
const REL_X: u16 = 0x00;
const REL_Y: u16 = 0x01;

// Absolute axes
const ABS_X: u16 = 0x00;
const ABS_Y: u16 = 0x01;

// Mouse buttons
const BTN_LEFT: u16 = 0x110;
const BTN_RIGHT: u16 = 0x111;
#[allow(dead_code)]
const BTN_MOUSE: u16 = 0x110; // alias

// Modifier keys
const KEY_LEFTCTRL: u16 = 29;
const KEY_LEFTSHIFT: u16 = 42;
const KEY_LEFTALT: u16 = 56;
const KEY_LEFTMETA: u16 = 125;
#[allow(dead_code)]
const KEY_RIGHTCTRL: u16 = 97;
#[allow(dead_code)]
const KEY_RIGHTSHIFT: u16 = 54;
#[allow(dead_code)]
const KEY_RIGHTALT: u16 = 100;
const KEY_TAB: u16 = 15;
const KEY_ENTER: u16 = 28;
const KEY_SPACE: u16 = 57;
const KEY_BACKSPACE: u16 = 14;
const KEY_ESC: u16 = 1;
const KEY_DELETE: u16 = 111;
const KEY_UP: u16 = 103;
const KEY_DOWN: u16 = 108;
const KEY_LEFT: u16 = 105;
const KEY_RIGHT: u16 = 106;
const KEY_HOME: u16 = 102;
const KEY_END: u16 = 107;
const KEY_PAGEUP: u16 = 104;
const KEY_PAGEDOWN: u16 = 109;

// F-keys
const KEY_F1: u16 = 59;
const KEY_F2: u16 = 60;
const KEY_F3: u16 = 61;
const KEY_F4: u16 = 62;
const KEY_F5: u16 = 63;
const KEY_F6: u16 = 64;
const KEY_F7: u16 = 65;
const KEY_F8: u16 = 66;
const KEY_F9: u16 = 67;
const KEY_F10: u16 = 68;
const KEY_F11: u16 = 87;
const KEY_F12: u16 = 88;

// Punctuation
const KEY_MINUS: u16 = 12;
const KEY_EQUAL: u16 = 13;
const KEY_LEFTBRACE: u16 = 26;
const KEY_RIGHTBRACE: u16 = 27;
const KEY_SEMICOLON: u16 = 39;
const KEY_APOSTROPHE: u16 = 40;
const KEY_GRAVE: u16 = 41;
const KEY_BACKSLASH: u16 = 43;
const KEY_COMMA: u16 = 51;
const KEY_DOT: u16 = 52;
const KEY_SLASH: u16 = 53;

// Number row
const KEY_1: u16 = 2;
const KEY_2: u16 = 3;
const KEY_3: u16 = 4;
const KEY_4: u16 = 5;
const KEY_5: u16 = 6;
const KEY_6: u16 = 7;
const KEY_7: u16 = 8;
const KEY_8: u16 = 9;
const KEY_9: u16 = 10;
const KEY_0: u16 = 11;

// Letters (QWERTY scancode order)
const KEY_Q: u16 = 16;
const KEY_W: u16 = 17;
const KEY_E: u16 = 18;
const KEY_R: u16 = 19;
const KEY_T: u16 = 20;
const KEY_Y: u16 = 21;
const KEY_U: u16 = 22;
const KEY_I: u16 = 23;
const KEY_O: u16 = 24;
const KEY_P: u16 = 25;
const KEY_A: u16 = 30;
const KEY_S: u16 = 31;
const KEY_D: u16 = 32;
const KEY_F: u16 = 33;
const KEY_G: u16 = 34;
const KEY_H: u16 = 35;
const KEY_J: u16 = 36;
const KEY_K: u16 = 37;
const KEY_L: u16 = 38;
const KEY_Z: u16 = 44;
const KEY_X: u16 = 45;
const KEY_C: u16 = 46;
const KEY_V: u16 = 47;
const KEY_B: u16 = 48;
const KEY_N: u16 = 49;
const KEY_M: u16 = 50;

const O_NONBLOCK: i32 = 0o4000;
const UINPUT_DEV_SIZE: usize = 1116;
const INPUT_EVENT_SIZE: usize = 24;

/// Parse a DRM mode string like "1920x1080" into (width, height).
fn parse_mode(s: &str) -> Option<(i32, i32)> {
    let (w, h) = s.split_once('x')?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

/// Detect total screen dimensions by reading connected DRM outputs from sysfs.
/// Falls back to framebuffer virtual_size, then to 1920x1080.
fn detect_screen_size() -> (i32, i32) {
    // Try DRM sysfs: /sys/class/drm/card*-*/status + modes
    if let Ok(entries) = std::fs::read_dir("/sys/class/drm") {
        let mut total_width: i32 = 0;
        let mut max_height: i32 = 0;
        let mut found = false;

        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            // Output entries look like "card0-HDMI-A-1", "card0-DP-1", "card0-eDP-1"
            // Skip bare "card0", "renderD128", etc.
            if !name_str.starts_with("card") || !name_str.contains('-') {
                continue;
            }
            let path = entry.path();
            let Ok(status) = std::fs::read_to_string(path.join("status")) else {
                continue;
            };
            if status.trim() != "connected" {
                continue;
            }
            let Ok(modes) = std::fs::read_to_string(path.join("modes")) else {
                continue;
            };
            let Some(first) = modes.lines().next() else {
                continue;
            };
            let Some((w, h)) = parse_mode(first) else {
                continue;
            };
            total_width += w;
            if h > max_height {
                max_height = h;
            }
            found = true;
        }

        if found {
            return (total_width, max_height);
        }
    }

    // Fallback: framebuffer virtual_size (format: "1920,1080")
    if let Ok(vs) = std::fs::read_to_string("/sys/class/graphics/fb0/virtual_size") {
        if let Some((w, h)) = vs.trim().split_once(',') {
            if let (Ok(w), Ok(h)) = (w.parse::<i32>(), h.parse::<i32>()) {
                return (w, h);
            }
        }
    }

    // Ultimate fallback
    (1920, 1080)
}

struct UinputDevice {
    file: File,
}

impl UinputDevice {
    fn create(name: &str, abs: bool) -> Result<Self, String> {
        let file = OpenOptions::new()
            .write(true)
            .custom_flags(O_NONBLOCK)
            .open("/dev/uinput")
            .map_err(|e| format!("Failed to open /dev/uinput: {}. Is user in 'input' group?", e))?;

        let fd = file.as_raw_fd();

        unsafe {
            // Enable EV_KEY (for keyboard keys and mouse buttons)
            check_ioctl(ioctl(fd, UI_SET_EVBIT, EV_KEY as u64), "UI_SET_EVBIT EV_KEY")?;

            // Enable EV_SYN
            check_ioctl(ioctl(fd, UI_SET_EVBIT, EV_SYN as u64), "UI_SET_EVBIT EV_SYN")?;

            // Enable all keys we might need (0..255 covers all standard keys)
            for code in 0u64..256 {
                ioctl(fd, UI_SET_KEYBIT, code);
            }

            // Enable mouse buttons
            ioctl(fd, UI_SET_KEYBIT, BTN_LEFT as u64);
            ioctl(fd, UI_SET_KEYBIT, BTN_RIGHT as u64);

            if abs {
                // Absolute positioning (for mouse move to coordinates)
                check_ioctl(ioctl(fd, UI_SET_EVBIT, EV_ABS as u64), "UI_SET_EVBIT EV_ABS")?;
                check_ioctl(ioctl(fd, UI_SET_ABSBIT, ABS_X as u64), "UI_SET_ABSBIT ABS_X")?;
                check_ioctl(ioctl(fd, UI_SET_ABSBIT, ABS_Y as u64), "UI_SET_ABSBIT ABS_Y")?;
            } else {
                // Relative mouse (for scroll, relative moves)
                check_ioctl(ioctl(fd, UI_SET_EVBIT, EV_REL as u64), "UI_SET_EVBIT EV_REL")?;
                check_ioctl(ioctl(fd, UI_SET_RELBIT, REL_X as u64), "UI_SET_RELBIT REL_X")?;
                check_ioctl(ioctl(fd, UI_SET_RELBIT, REL_Y as u64), "UI_SET_RELBIT REL_Y")?;
            }

            // Write uinput_user_dev struct (1116 bytes)
            let mut dev = [0u8; UINPUT_DEV_SIZE];
            let name_bytes = name.as_bytes();
            let len = name_bytes.len().min(79);
            dev[..len].copy_from_slice(&name_bytes[..len]);
            // BUS_VIRTUAL = 0x06 at offset 80
            dev[80] = 0x06;

            if abs {
                let (sw, sh) = detect_screen_size();
                // absmax[ABS_X] at offset 92 (i32 little-endian)
                let w = sw.to_le_bytes();
                dev[92..96].copy_from_slice(&w);
                // absmax[ABS_Y] at offset 96
                let h = sh.to_le_bytes();
                dev[96..100].copy_from_slice(&h);
            }

            let mut f = file.try_clone().map_err(|e| e.to_string())?;
            f.write_all(&dev).map_err(|e| format!("Failed to write uinput_user_dev: {}", e))?;

            check_ioctl(ioctl(fd, UI_DEV_CREATE, 0), "UI_DEV_CREATE")?;
        }

        // Wait for display server to detect the new input device.
        // Xorg (inotify) and Wayland (libinput/udev) typically bind in <20ms.
        std::thread::sleep(std::time::Duration::from_millis(50));

        Ok(UinputDevice { file })
    }

    fn write_event(&mut self, type_: u16, code: u16, value: i32) -> Result<(), String> {
        let mut buf = [0u8; INPUT_EVENT_SIZE];
        // tv_sec and tv_usec = 0 (kernel fills them)
        // type_ at offset 16
        buf[16..18].copy_from_slice(&type_.to_le_bytes());
        // code at offset 18
        buf[18..20].copy_from_slice(&code.to_le_bytes());
        // value at offset 20
        buf[20..24].copy_from_slice(&value.to_le_bytes());
        self.file.write_all(&buf).map_err(|e| format!("Failed to write event: {}", e))
    }

    fn syn(&mut self) -> Result<(), String> {
        self.write_event(EV_SYN, SYN_REPORT, 0)
    }

    fn key_down(&mut self, code: u16) -> Result<(), String> {
        self.write_event(EV_KEY, code, 1)?;
        self.syn()
    }

    fn key_up(&mut self, code: u16) -> Result<(), String> {
        self.write_event(EV_KEY, code, 0)?;
        self.syn()
    }

    fn tap_key(&mut self, code: u16) -> Result<(), String> {
        self.key_down(code)?;
        std::thread::sleep(std::time::Duration::from_millis(10));
        self.key_up(code)
    }
}

impl Drop for UinputDevice {
    fn drop(&mut self) {
        unsafe {
            ioctl(self.file.as_raw_fd(), UI_DEV_DESTROY, 0);
        }
    }
}

fn check_ioctl(ret: i64, name: &str) -> Result<(), String> {
    if ret < 0 {
        Err(format!("ioctl {} failed with error {}", name, -ret))
    } else {
        Ok(())
    }
}

/// Maps a character to (keycode, needs_shift)
fn char_to_key(c: char) -> Option<(u16, bool)> {
    match c {
        'a'..='z' => {
            let keys = [KEY_A, KEY_B, KEY_C, KEY_D, KEY_E, KEY_F, KEY_G, KEY_H,
                        KEY_I, KEY_J, KEY_K, KEY_L, KEY_M, KEY_N, KEY_O, KEY_P,
                        KEY_Q, KEY_R, KEY_S, KEY_T, KEY_U, KEY_V, KEY_W, KEY_X,
                        KEY_Y, KEY_Z];
            Some((keys[(c as u8 - b'a') as usize], false))
        }
        'A'..='Z' => {
            let keys = [KEY_A, KEY_B, KEY_C, KEY_D, KEY_E, KEY_F, KEY_G, KEY_H,
                        KEY_I, KEY_J, KEY_K, KEY_L, KEY_M, KEY_N, KEY_O, KEY_P,
                        KEY_Q, KEY_R, KEY_S, KEY_T, KEY_U, KEY_V, KEY_W, KEY_X,
                        KEY_Y, KEY_Z];
            Some((keys[(c as u8 - b'A') as usize], true))
        }
        '0' => Some((KEY_0, false)),
        '1'..='9' => Some((KEY_1 + (c as u8 - b'1') as u16, false)),
        ' ' => Some((KEY_SPACE, false)),
        '\n' => Some((KEY_ENTER, false)),
        '\t' => Some((KEY_TAB, false)),
        '-' => Some((KEY_MINUS, false)),
        '=' => Some((KEY_EQUAL, false)),
        '[' => Some((KEY_LEFTBRACE, false)),
        ']' => Some((KEY_RIGHTBRACE, false)),
        ';' => Some((KEY_SEMICOLON, false)),
        '\'' => Some((KEY_APOSTROPHE, false)),
        '`' => Some((KEY_GRAVE, false)),
        '\\' => Some((KEY_BACKSLASH, false)),
        ',' => Some((KEY_COMMA, false)),
        '.' => Some((KEY_DOT, false)),
        '/' => Some((KEY_SLASH, false)),
        // Shifted symbols
        '!' => Some((KEY_1, true)),
        '@' => Some((KEY_2, true)),
        '#' => Some((KEY_3, true)),
        '$' => Some((KEY_4, true)),
        '%' => Some((KEY_5, true)),
        '^' => Some((KEY_6, true)),
        '&' => Some((KEY_7, true)),
        '*' => Some((KEY_8, true)),
        '(' => Some((KEY_9, true)),
        ')' => Some((KEY_0, true)),
        '_' => Some((KEY_MINUS, true)),
        '+' => Some((KEY_EQUAL, true)),
        '{' => Some((KEY_LEFTBRACE, true)),
        '}' => Some((KEY_RIGHTBRACE, true)),
        ':' => Some((KEY_SEMICOLON, true)),
        '"' => Some((KEY_APOSTROPHE, true)),
        '~' => Some((KEY_GRAVE, true)),
        '|' => Some((KEY_BACKSLASH, true)),
        '<' => Some((KEY_COMMA, true)),
        '>' => Some((KEY_DOT, true)),
        '?' => Some((KEY_SLASH, true)),
        _ => None,
    }
}

/// Maps a combo name like "ctrl" to its keycode
fn modifier_to_key(name: &str) -> Option<u16> {
    match name {
        "ctrl" | "control" => Some(KEY_LEFTCTRL),
        "shift" => Some(KEY_LEFTSHIFT),
        "alt" => Some(KEY_LEFTALT),
        "super" | "meta" | "win" => Some(KEY_LEFTMETA),
        "tab" => Some(KEY_TAB),
        "enter" | "return" => Some(KEY_ENTER),
        "space" => Some(KEY_SPACE),
        "backspace" => Some(KEY_BACKSPACE),
        "delete" | "del" => Some(KEY_DELETE),
        "escape" | "esc" => Some(KEY_ESC),
        "up" => Some(KEY_UP),
        "down" => Some(KEY_DOWN),
        "left" => Some(KEY_LEFT),
        "right" => Some(KEY_RIGHT),
        "home" => Some(KEY_HOME),
        "end" => Some(KEY_END),
        "pageup" => Some(KEY_PAGEUP),
        "pagedown" => Some(KEY_PAGEDOWN),
        "f1" => Some(KEY_F1),
        "f2" => Some(KEY_F2),
        "f3" => Some(KEY_F3),
        "f4" => Some(KEY_F4),
        "f5" => Some(KEY_F5),
        "f6" => Some(KEY_F6),
        "f7" => Some(KEY_F7),
        "f8" => Some(KEY_F8),
        "f9" => Some(KEY_F9),
        "f10" => Some(KEY_F10),
        "f11" => Some(KEY_F11),
        "f12" => Some(KEY_F12),
        s if s.len() == 1 => char_to_key(s.chars().next().unwrap()).map(|(k, _)| k),
        _ => None,
    }
}

pub fn mouse_move(x: i32, y: i32) -> Result<String, String> {
    let mut dev = UinputDevice::create("gui-tool-mouse", true)?;
    dev.write_event(EV_ABS, ABS_X, x)?;
    dev.write_event(EV_ABS, ABS_Y, y)?;
    dev.syn()?;
    std::thread::sleep(std::time::Duration::from_millis(50));
    Ok(crate::json::success())
}

pub fn mouse_click(button: &str) -> Result<String, String> {
    let btn = match button {
        "left" => BTN_LEFT,
        "right" => BTN_RIGHT,
        _ => return Err(format!("Unknown button: {}. Use 'left' or 'right'", button)),
    };
    let mut dev = UinputDevice::create("gui-tool-mouse", true)?;
    dev.write_event(EV_KEY, btn, 1)?;
    dev.syn()?;
    std::thread::sleep(std::time::Duration::from_millis(50));
    dev.write_event(EV_KEY, btn, 0)?;
    dev.syn()?;
    std::thread::sleep(std::time::Duration::from_millis(50));
    Ok(crate::json::success())
}

pub fn key_type(text: &str) -> Result<String, String> {
    let mut dev = UinputDevice::create("gui-tool-kbd", false)?;
    for c in text.chars() {
        if let Some((code, shift)) = char_to_key(c) {
            if shift {
                dev.key_down(KEY_LEFTSHIFT)?;
            }
            dev.tap_key(code)?;
            if shift {
                dev.key_up(KEY_LEFTSHIFT)?;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(50));
    Ok(crate::json::success())
}

pub fn key_press(combo: &str) -> Result<String, String> {
    let parts: Vec<&str> = combo.split('+').collect();
    let mut keys: Vec<u16> = Vec::new();
    for part in &parts {
        let key = modifier_to_key(&part.to_lowercase())
            .ok_or_else(|| format!("Unknown key: {}", part))?;
        keys.push(key);
    }

    let mut dev = UinputDevice::create("gui-tool-kbd", false)?;

    // Press all keys down in order
    for &key in &keys {
        dev.key_down(key)?;
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    // Release in reverse order
    for &key in keys.iter().rev() {
        dev.key_up(key)?;
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    std::thread::sleep(std::time::Duration::from_millis(50));
    Ok(crate::json::success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mode_standard() {
        assert_eq!(parse_mode("1920x1080"), Some((1920, 1080)));
    }

    #[test]
    fn test_parse_mode_4k() {
        assert_eq!(parse_mode("3840x2160"), Some((3840, 2160)));
    }

    #[test]
    fn test_parse_mode_invalid() {
        assert_eq!(parse_mode("not_a_mode"), None);
        assert_eq!(parse_mode("1920"), None);
        assert_eq!(parse_mode(""), None);
        assert_eq!(parse_mode("abcxdef"), None);
    }

    #[test]
    fn test_detect_screen_size_returns_positive() {
        let (w, h) = detect_screen_size();
        assert!(w > 0, "width must be positive, got {}", w);
        assert!(h > 0, "height must be positive, got {}", h);
    }
}
