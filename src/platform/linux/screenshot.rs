use crate::json::{self, JsonValue};
use super::dbus::DbusConnection;
use super::dbus::types::MarshalBuffer;
use super::desktop;

const PORTAL_DEST: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const PORTAL_IFACE: &str = "org.freedesktop.portal.Screenshot";

pub fn screenshot_full(output: &str) -> Result<String, String> {
    let mut conn = DbusConnection::connect()?;
    let uri = take_portal_screenshot(&mut conn)?;

    let src_path = uri_to_path(&uri)?;
    std::fs::copy(&src_path, output)
        .map_err(|e| format!("Failed to copy screenshot to {}: {}", output, e))?;
    // The portal saved its own copy (typically in ~/Pictures/Screenshots);
    // remove it so repeated captures don't litter the user's files.
    let _ = std::fs::remove_file(&src_path);

    Ok(json::success_with(vec![
        ("path", JsonValue::Str(output)),
    ]))
}

/// Crop rectangle for a window that may hang off the screen edges: clamp the
/// origin to 0 and shrink the size by the off-screen amount, so the crop
/// contains only the window (not content shifted in from neighbors).
fn visible_crop(x: i64, y: i64, w: i64, h: i64) -> (u32, u32, u32, u32) {
    let crop_w = (w + x.min(0)).max(0) as u32;
    let crop_h = (h + y.min(0)).max(0) as u32;
    (x.max(0) as u32, y.max(0) as u32, crop_w, crop_h)
}

/// Map a rectangle in KWin global logical coordinates into the pixel space of
/// the KDE portal's full-workspace PNG.
///
/// xdg-desktop-portal-kde captures KWin's virtual screen geometry at native
/// resolution. KWin renders that entire logical rectangle at one scale, so the
/// actual PNG dimensions are authoritative: subtract the virtual origin and
/// scale both edges into the image. Computing edges independently preserves
/// rounding and handles windows partially outside the captured workspace.
fn logical_crop_to_pixels(
    x: i64,
    y: i64,
    w: i64,
    h: i64,
    virtual_geometry: (i64, i64, i64, i64),
    image_size: (u32, u32),
) -> Result<(u32, u32, u32, u32), String> {
    let (vx, vy, vw, vh) = virtual_geometry;
    let (image_w, image_h) = image_size;

    if w <= 0 || h <= 0 {
        return Err(format!("Window has invalid geometry {}x{}", w, h));
    }
    if vw <= 0 || vh <= 0 || image_w == 0 || image_h == 0 {
        return Err("Cannot map screenshot coordinates with empty geometry".to_string());
    }

    let map_x = |value: i64| (((value - vx) as f64) * image_w as f64 / vw as f64).round() as i64;
    let map_y = |value: i64| (((value - vy) as f64) * image_h as f64 / vh as f64).round() as i64;

    let left = map_x(x).clamp(0, image_w as i64);
    let top = map_y(y).clamp(0, image_h as i64);
    let right = map_x(x + w).clamp(0, image_w as i64);
    let bottom = map_y(y + h).clamp(0, image_h as i64);

    if right <= left || bottom <= top {
        return Err("Window does not intersect the captured workspace".to_string());
    }

    Ok((
        left as u32,
        top as u32,
        (right - left) as u32,
        (bottom - top) as u32,
    ))
}

fn window_crop(
    logical_geometry: Option<(i64, i64, i64, i64)>,
    image_size: (u32, u32),
    x: i64,
    y: i64,
    w: i64,
    h: i64,
) -> Result<(u32, u32, u32, u32), String> {
    match logical_geometry {
        Some(geometry) => logical_crop_to_pixels(x, y, w, h, geometry, image_size),
        None => Ok(visible_crop(x, y, w, h)),
    }
}

