mod ffi;
mod input;
mod screenshot;
mod windows;

fn parse_window_id(id: &str) -> Result<u64, String> {
    id.parse::<u64>()
        .map_err(|_| format!("Invalid numeric window ID: {}", id))
}

pub fn screenshot_full(output: &str) -> Result<String, String> {
    screenshot::screenshot_full(output)
}

pub fn screenshot_window(title: &str, output: &str) -> Result<String, String> {
    screenshot::screenshot_window(title, output)
}

pub fn screenshot_window_by_id(id: &str, output: &str) -> Result<String, String> {
    let native = parse_window_id(id)?;
    screenshot::screenshot_window_by_id(native, output)
}

pub fn find_window_by_title(title: &str) -> Result<Option<(String, String)>, String> {
    windows::find_window_by_title(title).map(|opt| opt.map(|(id, json)| (id.to_string(), json)))
}

pub fn get_window_bounds(id: &str) -> Result<(i32, i32, u32, u32), String> {
    let native = parse_window_id(id)?;
    let id32 = u32::try_from(native).map_err(|_| format!("Window ID {} out of range", id))?;
    windows::get_window_bounds(id32)
}

pub fn list_windows() -> Result<String, String> {
    windows::list_windows()
}

pub fn raise_window(id: &str) -> Result<String, String> {
    windows::raise_window(parse_window_id(id)?)
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
