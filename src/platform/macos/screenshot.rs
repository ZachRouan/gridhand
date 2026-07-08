#![allow(non_upper_case_globals)]

use std::ffi::c_void;
use crate::json::{self, JsonValue};
use super::ffi::*;
use super::windows;

pub fn screenshot_full(output: &str) -> Result<String, String> {
    let image = capture_screen(
        kCGWindowListOptionOnScreenOnly,
        kCGNullWindowID,
        CGRect::null(),
        kCGWindowImageDefault,
    )?;
    let pixels = extract_pixels(image);
    unsafe { CFRelease(image); }
    let img = pixels?;

    crate::platform::png::write_png(output, &img)?;

    Ok(json::success_with(vec![
        ("path", JsonValue::Str(output)),
    ]))
}

pub fn screenshot_window(title: &str, output: &str) -> Result<String, String> {
    let (win_id, win_json) = windows::find_window_by_title(title)?
        .ok_or_else(|| format!("No window found matching '{}'", title))?;

    // Raise before capturing, same as screenshot_window_by_id: without this,
    // a background window can be captured mid-transition or behind whatever
    // is currently frontmost.
    raise_and_settle(win_id as u64)?;

    // Capture just this window — macOS crops natively. Ignore framing effects
    // (the drop shadow) so the image corresponds exactly to kCGWindowBounds;
    // otherwise every grid cell is offset by the shadow margin.
    let image = capture_screen(
        kCGWindowListOptionIncludingWindow,
        win_id,
        CGRect::null(),
        kCGWindowImageBoundsIgnoreFraming,
    )?;
    let pixels = extract_pixels(image);
    unsafe { CFRelease(image); }
    let img = pixels?;

    crate::platform::png::write_png(output, &img)?;

    Ok(json::success_with(vec![
        ("path", JsonValue::Str(output)),
        ("window", JsonValue::RawJson(win_json)),
    ]))
}

pub fn screenshot_window_by_id(id: u64, output: &str) -> Result<String, String> {
    // window-calls / CGWindowID is a u32 on the wire; a larger value would
    // silently wrap and capture a different window (mirrors linux/mod.rs's
    // check_window_id).
    let id32 = u32::try_from(id).map_err(|_| format!("Window ID {} out of range", id))?;

    raise_and_settle(id)?;

    // Capture just this window — macOS crops natively, ignoring the drop
    // shadow so the image corresponds exactly to kCGWindowBounds.
    let image = capture_screen(
        kCGWindowListOptionIncludingWindow,
        id32,
        CGRect::null(),
        kCGWindowImageBoundsIgnoreFraming,
    )?;
    let pixels = extract_pixels(image);
    unsafe { CFRelease(image); }
    let img = pixels?;

    crate::platform::png::write_png(output, &img)?;

    Ok(json::success_with(vec![
        ("path", JsonValue::Str(output)),
    ]))
}

/// Raise the window and give the window manager time to bring it to front
/// before capturing. Without the settle delay, screenshot_window(_by_id) can
/// capture a stale frame from before the raise took effect.
fn raise_and_settle(id: u64) -> Result<(), String> {
    windows::raise_window(id)?;
    std::thread::sleep(std::time::Duration::from_millis(300));
    Ok(())
}

fn capture_screen(list_option: u32, window_id: u32, bounds: CGRect, image_option: u32) -> Result<*mut c_void, String> {
    unsafe {
        let image = CGWindowListCreateImage(
            bounds,
            list_option,
            window_id,
            image_option,
        );
        if image.is_null() {
            return Err(
                "Failed to capture screen image — check Screen Recording permission (System \
                 Settings > Privacy & Security). On macOS 15+ the CGWindowList capture API may \
                 be unavailable."
                    .to_string(),
            );
        }
        Ok(image)
    }
}

/// In-memory byte layout of a captured CGImage's pixel data, derived from
/// its CGBitmapInfo. CGWindowListCreateImage's actual layout varies by
/// macOS version and hardware — it is not reliably BGRA.
#[derive(Debug, PartialEq, Eq)]
enum PixelLayout {
    /// R,G,B,A in memory — no reordering needed.
    Rgba,
    /// B,G,R,A in memory: alpha-first with byte-swapped 32-bit-little order.
    Bgra,
    /// A,R,G,B in memory: alpha-first with default (big-endian) byte order.
    Argb,
}