pub fn screenshot_window(title: &str, output: &str) -> Result<String, String> {
    let mut conn = DbusConnection::connect()?;

    let (win_id, win_json) = desktop::find_window_by_title_with_conn(&mut conn, title)?
        .ok_or_else(|| format!("No window found matching '{}'", title))?;

    desktop::raise_window_with_conn(&mut conn, &win_id)?;

    std::thread::sleep(std::time::Duration::from_millis(300));

    // Get window details (x, y, width, height) for cropping
    let details_json = desktop::window_details_with_conn(&mut conn, &win_id)?;
    let win_x = crate::json::extract_json_number(&details_json, "x")
        .ok_or_else(|| "Window details missing 'x' field".to_string())?;
    let win_y = crate::json::extract_json_number(&details_json, "y")
        .ok_or_else(|| "Window details missing 'y' field".to_string())?;
    let win_w = crate::json::extract_json_number(&details_json, "width")
        .ok_or_else(|| "Window details missing 'width' field".to_string())?;
    let win_h = crate::json::extract_json_number(&details_json, "height")
        .ok_or_else(|| "Window details missing 'height' field".to_string())?;

    // Observe the KDE logical workspace before and after capture. If the
    // topology changes while the portal is producing pixels, no coordinate
    // transform can be trusted.
    let geometry_before = desktop::screenshot_logical_geometry_with_conn(&mut conn)?;

    // Take full-screen screenshot
    let uri = take_portal_screenshot(&mut conn)?;
    let src_path = uri_to_path(&uri)?;

    // Read, crop to the visible part of the window, and write the PNG.
    let full_img = crate::platform::png::read_png(&src_path)?;
    let _ = std::fs::remove_file(&src_path);
    let geometry_after = desktop::screenshot_logical_geometry_with_conn(&mut conn)?;
    if geometry_before != geometry_after {
        return Err("Desktop layout changed during screenshot; retry the capture".to_string());
    }
    let (cx, cy, cw, ch) = window_crop(
        geometry_after,
        (full_img.width, full_img.height),
        win_x,
        win_y,
        win_w,
        win_h,
    )?;
    let cropped = crate::platform::png::crop(&full_img, cx, cy, cw, ch)?;
    crate::platform::png::write_png(output, &cropped)?;

    Ok(json::success_with(vec![
        ("path", JsonValue::Str(output)),
        ("window", JsonValue::RawJson(win_json)),
        ("bounds", JsonValue::Object(vec![
            ("x", JsonValue::Int(win_x)),
            ("y", JsonValue::Int(win_y)),
            ("width", JsonValue::Int(win_w)),
            ("height", JsonValue::Int(win_h)),
        ])),
    ]))
}

pub fn screenshot_window_by_id(id: &str, output: &str) -> Result<String, String> {
    let mut conn = DbusConnection::connect()?;

    // Raise the window first
    desktop::raise_window_with_conn(&mut conn, id)?;

    std::thread::sleep(std::time::Duration::from_millis(300));

    // Get window details (x, y, width, height) for cropping
    let details_json = desktop::window_details_with_conn(&mut conn, id)?;
    let win_x = crate::json::extract_json_number(&details_json, "x")
        .ok_or_else(|| "Window details missing 'x' field".to_string())?;
    let win_y = crate::json::extract_json_number(&details_json, "y")
        .ok_or_else(|| "Window details missing 'y' field".to_string())?;
    let win_w = crate::json::extract_json_number(&details_json, "width")
        .ok_or_else(|| "Window details missing 'width' field".to_string())?;
    let win_h = crate::json::extract_json_number(&details_json, "height")
        .ok_or_else(|| "Window details missing 'height' field".to_string())?;

    let geometry_before = desktop::screenshot_logical_geometry_with_conn(&mut conn)?;

    // Take full-screen screenshot via portal
    let uri = take_portal_screenshot(&mut conn)?;
    let src_path = uri_to_path(&uri)?;

    // Read, crop to the visible part of the window, and write the PNG.
    let full_img = crate::platform::png::read_png(&src_path)?;
    let _ = std::fs::remove_file(&src_path);
    let geometry_after = desktop::screenshot_logical_geometry_with_conn(&mut conn)?;
    if geometry_before != geometry_after {
        return Err("Desktop layout changed during screenshot; retry the capture".to_string());
    }
    let (cx, cy, cw, ch) = window_crop(
        geometry_after,
        (full_img.width, full_img.height),
        win_x,
        win_y,
        win_w,
        win_h,
    )?;
    let cropped = crate::platform::png::crop(&full_img, cx, cy, cw, ch)?;
    crate::platform::png::write_png(output, &cropped)?;

    Ok(json::success_with(vec![
        ("path", JsonValue::Str(output)),
        ("bounds", JsonValue::Object(vec![
            ("x", JsonValue::Int(win_x)),
            ("y", JsonValue::Int(win_y)),
            ("width", JsonValue::Int(win_w)),
            ("height", JsonValue::Int(win_h)),
        ])),
    ]))
}

