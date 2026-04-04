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

    Ok(json::success_with(vec![
        ("path", JsonValue::Str(output)),
    ]))
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

    // Read, crop, and write the PNG
    let full_img = crate::platform::png::read_png(&src_path)?;
    let cropped = crate::platform::png::crop(&full_img, win_x, win_y, win_w, win_h)?;
    crate::platform::png::write_png(output, &cropped)?;

    Ok(json::success_with(vec![
        ("path", JsonValue::Str(output)),
        ("window", JsonValue::RawJson(win_json)),
        ("bounds", JsonValue::Object(vec![
            ("x", JsonValue::Int(win_x as i64)),
            ("y", JsonValue::Int(win_y as i64)),
            ("width", JsonValue::Int(win_w as i64)),
            ("height", JsonValue::Int(win_h as i64)),
        ])),
    ]))
}

fn take_portal_screenshot(conn: &mut DbusConnection) -> Result<String, String> {
    let sender_escaped = conn.unique_name()
        .trim_start_matches(':')
        .replace('.', "_");
    let token = format!("gui_tool_{}", std::process::id());
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

    conn.call_method(
        PORTAL_DEST,
        PORTAL_PATH,
        PORTAL_IFACE,
        "Screenshot",
        Some("sa{sv}"),
        &body_bytes,
    )?;

    let signal = conn.wait_for_signal(
        &handle_path,
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
        if key == "uri" {
            if let Some(uri) = val {
                return Ok(uri);
            }
        }
    }

    Err("Screenshot response missing 'uri' field".to_string())
}

fn uri_to_path(uri: &str) -> Result<String, String> {
    if let Some(path) = uri.strip_prefix("file://") {
        Ok(url_decode(path))
    } else {
        Err(format!("Unexpected URI format: {}", uri))
    }
}

fn url_decode(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            }
        } else {
            result.push(c);
        }
    }
    result
}
