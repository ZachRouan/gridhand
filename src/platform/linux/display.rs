//! Desktop-size detection via org.gnome.Mutter.DisplayConfig.
//!
//! The DRM-sysfs heuristic (sum widths of connected outputs, first listed
//! mode) is wrong for mirrored outputs, vertical stacks, and non-native
//! resolutions. Mutter's GetCurrentState reports the actual logical monitor
//! layout — the same coordinate space window-calls reports window bounds in.
//! Any failure returns None and the caller falls back to the heuristic.

use std::collections::HashMap;

use super::dbus::types::UnmarshalBuffer;
use super::dbus::DbusConnection;

const DEST: &str = "org.gnome.Mutter.DisplayConfig";
const PATH: &str = "/org/gnome/Mutter/DisplayConfig";
const IFACE: &str = "org.gnome.Mutter.DisplayConfig";

/// Query Mutter for the current monitor layout and return the logical
/// desktop extent `(width, height)` — the bounding box of all logical
/// monitors in the compositor's coordinate space. `None` on any failure
/// (D-Bus unavailable, non-GNOME compositor, unexpected reply shape): the
/// caller falls back to the DRM-sysfs heuristic.
pub fn logical_desktop_size() -> Option<(i32, i32)> {
    query_and_parse().ok().flatten()
}

fn query_and_parse() -> Result<Option<(i32, i32)>, String> {
    let mut conn = DbusConnection::connect()?;
    let reply = conn.call_method(DEST, PATH, IFACE, "GetCurrentState", None, &[])?;
    parse_current_state(&reply.body)
}

/// Reply body signature: `u a((ssss)a(siiddada{sv})a{sv}) a(iiduba(ssss)a{sv}) a{sv}`
/// — serial, monitors, logical_monitors, properties.
fn parse_current_state(body: &[u8]) -> Result<Option<(i32, i32)>, String> {
    let mut ubuf = UnmarshalBuffer::new(body);

    let _serial = ubuf.read_u32()?;

    let modes_by_connector = parse_monitors(&mut ubuf)?;

    let logical_end = read_array_end(&mut ubuf, 8)?; // a(iiduba(ssss)a{sv}): struct-of alignment 8
    let mut max_x: i32 = 0;
    let mut max_y: i32 = 0;
    let mut count: u32 = 0;
    while ubuf.pos < logical_end {
        if let Some((x, y, lw, lh)) = read_logical_monitor(&mut ubuf, &modes_by_connector)? {
            count += 1;
            max_x = max_x.max(x.saturating_add(lw));
            max_y = max_y.max(y.saturating_add(lh));
        }
    }

    // Trailing top-level `a{sv}` properties: not needed, and not read below,
    // but leaving it unconsumed is harmless — we're done with the buffer.

    if count == 0 {
        return Ok(None);
    }
    Ok(Some((max_x, max_y)))
}

/// Read an array's 4-byte length prefix, align to the element type's
/// alignment, and return the byte offset where the array's elements end.
fn read_array_end(ubuf: &mut UnmarshalBuffer, element_alignment: usize) -> Result<usize, String> {
    let len = ubuf.read_u32()? as usize;
    ubuf.align(element_alignment);
    ubuf.pos.checked_add(len).ok_or_else(|| "D-Bus array length overflow".to_string())
}

/// Pass 1: map connector name -> (width, height) of its currently active
/// mode, from the `monitors` array.
fn parse_monitors(ubuf: &mut UnmarshalBuffer) -> Result<HashMap<String, (i32, i32)>, String> {
    let end = read_array_end(ubuf, 8)?; // monitor struct alignment 8
    let mut map = HashMap::new();
    while ubuf.pos < end {
        let (connector, current_mode) = read_monitor(ubuf)?;
        if let Some(size) = current_mode {
            map.insert(connector, size);
        }
    }
    Ok(map)
}