fn take_portal_screenshot(conn: &mut DbusConnection) -> Result<String, String> {
    let sender_escaped = conn.unique_name()
        .trim_start_matches(':')
        .replace('.', "_");
    // Token must be unique per request, not just per process — concurrent
    // requests (e.g. parallel tests) would otherwise collide on the handle.
    static REQUEST_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let seq = REQUEST_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let token = format!("gridhand_{}_{}", std::process::id(), seq);
    let handle_path = format!(
        "/org/freedesktop/portal/desktop/request/{}/{}",
        sender_escaped, token
    );

    let match_rule = format!(
        "type='signal',interface='org.freedesktop.portal.Request',member='Response',path='{}'",
        handle_path
    );
    conn.add_match(&match_rule)?;

    let mut body = MarshalBuffer::new();
    body.write_string("");

    let arr_pos = body.start_array(8);

    body.align_struct();
    body.write_string("handle_token");
    body.write_variant_string(&token);

    body.align_struct();
    body.write_string("interactive");
    body.write_variant_bool(false);

    body.finish_array(arr_pos, 8);

    let body_bytes = body.into_bytes();

    let reply = conn.call_method(
        PORTAL_DEST,
        PORTAL_PATH,
        PORTAL_IFACE,
        "Screenshot",
        Some("sa{sv}"),
        &body_bytes,
    )?;

    // The method reply carries the actual request object path. Portals
    // predating xdg-desktop-portal 0.9 use a different handle than the
    // predicted one — listen on the path the portal actually returned.
    let mut rbuf = super::dbus::types::UnmarshalBuffer::new(&reply.body);
    let actual_handle = rbuf.read_object_path().unwrap_or_else(|_| handle_path.clone());
    if actual_handle != handle_path {
        let rule = format!(
            "type='signal',interface='org.freedesktop.portal.Request',member='Response',path='{}'",
            actual_handle
        );
        conn.add_match(&rule)?;
    }

    let signal = conn.wait_for_signal(
        &actual_handle,
        "org.freedesktop.portal.Request",
        "Response",
        10_000,
    )?;

    let mut ubuf = super::dbus::types::UnmarshalBuffer::new(&signal.body);
    let response_code = ubuf.read_u32()?;
    if response_code != 0 {
        return Err(format!("Screenshot was cancelled or failed (code {})", response_code));
    }

    let arr_len = ubuf.read_u32()? as usize;
    let arr_end = ubuf.pos + arr_len;

    while ubuf.pos < arr_end {
        ubuf.align(8);
        let key = ubuf.read_string()?;
        let val = ubuf.read_variant_string()?;
        if key == "uri"
            && let Some(uri) = val {
                return Ok(uri);
            }
    }

    Err("Screenshot response missing 'uri' field".to_string())
}

fn uri_to_path(uri: &str) -> Result<String, String> {
    if let Some(path) = uri.strip_prefix("file://") {
        url_decode(path)
    } else {
        Err(format!("Unexpected URI format: {}", uri))
    }
}

/// Percent-decode a URI path. Escapes are UTF-8 *bytes*, so they must be
/// collected into a byte buffer and validated as UTF-8 — decoding each byte
/// as a char mangles multibyte characters ("%C3%A9" would become "Ã©" and
/// GNOME's localized screenshot paths would stop resolving). Malformed
/// escapes pass through literally rather than swallowing characters.
fn url_decode(s: &str) -> Result<String, String> {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (
                (bytes[i + 1] as char).to_digit(16),
                (bytes[i + 2] as char).to_digit(16),
            ) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).map_err(|_| format!("URI is not valid UTF-8 after decoding: {}", s))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logical_crop_to_pixels_fractional_scale() {
        assert_eq!(
            logical_crop_to_pixels(2583, 0, 1257, 1440, (0, 0, 3840, 1440), (5760, 2160),).unwrap(),
            (3875, 0, 1885, 2160)
        );
    }

    #[test]
    fn test_logical_crop_to_pixels_negative_virtual_origin() {
        assert_eq!(
            logical_crop_to_pixels(-1280, 0, 1280, 720, (-1280, 0, 3840, 1440), (5760, 2160),)
                .unwrap(),
            (0, 0, 1920, 1080)
        );
    }

    #[test]
    fn test_logical_crop_to_pixels_clips_offscreen_window() {
        assert_eq!(
            logical_crop_to_pixels(-100, 0, 200, 100, (0, 0, 3840, 1440), (5760, 2160),).unwrap(),
            (0, 0, 150, 150)
        );
    }

    #[test]
    fn test_logical_crop_to_pixels_rejects_outside_window() {
        assert!(
            logical_crop_to_pixels(5000, 0, 200, 100, (0, 0, 3840, 1440), (5760, 2160),).is_err()
        );
    }

    #[test]
    fn test_url_decode_utf8() {
        // GNOME saves screenshots under localized directory names; percent
        // escapes are UTF-8 bytes and must decode as such, not as Latin-1
        // chars ("%C3%A9" is 'é', not "Ã©").
        assert_eq!(
            url_decode("/home/z/Images/Captures%20d%27%C3%A9cran/s.png").unwrap(),
            "/home/z/Images/Captures d'écran/s.png"
        );
    }

    #[test]
    fn test_url_decode_plain_ascii() {
        assert_eq!(url_decode("/tmp/shot.png").unwrap(), "/tmp/shot.png");
        assert_eq!(url_decode("/tmp/a%20b.png").unwrap(), "/tmp/a b.png");
    }

    #[test]
    fn test_url_decode_invalid_sequences_pass_through() {
        // A malformed escape must not swallow the following characters
        assert_eq!(url_decode("/a%2").unwrap(), "/a%2");
        assert_eq!(url_decode("/a%zzb").unwrap(), "/a%zzb");
    }

    #[test]
    fn test_uri_to_path() {
        assert_eq!(uri_to_path("file:///tmp/s.png").unwrap(), "/tmp/s.png");
        assert!(uri_to_path("http://example.com/s.png").is_err());
    }
}