/// Classify a CGBitmapInfo into a pixel layout, or error for combinations
/// this decoder doesn't recognize (e.g. non-alpha-related bitmap flags this
/// backend was never taught).
fn pixel_layout(bitmap_info: u32) -> Result<PixelLayout, String> {
    let alpha_info = bitmap_info & kCGBitmapAlphaInfoMask;
    let byte_order = bitmap_info & kCGBitmapByteOrderMask;

    match alpha_info {
        kCGImageAlphaPremultipliedLast | kCGImageAlphaNoneSkipLast => Ok(PixelLayout::Rgba),
        kCGImageAlphaPremultipliedFirst | kCGImageAlphaNoneSkipFirst => {
            if byte_order == kCGBitmapByteOrder32Little {
                Ok(PixelLayout::Bgra)
            } else {
                // Default (0) or explicit 32-big: bytes sit in nominal
                // struct order — alpha first, i.e. ARGB in memory.
                Ok(PixelLayout::Argb)
            }
        }
        other => Err(format!("Unsupported CGImage alpha info: {}", other)),
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

        let layout = pixel_layout(bitmap_info)?;

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

        // Copy pixels row-by-row, handling row padding and channel swapping
        let mut pixels = Vec::with_capacity(width * height * 4);
        let src = std::slice::from_raw_parts(ptr, len);

        for row in 0..height {
            let row_start = row * bytes_per_row;
            for col in 0..width {
                let offset = row_start + col * 4;
                if offset + 4 > len { break; }
                match layout {
                    PixelLayout::Bgra => {
                        pixels.push(src[offset + 2]); // R
                        pixels.push(src[offset + 1]); // G
                        pixels.push(src[offset]);     // B
                        pixels.push(src[offset + 3]); // A
                    }
                    PixelLayout::Argb => {
                        pixels.push(src[offset + 1]); // R
                        pixels.push(src[offset + 2]); // G
                        pixels.push(src[offset + 3]); // B
                        pixels.push(src[offset]);     // A
                    }
                    PixelLayout::Rgba => {
                        pixels.push(src[offset]);
                        pixels.push(src[offset + 1]);
                        pixels.push(src[offset + 2]);
                        pixels.push(src[offset + 3]);
                    }
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

// This whole module is `#[cfg(target_os = "macos")]`-gated at
// `platform/mod.rs`, so these tests only compile and run in CI's macos job
// (or on macOS hardware) — never under a Linux-host `cargo test`. They cover
// pure classification logic only; nothing here touches the CoreGraphics FFI.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn premultiplied_last_is_rgba_regardless_of_byte_order() {
        assert_eq!(pixel_layout(kCGImageAlphaPremultipliedLast).unwrap(), PixelLayout::Rgba);
        assert_eq!(
            pixel_layout(kCGImageAlphaPremultipliedLast | kCGBitmapByteOrder32Little).unwrap(),
            PixelLayout::Rgba
        );
    }

    #[test]
    fn none_skip_last_is_rgba() {
        assert_eq!(pixel_layout(kCGImageAlphaNoneSkipLast).unwrap(), PixelLayout::Rgba);
    }

    #[test]
    fn premultiplied_first_little_endian_is_bgra() {
        assert_eq!(
            pixel_layout(kCGImageAlphaPremultipliedFirst | kCGBitmapByteOrder32Little).unwrap(),
            PixelLayout::Bgra
        );
    }

    #[test]
    fn none_skip_first_little_endian_is_bgra() {
        assert_eq!(
            pixel_layout(kCGImageAlphaNoneSkipFirst | kCGBitmapByteOrder32Little).unwrap(),
            PixelLayout::Bgra
        );
    }

    #[test]
    fn premultiplied_first_default_byte_order_is_argb() {
        assert_eq!(
            pixel_layout(kCGImageAlphaPremultipliedFirst | kCGBitmapByteOrderDefault).unwrap(),
            PixelLayout::Argb
        );
    }

    #[test]
    fn none_skip_first_default_byte_order_is_argb() {
        assert_eq!(
            pixel_layout(kCGImageAlphaNoneSkipFirst).unwrap(),
            PixelLayout::Argb
        );
    }

    #[test]
    fn unsupported_alpha_info_errors() {
        // 0 = kCGImageAlphaNone: no alpha channel at all, not a 4-byte
        // layout this decoder can classify.
        assert!(pixel_layout(0).is_err());
    }
}
