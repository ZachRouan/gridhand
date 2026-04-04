mod ffi;
mod input;
mod screenshot;
mod windows;

pub fn screenshot_full(output: &str) -> Result<String, String> {
    screenshot::screenshot_full(output)
}

pub fn screenshot_window(title: &str, output: &str) -> Result<String, String> {
    screenshot::screenshot_window(title, output)
}

pub fn screenshot_window_by_id(id: u64, output: &str) -> Result<String, String> {
    screenshot::screenshot_window_by_id(id, output)
}

pub fn find_window_by_title(title: &str) -> Result<Option<(u64, String)>, String> {
    windows::find_window_by_title(title)
}

pub fn get_window_position(id: u64) -> Result<(i32, i32), String> {
    let rect = windows::get_window_rect(id)?;
    Ok((rect.left, rect.top))
}

pub fn list_windows() -> Result<String, String> {
    windows::list_windows()
}

pub fn raise_window(id: u64) -> Result<String, String> {
    windows::raise_window(id)
}

pub fn mouse_move(x: i32, y: i32) -> Result<String, String> {
    input::mouse_move(x, y)
}

pub fn mouse_click(button: &str) -> Result<String, String> {
    input::mouse_click(button)
}

pub fn key_type(text: &str) -> Result<String, String> {
    input::key_type(text)
}

pub fn key_press(combo: &str) -> Result<String, String> {
    input::key_press(combo)
}