/// One `(ssss)a(siiddada{sv})a{sv}` monitor struct. Returns the connector
/// name and, if one mode's props carried `"is-current": true`, its size.
fn read_monitor(ubuf: &mut UnmarshalBuffer) -> Result<(String, Option<(i32, i32)>), String> {
    ubuf.align(8);

    // (ssss): connector, vendor, product, serial — only connector matters.
    ubuf.align(8);
    let connector = ubuf.read_string()?;
    let _vendor = ubuf.read_string()?;
    let _product = ubuf.read_string()?;
    let _serial = ubuf.read_string()?;

    // a(siiddada{sv}) modes
    let modes_end = read_array_end(ubuf, 8)?; // mode struct alignment 8
    let mut current_mode = None;
    while ubuf.pos < modes_end {
        let (w, h, is_current) = read_mode(ubuf)?;
        if is_current {
            current_mode = Some((w, h));
        }
    }

    // a{sv} monitor-level properties — not needed.
    ubuf.skip_value("a{sv}", 0)?;

    Ok((connector, current_mode))
}

/// One `(s id, i width, i height, d refresh, d preferred_scale,
/// ad supported_scales, a{sv} props)` mode struct.
fn read_mode(ubuf: &mut UnmarshalBuffer) -> Result<(i32, i32, bool), String> {
    ubuf.align(8);
    let _id = ubuf.read_string()?;
    let width = ubuf.read_i32()?;
    let height = ubuf.read_i32()?;
    let _refresh = ubuf.read_double()?;
    let _preferred_scale = ubuf.read_double()?;
    ubuf.skip_value("ad", 0)?; // supported_scales — not needed
    let is_current = scan_bool_prop(ubuf, "is-current")?;
    Ok((width, height, is_current))
}

/// Scan an `a{sv}` properties dict for a boolean-valued key, consuming the
/// whole dict regardless of whether the key is found (so the caller's
/// position ends up correctly past it either way). A missing key or a
/// non-boolean value for it is treated as `false`, not an error — an
/// unrecognized or absent property on a well-formed reply isn't malformed
/// data, just not the flag we're looking for.
fn scan_bool_prop(ubuf: &mut UnmarshalBuffer, key: &str) -> Result<bool, String> {
    let end = read_array_end(ubuf, 8)?; // dict-entry alignment 8
    let mut found = false;
    while ubuf.pos < end {
        ubuf.align(8);
        let k = ubuf.read_string()?;
        let sig = ubuf.read_signature()?;
        if k == key && sig == "b" {
            found = ubuf.read_bool()?;
        } else {
            ubuf.skip_value(&sig, 0)?;
        }
    }
    Ok(found)
}

