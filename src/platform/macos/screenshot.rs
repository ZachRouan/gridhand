use std::ffi::c_void;
use crate::json::{self, JsonValue};
use super::ffi::*;
use super::windows;

pub fn screenshot_full(output: &str) -> Result<String, String> {
    let image = capture_screen(kCGWindowListOptionOnScreenOnly, kCGNullWindowID, CGRect::null())?;
    let img = extract_pixels(image)?;
    unsafe { CFRelease(image); }

    crate::platform::png::write_png(output, &img)?;

    Ok(json::success_with(vec![
        ("path", JsonValue::Str(output)),
    ]))
}

pub fn screenshot_window(title: &str, output: &str) -> Result<String, String> {
    let (win_id, win_json) = windows::find_window_by_title(title)?
        .ok_or_else(|| format!("No window found matching '{}'", title))?;

    // Capture just this window — macOS crops natively
    let image = capture_screen(
        kCGWindowListOptionIncludingWindow,
        win_id,
        CGRect::null(),
    )?;
    let img = extract_pixels(image)?;
    unsafe { CFRelease(image); }

    crate::platform::png::write_png(output, &img)?;

    Ok(json::success_with(vec![
        ("path", JsonValue::Str(output)),
        ("window", JsonValue::RawJson(win_json)),
    ]))
}

pub fn screenshot_window_by_id(id: u64, output: &str) -> Result<String, String> {
    // Raise the window first
    windows::raise_window(id)?;
    std::thread::sleep(std::time::Duration::from_millis(300));

    // Capture just this window — macOS crops natively
    let image = capture_screen(
        kCGWindowListOptionIncludingWindow,
        id as u32,
        CGRect::null(),
    )?;
    let img = extract_pixels(image)?;
    unsafe { CFRelease(image); }

    crate::platform::png::write_png(output, &img)?;

    Ok(json::success_with(vec![
        ("path", JsonValue::Str(output)),
    ]))
}

fn capture_screen(list_option: u32, window_id: u32, bounds: CGRect) -> Result<*mut c_void, String> {
    unsafe {
        let image = CGWindowListCreateImage(
            bounds,
            list_option,
            window_id,
            kCGWindowImageDefault,
        );
        if image.is_null() {
            return Err("Failed to capture screen image".to_string());
        }
        Ok(image)
    }
}

fn extract_pixels(image: *mut c_void) -> Result<crate::platform::png::Image, String> {
    unsafe {
        let width = CGImageGetWidth(image);
        let height = CGImageGetHeight(image);
        let bytes_per_row = CGImageGetBytesPerRow(image);
        let bits_per_pixel = CGImageGetBitsPerPixel(image);
        let bitmap_info = CGImageGetBitmapInfo(image);
        let bpp = (bits_per_pixel / 8) as u32;

        if bpp != 4 {
            return Err(format!("Unsupported bits per pixel: {}", bits_per_pixel));
        }

        let provider = CGImageGetDataProvider(image);
        if provider.is_null() {
            return Err("Failed to get image data provider".to_string());
        }

        let data = CGDataProviderCopyData(provider);
        if data.is_null() {
            return Err("Failed to copy image data".to_string());
        }

        let ptr = CFDataGetBytePtr(data);
        let len = CFDataGetLength(data) as usize;

        // Determine if we need to swap BGRA -> RGBA
        let alpha_info = bitmap_info & 0x1F;
        let byte_order = bitmap_info & (7 << 12);
        let is_bgra = byte_order == kCGBitmapByteOrder32Little
            || alpha_info == kCGImageAlphaPremultipliedFirst
            || alpha_info == kCGImageAlphaNoneSkipFirst;

        // Copy pixels row-by-row, handling row padding and channel swapping
        let mut pixels = Vec::with_capacity(width * height * 4);
        let src = std::slice::from_raw_parts(ptr, len);

        for row in 0..height {
            let row_start = row * bytes_per_row;
            for col in 0..width {
                let offset = row_start + col * 4;
                if offset + 4 > len { break; }
                if is_bgra {
                    // BGRA -> RGBA
                    pixels.push(src[offset + 2]); // R
                    pixels.push(src[offset + 1]); // G
                    pixels.push(src[offset]);     // B
                    pixels.push(src[offset + 3]); // A
                } else {
                    // Already RGBA
                    pixels.push(src[offset]);
                    pixels.push(src[offset + 1]);
                    pixels.push(src[offset + 2]);
                    pixels.push(src[offset + 3]);
                }
            }
        }

        CFRelease(data);

        Ok(crate::platform::png::Image {
            width: width as u32,
            height: height as u32,
            bpp: 4,
            pixels,
        })
    }
}
