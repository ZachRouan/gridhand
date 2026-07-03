use crate::json::{self, JsonValue};
use super::dbus::DbusConnection;
use super::dbus::types::MarshalBuffer;
use super::windows;

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

pub fn screenshot_window(title: &str, output: &str) -> Result<String, String> {
    let mut conn = DbusConnection::connect()?;

    let (win_id, win_json) = windows::find_window_by_title(&mut conn, title)?
        .ok_or_else(|| format!("No window found matching '{}'", title))?;

    // Activate the window
    let mut body = MarshalBuffer::new();
    body.write_u32(win_id);
    conn.call_method(
        "org.gnome.Shell",
        "/org/gnome/Shell/Extensions/Windows",
        "org.gnome.Shell.Extensions.Windows",
        "Activate",
        Some("u"),
        &body.into_bytes(),
    )?;

    std::thread::sleep(std::time::Duration::from_millis(300));

    // Get window details (x, y, width, height) for cropping
    let details_json = windows::get_window_details(&mut conn, win_id)?;
    let win_x = crate::json::extract_json_number(&details_json, "x")
        .ok_or_else(|| "Window details missing 'x' field".to_string())?;
    let win_y = crate::json::extract_json_number(&details_json, "y")
        .ok_or_else(|| "Window details missing 'y' field".to_string())?;
    let win_w = crate::json::extract_json_number(&details_json, "width")
        .ok_or_else(|| "Window details missing 'width' field".to_string())?;
    let win_h = crate::json::extract_json_number(&details_json, "height")
        .ok_or_else(|| "Window details missing 'height' field".to_string())?;

    // Take full-screen screenshot
    let uri = take_portal_screenshot(&mut conn)?;
    let src_path = uri_to_path(&uri)?;

    // Read, crop to the visible part of the window, and write the PNG
    let full_img = crate::platform::png::read_png(&src_path)?;
    let _ = std::fs::remove_file(&src_path);
    let (cx, cy, cw, ch) = visible_crop(win_x, win_y, win_w, win_h);
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

pub fn screenshot_window_by_id(id: u32, output: &str) -> Result<String, String> {
    let mut conn = DbusConnection::connect()?;

    // Raise the window first
    let mut body = MarshalBuffer::new();
    body.write_u32(id);
    conn.call_method(
        "org.gnome.Shell",
        "/org/gnome/Shell/Extensions/Windows",
        "org.gnome.Shell.Extensions.Windows",
        "Activate",
        Some("u"),
        &body.into_bytes(),
    )?;

    std::thread::sleep(std::time::Duration::from_millis(300));

    // Get window details (x, y, width, height) for cropping
    let details_json = windows::get_window_details(&mut conn, id)?;
    let win_x = crate::json::extract_json_number(&details_json, "x")
        .ok_or_else(|| "Window details missing 'x' field".to_string())?;
    let win_y = crate::json::extract_json_number(&details_json, "y")
        .ok_or_else(|| "Window details missing 'y' field".to_string())?;
    let win_w = crate::json::extract_json_number(&details_json, "width")
        .ok_or_else(|| "Window details missing 'width' field".to_string())?;
    let win_h = crate::json::extract_json_number(&details_json, "height")
        .ok_or_else(|| "Window details missing 'height' field".to_string())?;

    // Take full-screen screenshot via portal
    let uri = take_portal_screenshot(&mut conn)?;
    let src_path = uri_to_path(&uri)?;

    // Read, crop to the visible part of the window, and write the PNG
    let full_img = crate::platform::png::read_png(&src_path)?;
    let _ = std::fs::remove_file(&src_path);
    let (cx, cy, cw, ch) = visible_crop(win_x, win_y, win_w, win_h);
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
    let token = format!("gui_tool_{}_{}", std::process::id(), seq);
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

    body.finish_array(arr_pos);

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