/// One `(i x, i y, d scale, u transform, b primary, a(ssss) assigned,
/// a{sv} props)` logical monitor struct. Returns `(x, y, logical_w,
/// logical_h)` if the first assigned connector's current mode is known;
/// `None` if the monitor can't be resolved (no assigned connector, unknown
/// connector, or a non-positive scale).
fn read_logical_monitor(
    ubuf: &mut UnmarshalBuffer,
    modes_by_connector: &HashMap<String, (i32, i32)>,
) -> Result<Option<(i32, i32, i32, i32)>, String> {
    ubuf.align(8);
    let x = ubuf.read_i32()?;
    let y = ubuf.read_i32()?;
    let scale = ubuf.read_double()?;
    let transform = ubuf.read_u32()?;
    let _primary = ubuf.read_bool()?;

    // a(ssss) assigned monitors — only the first entry's connector matters,
    // but every entry must still be consumed to leave `pos` correct.
    let assigned_end = read_array_end(ubuf, 8)?; // (ssss) struct alignment 8
    let mut first_connector: Option<String> = None;
    while ubuf.pos < assigned_end {
        ubuf.align(8);
        let connector = ubuf.read_string()?;
        let _vendor = ubuf.read_string()?;
        let _product = ubuf.read_string()?;
        let _serial = ubuf.read_string()?;
        if first_connector.is_none() {
            first_connector = Some(connector);
        }
    }

    // a{sv} logical-monitor properties — not needed.
    ubuf.skip_value("a{sv}", 0)?;

    let Some(connector) = first_connector else { return Ok(None) };
    let Some(&(mw, mh)) = modes_by_connector.get(&connector) else { return Ok(None) };

    // Transforms 1/3/5/7 are the 90 and 270 degree rotation families
    // (with and without flip) and swap the mode's width and height.
    let (mw, mh) = if matches!(transform, 1 | 3 | 5 | 7) { (mh, mw) } else { (mw, mh) };

    if scale <= 0.0 || !scale.is_finite() {
        return Ok(None);
    }
    let logical_w = (mw as f64 / scale).round() as i32;
    let logical_h = (mh as f64 / scale).round() as i32;

    Ok(Some((x, y, logical_w, logical_h)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::dbus::types::MarshalBuffer;

    /// Append a raw little-endian f64 at the current (8-aligned) position.
    /// MarshalBuffer has no write_double helper; write the bytes directly
    /// the same way the existing D-Bus tests do.
    fn push_double(buf: &mut MarshalBuffer, v: f64) {
        buf.align(8);
        buf.data.extend_from_slice(&v.to_le_bytes());
    }

    /// Build a synthetic GetCurrentState reply body for one monitor with
    /// one (current) mode, and one logical monitor covering it at the
    /// given origin/scale/transform.
    #[allow(clippy::too_many_arguments)]
    fn build_reply(
        connector: &str,
        mode_w: i32,
        mode_h: i32,
        x: i32,
        y: i32,
        scale: f64,
        transform: u32,
        assign_connector: Option<&str>,
    ) -> Vec<u8> {
        let mut buf = MarshalBuffer::new();
        buf.write_u32(1); // serial

        // monitors: a((ssss)a(siiddada{sv})a{sv})
        let monitors_pos = buf.start_array(8);
        buf.align_struct();
        // (ssss)
        buf.align_struct();
        buf.write_string(connector);
        buf.write_string("VEN");
        buf.write_string("PROD");
        buf.write_string("SER");
        // a(siiddada{sv}) modes — one mode, marked current
        let modes_pos = buf.start_array(8);
        buf.align_struct();
        buf.write_string("mode-id");
        buf.write_i32(mode_w);
        buf.write_i32(mode_h);
        push_double(&mut buf, 60.0); // refresh
        push_double(&mut buf, 1.0); // preferred_scale
        // ad supported_scales (empty)
        let scales_pos = buf.start_array(8);
        buf.finish_array(scales_pos, 8);
        // a{sv} mode props: { "is-current": true }
        let mode_props_pos = buf.start_array(8);
        buf.align_struct();
        buf.write_string("is-current");
        buf.write_variant_bool(true);
        buf.finish_array(mode_props_pos, 8);
        buf.finish_array(modes_pos, 8);
        // a{sv} monitor props (empty)
        let monitor_props_pos = buf.start_array(8);
        buf.finish_array(monitor_props_pos, 8);
        buf.finish_array(monitors_pos, 8);

        // logical_monitors: a(iiduba(ssss)a{sv})
        let logical_pos = buf.start_array(8);
        buf.align_struct();
        buf.write_i32(x);
        buf.write_i32(y);
        push_double(&mut buf, scale);
        buf.write_u32(transform);
        buf.write_boolean(true); // primary
        // a(ssss) assigned
        let assigned_pos = buf.start_array(8);
        if let Some(ac) = assign_connector {
            buf.align_struct();
            buf.write_string(ac);
            buf.write_string("VEN");
            buf.write_string("PROD");
            buf.write_string("SER");
        }
        buf.finish_array(assigned_pos, 8);
        // a{sv} logical monitor props (empty)
        let lprops_pos = buf.start_array(8);
        buf.finish_array(lprops_pos, 8);
        buf.finish_array(logical_pos, 8);

        // top-level a{sv} properties (empty)
        let tprops_pos = buf.start_array(8);
        buf.finish_array(tprops_pos, 8);

        buf.into_bytes()
    }

    #[test]
    fn test_parse_current_state_single_monitor_scale_1() {
        let body = build_reply("DP-1", 1920, 1080, 0, 0, 1.0, 0, Some("DP-1"));
        assert_eq!(parse_current_state(&body).unwrap(), Some((1920, 1080)));
    }

    #[test]
    fn test_parse_current_state_applies_fractional_scale() {
        // 3840x2160 mode at scale 1.5 -> logical 2560x1440.
        let body = build_reply("DP-1", 3840, 2160, 0, 0, 1.5, 0, Some("DP-1"));
        assert_eq!(parse_current_state(&body).unwrap(), Some((2560, 1440)));
    }

    #[test]
    fn test_parse_current_state_swaps_dimensions_for_rotated_transform() {
        // transform 1 = 90 degrees: a 1920x1080 mode becomes a 1080x1920
        // logical monitor.
        let body = build_reply("DP-1", 1920, 1080, 0, 0, 1.0, 1, Some("DP-1"));
        assert_eq!(parse_current_state(&body).unwrap(), Some((1080, 1920)));
    }

    #[test]
    fn test_parse_current_state_offsets_extent_by_origin() {
        // A second-monitor-style origin at (1920, 0) with a 1920x1080 mode
        // yields an extent of (3840, 1080), matching a side-by-side layout.
        let body = build_reply("DP-1", 1920, 1080, 1920, 0, 1.0, 0, Some("DP-1"));
        assert_eq!(parse_current_state(&body).unwrap(), Some((3840, 1080)));
    }

    #[test]
    fn test_parse_current_state_unassigned_logical_monitor_yields_none() {
        // A logical monitor with no assigned connector can't be resolved;
        // with it being the only logical monitor, the whole parse yields
        // None (not a panic, not a bogus 0x0).
        let body = build_reply("DP-1", 1920, 1080, 0, 0, 1.0, 0, None);
        assert_eq!(parse_current_state(&body).unwrap(), None);
    }

    #[test]
    fn test_parse_current_state_unknown_connector_yields_none() {
        // The logical monitor references a connector that isn't in the
        // monitors array at all (shouldn't happen on a real compositor,
        // but malformed/adversarial replies must not panic or fabricate a
        // size).
        let body = build_reply("DP-1", 1920, 1080, 0, 0, 1.0, 0, Some("DP-99"));
        assert_eq!(parse_current_state(&body).unwrap(), None);
    }

    #[test]
    fn test_parse_current_state_truncated_body_errors_not_panics() {
        let mut body = build_reply("DP-1", 1920, 1080, 0, 0, 1.0, 0, Some("DP-1"));
        body.truncate(body.len() / 2);
        assert!(parse_current_state(&body).is_err());
    }

    #[test]
    fn test_parse_current_state_empty_body_errors_not_panics() {
        assert!(parse_current_state(&[]).is_err());
    }

    #[test]
    fn test_parse_current_state_zero_scale_yields_none_not_divide_by_zero() {
        let body = build_reply("DP-1", 1920, 1080, 0, 0, 0.0, 0, Some("DP-1"));
        assert_eq!(parse_current_state(&body).unwrap(), None);
    }

    #[test]
    #[ignore] // requires a running GNOME session on the D-Bus session bus
    fn test_logical_desktop_size_matches_desktop() {
        // Ground truth captured by hand from this machine's live
        // `gdbus call --session --dest org.gnome.Mutter.DisplayConfig
        // --object-path /org/gnome/Mutter/DisplayConfig --method
        // org.gnome.Mutter.DisplayConfig.GetCurrentState` output: two
        // side-by-side 1920x1080 @ scale 1.0 monitors,
        //   logical_monitors: [(0, 0, 1.0, 0, false, [HDMI-2], {}),
        //                      (1920, 0, 1.0, 0, true, [DP-4], {})]
        // giving a bounding extent of (1920+1920, max(1080,1080)) = (3840, 1080).
        // The DRM-sysfs heuristic independently sums the same two connected
        // outputs' first listed mode (1920x1080 each) to the same (3840, 1080)
        // on this machine, so this case doesn't by itself distinguish the two
        // — see the task report for that comparison.
        let result = logical_desktop_size();
        assert_eq!(result, Some((3840, 1080)), "logical_desktop_size() must match gdbus ground truth");
    }

    #[test]
    fn test_logical_desktop_size_never_panics_without_a_session_bus() {
        // Smoke test for the public entry point's error handling: even in
        // an environment where D-Bus is entirely unavailable, this must
        // return None, never panic or propagate an Err.
        let _ = logical_desktop_size();
    }
}
