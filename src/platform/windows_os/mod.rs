mod ffi;
mod input;
mod screenshot;
mod windows;

/// Opt in to DPI awareness once per process. Without it, on scaled displays
/// (the 125/150% laptop default) GDI captures are DWM-virtualized and
/// blurry, and window metrics come back in scaled units.
fn ensure_dpi_aware() {
    static DPI_AWARE: std::sync::Once = std::sync::Once::new();
    DPI_AWARE.call_once(init_dpi_awareness);
}

/// Opt into per-monitor-v2 DPI awareness when available (Win10 1703+),
/// falling back to system-DPI awareness. System-DPI alone virtualizes
/// coordinates on mixed-DPI multi-monitor setups: GetWindowRect and
/// GetSystemMetrics then disagree with SendInput's physical-desktop
/// normalization, displacing every click on the non-primary-DPI monitor.
/// `SetProcessDpiAwarenessContext` postdates the linked baseline (Vista), so
/// it is resolved dynamically via GetProcAddress rather than linked
/// directly — linking it would fail to load on pre-1703 Windows 10 and on
/// Windows 7/8.
fn init_dpi_awareness() {
    unsafe {
        let user32 = ffi::GetModuleHandleA(c"user32.dll".as_ptr().cast());
        if !user32.is_null() {
            let f = ffi::GetProcAddress(user32, c"SetProcessDpiAwarenessContext".as_ptr().cast());
            if !f.is_null() {
                type SetCtxFn = unsafe extern "system" fn(isize) -> ffi::BOOL;
                let set_ctx: SetCtxFn = std::mem::transmute(f);
                const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: isize = -4;
                if set_ctx(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) != 0 {
                    return;
                }
            }
        }
        ffi::SetProcessDPIAware();
    }
}

pub fn screenshot_full(output: &str) -> Result<String, String> {
    ensure_dpi_aware();
    screenshot::screenshot_full(output)
}

pub fn screenshot_window(title: &str, output: &str) -> Result<String, String> {
    ensure_dpi_aware();
    screenshot::screenshot_window(title, output)
}

pub fn screenshot_window_by_id(id: u64, output: &str) -> Result<String, String> {
    ensure_dpi_aware();
    screenshot::screenshot_window_by_id(id, output)
}

pub fn find_window_by_title(title: &str) -> Result<Option<(u64, String)>, String> {
    ensure_dpi_aware();
    windows::find_window_by_title(title)
}

pub fn get_window_bounds(id: u64) -> Result<(i32, i32, u32, u32), String> {
    ensure_dpi_aware();
    let rect = windows::get_window_rect(id)?;
    Ok((
        rect.left,
        rect.top,
        (rect.right - rect.left) as u32,
        (rect.bottom - rect.top) as u32,
    ))
}

pub fn list_windows() -> Result<String, String> {
    ensure_dpi_aware();
    windows::list_windows()
}

pub fn raise_window(id: u64) -> Result<String, String> {
    ensure_dpi_aware();
    windows::raise_window(id)
}

pub fn mouse_move(x: i32, y: i32) -> Result<String, String> {
    ensure_dpi_aware();
    input::mouse_move(x, y)
}

pub fn mouse_click(button: &str) -> Result<String, String> {
    ensure_dpi_aware();
    input::mouse_click(button)
}

pub fn key_type(text: &str) -> Result<String, String> {
    ensure_dpi_aware();
    input::key_type(text)
}

pub fn key_press(combo: &str) -> Result<String, String> {
    ensure_dpi_aware();
    input::key_press(combo)
}
