use crate::json::{self, JsonValue};
use super::ffi::*;
use super::windows;

pub fn screenshot_full(output: &str) -> Result<String, String> {
    let width = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let height = unsafe { GetSystemMetrics(SM_CYSCREEN) };

    if width <= 0 || height <= 0 {
        return Err("Failed to get screen dimensions".to_string());
    }

    let img = capture_region(0, 0, width, height)?;
    crate::platform::png::write_png(output, &img)?;

    Ok(json::success_with(vec![
        ("path", JsonValue::Str(output)),
    ]))
}

pub fn screenshot_window(title: &str, output: &str) -> Result<String, String> {
    let (win_id, win_title) = windows::find_window_by_title(title)?
        .ok_or_else(|| format!("No window found matching '{}'", title))?;

    // Raise the window
    windows::raise_window(win_id)?;
    std::thread::sleep(std::time::Duration::from_millis(300));

    // Get window bounds
    let rect = windows::get_window_rect(win_id)?;
    let x = rect.left;
    let y = rect.top;
    let w = rect.right - rect.left;
    let h = rect.bottom - rect.top;

    if w <= 0 || h <= 0 {
        return Err(format!("Window '{}' has invalid dimensions", win_title));
    }

    let img = capture_region(x, y, w, h)?;
    crate::platform::png::write_png(output, &img)?;

    let bounds_json = format!(
        "{{\"x\":{},\"y\":{},\"width\":{},\"height\":{}}}",
        x, y, w, h
    );

    Ok(json::success_with(vec![
        ("path", JsonValue::Str(output)),
        ("window", JsonValue::OwnedStr(win_title)),
        ("bounds", JsonValue::RawJson(bounds_json)),
    ]))
}

pub fn screenshot_window_by_id(id: u64, output: &str) -> Result<String, String> {
    // Raise the window first
    windows::raise_window(id)?;
    std::thread::sleep(std::time::Duration::from_millis(300));

    // Get window bounds
    let rect = windows::get_window_rect(id)?;
    let x = rect.left;
    let y = rect.top;
    let w = rect.right - rect.left;
    let h = rect.bottom - rect.top;

    if w <= 0 || h <= 0 {
        return Err(format!("Window {} has invalid dimensions", id));
    }

    let img = capture_region(x, y, w, h)?;
    crate::platform::png::write_png(output, &img)?;

    let bounds_json = format!(
        "{{\"x\":{},\"y\":{},\"width\":{},\"height\":{}}}",
        x, y, w, h
    );

    Ok(json::success_with(vec![
        ("path", JsonValue::Str(output)),
        ("bounds", JsonValue::RawJson(bounds_json)),
    ]))
}

fn capture_region(x: i32, y: i32, width: i32, height: i32) -> Result<crate::platform::png::Image, String> {
    unsafe {
        // Get screen DC
        let screen_dc = GetDC(std::ptr::null_mut());
        if screen_dc.is_null() {
            return Err("Failed to get screen DC".to_string());
        }

        // Create memory DC and bitmap
        let mem_dc = CreateCompatibleDC(screen_dc);
        if mem_dc.is_null() {
            ReleaseDC(std::ptr::null_mut(), screen_dc);
            return Err("Failed to create compatible DC".to_string());
        }

        let bitmap = CreateCompatibleBitmap(screen_dc, width, height);
        if bitmap.is_null() {
            DeleteDC(mem_dc);
            ReleaseDC(std::ptr::null_mut(), screen_dc);
            return Err("Failed to create compatible bitmap".to_string());
        }

        let old_bitmap = SelectObject(mem_dc, bitmap);

        // BitBlt from screen to memory
        let ok = BitBlt(mem_dc, 0, 0, width, height, screen_dc, x, y, SRCCOPY);
        if ok == 0 {
            SelectObject(mem_dc, old_bitmap);
            DeleteObject(bitmap);
            DeleteDC(mem_dc);
            ReleaseDC(std::ptr::null_mut(), screen_dc);
            return Err("BitBlt failed".to_string());
        }

        // Extract pixels via GetDIBits (24-bit BGR, bottom-up)
        let row_stride = ((width as usize * 3) + 3) & !3; // 4-byte aligned
        let buf_size = row_stride * height as usize;
        let mut buf = vec![0u8; buf_size];

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: height, // positive = bottom-up
                biPlanes: 1,
                biBitCount: 24,
                biCompression: BI_RGB,
                biSizeImage: 0,
                biXPelsPerMeter: 0,
                biYPelsPerMeter: 0,
                biClrUsed: 0,
                biClrImportant: 0,
            },
            bmiColors: [RGBQUAD { rgbBlue: 0, rgbGreen: 0, rgbRed: 0, rgbReserved: 0 }],
        };

        let lines = GetDIBits(
            mem_dc,
            bitmap,
            0,
            height as u32,
            buf.as_mut_ptr(),
            &mut bmi,
            DIB_RGB_COLORS,
        );

        // Cleanup GDI resources
        SelectObject(mem_dc, old_bitmap);
        DeleteObject(bitmap);
        DeleteDC(mem_dc);
        ReleaseDC(std::ptr::null_mut(), screen_dc);

        if lines == 0 {
            return Err("GetDIBits failed".to_string());
        }

        // Convert bottom-up BGR to top-down RGB for png.rs
        let w = width as usize;
        let h = height as usize;
        let mut pixels = Vec::with_capacity(w * h * 3);

        for row in (0..h).rev() {
            let row_start = row * row_stride;
            for col in 0..w {
                let offset = row_start + col * 3;
                pixels.push(buf[offset + 2]); // R (was B)
                pixels.push(buf[offset + 1]); // G
                pixels.push(buf[offset]);     // B (was R)
            }
        }

        Ok(crate::platform::png::Image {
            width: width as u32,
            height: height as u32,
            bpp: 3,
            pixels,
        })
    }
}
