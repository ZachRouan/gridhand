//! KDE Plasma / KWin desktop integration.
//!
//! KWin does not expose non-interactive window enumeration directly on its
//! public D-Bus object. Its scripting API does expose workspace windows and
//! output geometry, so Gridhand loads a tiny temporary script, runs it, and
//! receives the result through KWin's callDBus() helper.
//!
//! No external helper binary or Rust crate is required: this uses Gridhand's
//! existing raw session-bus implementation.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::json::{self, JsonValue};

use super::dbus::DbusConnection;
use super::dbus::types::{MarshalBuffer, UnmarshalBuffer};

const KWIN_DEST: &str = "org.kde.KWin";
const SCRIPTING_PATH: &str = "/Scripting";
const SCRIPTING_IFACE: &str = "org.kde.kwin.Scripting";
const SCRIPT_IFACE: &str = "org.kde.kwin.Script";

const BRIDGE_PATH: &str = "/io/github/gridhand/KWinBridge";
const BRIDGE_IFACE: &str = "io.github.gridhand.KWinBridge";
const BRIDGE_MEMBER: &str = "Report";
const CALLBACK_TIMEOUT_MS: u64 = 5_000;

static SCRIPT_COUNTER: AtomicU32 = AtomicU32::new(0);

struct TempScript {
    path: PathBuf,
    plugin_name: String,
}

impl TempScript {
    fn create(source: &str) -> Result<Self, String> {
        let seq = SCRIPT_COUNTER.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| format!("System clock error: {}", e))?
            .as_nanos();
        let plugin_name = format!("gridhand_{}_{}", std::process::id(), seq);
        let path = std::env::temp_dir().join(format!("{}_{}.js", plugin_name, nanos));

        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .map_err(|e| format!("Failed to create KWin script {}: {}", path.display(), e))?;
        file.write_all(source.as_bytes())
            .map_err(|e| format!("Failed to write KWin script {}: {}", path.display(), e))?;
        file.flush()
            .map_err(|e| format!("Failed to flush KWin script {}: {}", path.display(), e))?;

        Ok(Self { path, plugin_name })
    }

    fn path_str(&self) -> Result<&str, String> {
        self.path
            .to_str()
            .ok_or_else(|| "KWin script path is not valid UTF-8".to_string())
    }
}

impl Drop for TempScript {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn js_string(value: &str) -> String {
    JsonValue::Str(value).to_string()
}

fn bridge_prelude(conn: &DbusConnection) -> String {
    format!(
        "function gridhandReport(value) {{ callDBus({}, {}, {}, {}, String(value)); }}\n",
        js_string(conn.unique_name()),
        js_string(BRIDGE_PATH),
        js_string(BRIDGE_IFACE),
        js_string(BRIDGE_MEMBER),
    )
}

fn unload_script(conn: &mut DbusConnection, plugin_name: &str) {
    let mut body = MarshalBuffer::new();
    body.write_string(plugin_name);
    let _ = conn.call_method(
        KWIN_DEST,
        SCRIPTING_PATH,
        SCRIPTING_IFACE,
        "unloadScript",
        Some("s"),
        &body.into_bytes(),
    );
}

fn run_script(conn: &mut DbusConnection, source: &str) -> Result<String, String> {
    let source = format!("{}{}", bridge_prelude(conn), source);
    let script = TempScript::create(&source)?;

    let mut load_body = MarshalBuffer::new();
    load_body.write_string(script.path_str()?);
    load_body.write_string(&script.plugin_name);
    let reply = conn.call_method(
        KWIN_DEST,
        SCRIPTING_PATH,
        SCRIPTING_IFACE,
        "loadScript",
        Some("ss"),
        &load_body.into_bytes(),
    )?;

    let script_id = UnmarshalBuffer::new(&reply.body).read_i32()?;
    if script_id < 0 {
        return Err(format!("KWin loadScript returned {}", script_id));
    }

    let script_path = format!("/Scripting/Script{}", script_id);
    if let Err(e) =
        conn.call_method_no_reply(KWIN_DEST, &script_path, SCRIPT_IFACE, "run", None, &[])
    {
        unload_script(conn, &script.plugin_name);
        return Err(e);
    }

    let result = (|| {
        let call = conn.wait_for_method_call(
            BRIDGE_PATH,
            BRIDGE_IFACE,
            BRIDGE_MEMBER,
            CALLBACK_TIMEOUT_MS,
        )?;

        if call.header.signature.as_deref() != Some("s") {
            let signature = call.header.signature.as_deref().unwrap_or("").to_string();
            let _ = conn.send_empty_method_return(&call);
            return Err(format!(
                "KWin callback had unexpected signature '{}'",
                signature
            ));
        }

        let payload = UnmarshalBuffer::new(&call.body).read_string();
        let reply_result = conn.send_empty_method_return(&call);
        let payload = payload?;
        reply_result?;
        Ok(payload)
    })();

    unload_script(conn, &script.plugin_name);
    result
}

fn list_windows_script() -> &'static str {
    r#"
(function() {
    var out = [];
    var list = (typeof workspace.windowList === "function")
        ? workspace.windowList()
        : workspace.clientList();

    for (var i = 0; i < list.length; i++) {
        var w = list[i];
        if (!w || w.deleted || w.desktopWindow || w.notification || w.managed === false) {
            continue;
        }

        var g = w.frameGeometry;
        var id = String(w.internalId || w.windowId || "");
        if (!g || !id || g.width <= 0 || g.height <= 0) {
            continue;
        }

        out.push({
            id: id,
            title: String(w.caption || ""),
            wm_class: String(w.resourceClass || w.resourceName || ""),
            pid: Number(w.pid || 0),
            x: Math.round(g.x),
            y: Math.round(g.y),
            width: Math.round(g.width),
            height: Math.round(g.height),
            has_focus: w === workspace.activeWindow
        });
    }

    gridhandReport(JSON.stringify(out));
})();
"#
}

