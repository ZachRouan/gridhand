#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
compile_error!(
    "gridhand's Linux backend issues raw syscalls via inline assembly and \
     supports only x86_64 and aarch64"
);

mod uinput;
mod dbus;
mod display;
mod screenshot;
mod windows;

/// window-calls ids are u32 on the wire; a larger value would silently wrap
/// and act on a different window.
fn check_window_id(id: u64) -> Result<u32, String> {
    u32::try_from(id)
        .map_err(|_| format!("Window id {} is out of range for the window-calls extension (u32)", id))
}

pub fn screenshot_full(output: &str) -> Result<String, String> {
    screenshot::screenshot_full(output)
}

pub fn screenshot_window(title: &str, output: &str) -> Result<String, String> {
    screenshot::screenshot_window(title, output)
}

pub fn screenshot_window_by_id(id: u64, output: &str) -> Result<String, String> {
    screenshot::screenshot_window_by_id(check_window_id(id)?, output)
}

pub fn find_window_by_title(title: &str) -> Result<Option<(u64, String)>, String> {
    let mut conn = dbus::DbusConnection::connect()?;
    windows::find_window_by_title(&mut conn, title)
        .map(|opt| opt.map(|(id, json)| (id as u64, json)))
}

pub fn get_window_bounds(id: u64) -> Result<(i32, i32, u32, u32), String> {
    let mut conn = dbus::DbusConnection::connect()?;
    let details = windows::get_window_details(&mut conn, check_window_id(id)?)?;
    let x = crate::json::extract_json_number(&details, "x")
        .ok_or("Window details missing 'x'")? as i32;
    let y = crate::json::extract_json_number(&details, "y")
        .ok_or("Window details missing 'y'")? as i32;
    let w = crate::json::extract_json_number(&details, "width")
        .ok_or("Window details missing 'width'")? as u32;
    let h = crate::json::extract_json_number(&details, "height")
        .ok_or("Window details missing 'height'")? as u32;
    Ok((x, y, w, h))
}

pub fn list_windows() -> Result<String, String> {
    windows::list_windows()
}

pub fn raise_window(id: u64) -> Result<String, String> {
    windows::raise_window(check_window_id(id)?)
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
