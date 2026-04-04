mod uinput;
mod dbus;
mod screenshot;
mod windows;

pub fn screenshot_full(output: &str) -> Result<String, String> {
    screenshot::screenshot_full(output)
}

pub fn screenshot_window(title: &str, output: &str) -> Result<String, String> {
    screenshot::screenshot_window(title, output)
}

pub fn list_windows() -> Result<String, String> {
    windows::list_windows()
}

pub fn raise_window(id: u32) -> Result<String, String> {
    windows::raise_window(id)
}

pub fn mouse_move(x: i32, y: i32) -> Result<String, String> {
    uinput::mouse_move(x, y)
}

pub fn mouse_click(button: &str) -> Result<String, String> {
    uinput::mouse_click(button)
}

pub fn key_type(text: &str) -> Result<String, String> {
    uinput::key_type(text)
}

pub fn key_press(combo: &str) -> Result<String, String> {
    uinput::key_press(combo)
}