fn list_windows_json(conn: &mut DbusConnection) -> Result<String, String> {
    run_script(conn, list_windows_script())
}

pub fn list_windows() -> Result<String, String> {
    let mut conn = DbusConnection::connect()?;
    let windows_json = list_windows_json(&mut conn)?;
    Ok(json::success_with(vec![(
        "windows",
        JsonValue::RawJson(windows_json),
    )]))
}

pub fn find_window_by_title_with_conn(
    conn: &mut DbusConnection,
    title: &str,
) -> Result<Option<(String, String)>, String> {
    let windows_json = list_windows_json(conn)?;
    let needle = title.to_lowercase();

    for window in json::split_json_array(&windows_json) {
        let caption = json::extract_json_string(window, "title").unwrap_or_default();
        let app = json::extract_json_string(window, "wm_class").unwrap_or_default();
        if caption.to_lowercase().contains(&needle) || app.to_lowercase().contains(&needle) {
            let id = json::extract_json_string(window, "id")
                .ok_or("KWin window entry missing string id")?;
            return Ok(Some((id, window.to_string())));
        }
    }

    Ok(None)
}

pub fn window_details_with_conn(conn: &mut DbusConnection, id: &str) -> Result<String, String> {
    let windows_json = list_windows_json(conn)?;
    for window in json::split_json_array(&windows_json) {
        if json::extract_json_string(window, "id").as_deref() == Some(id) {
            return Ok(window.to_string());
        }
    }
    Err(format!("No KWin window found with id '{}'", id))
}

fn raise_window_script(id: &str) -> String {
    format!(
        r#"
(function() {{
    var target = {};
    var list = (typeof workspace.windowList === "function")
        ? workspace.windowList()
        : workspace.clientList();
    var found = false;

    for (var i = 0; i < list.length; i++) {{
        var w = list[i];
        if (String(w.internalId || w.windowId || "") === target) {{
            workspace.activeWindow = w;
            found = true;
            break;
        }}
    }}

    gridhandReport(found ? "1" : "0");
}})();
"#,
        js_string(id)
    )
}

pub fn raise_window_with_conn(conn: &mut DbusConnection, id: &str) -> Result<String, String> {
    match run_script(conn, &raise_window_script(id))?.as_str() {
        "1" => Ok(json::success()),
        _ => Err(format!("No KWin window found with id '{}'", id)),
    }
}

fn desktop_size_script() -> &'static str {
    r#"
(function() {
    var g = workspace.virtualScreenGeometry;
    if (g) {
        gridhandReport(JSON.stringify({
            x: Math.round(g.x),
            y: Math.round(g.y),
            width: Math.round(g.width),
            height: Math.round(g.height)
        }));
        return;
    }

    var s = workspace.virtualScreenSize;
    gridhandReport(JSON.stringify({
        x: 0,
        y: 0,
        width: s ? Math.round(s.width) : 0,
        height: s ? Math.round(s.height) : 0
    }));
})();
"#
}

pub(crate) fn virtual_geometry_with_conn(
    conn: &mut DbusConnection,
) -> Result<(i64, i64, i64, i64), String> {
    let payload = run_script(conn, desktop_size_script())?;
    let x = json::extract_json_number(&payload, "x").ok_or("KWin virtual geometry missing 'x'")?;
    let y = json::extract_json_number(&payload, "y").ok_or("KWin virtual geometry missing 'y'")?;
    let width = json::extract_json_number(&payload, "width")
        .ok_or("KWin virtual geometry missing 'width'")?;
    let height = json::extract_json_number(&payload, "height")
        .ok_or("KWin virtual geometry missing 'height'")?;

    if width <= 0 || height <= 0 {
        return Err(format!(
            "KWin reported invalid virtual geometry {}x{} at ({}, {})",
            width, height, x, y
        ));
    }

    Ok((x, y, width, height))
}

pub fn logical_desktop_size() -> Result<(i32, i32), String> {
    let mut conn = DbusConnection::connect()?;
    let (_, _, width, height) = virtual_geometry_with_conn(&mut conn)?;
    Ok((width as i32, height as i32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripts_use_kwin_window_api_and_bridge() {
        let list = list_windows_script();
        assert!(list.contains("workspace.windowList"));
        assert!(list.contains("workspace.clientList"));
        assert!(list.contains("internalId"));
        assert!(list.contains("g.width <= 0"));
        assert!(list.contains("g.height <= 0"));
        assert!(list.contains("gridhandReport"));
    }

    #[test]
    fn raise_script_quotes_opaque_window_id() {
        let script = raise_window_script("{ab\"c\\d}");
        assert!(script.contains(r#"var target = "{ab\"c\\d}";"#));
        assert!(script.contains("workspace.activeWindow = w"));
    }
}
