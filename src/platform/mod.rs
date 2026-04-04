#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "linux")]
pub use linux::*;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::*;

#[cfg(target_os = "windows")]
mod windows_os;
#[cfg(target_os = "windows")]
pub use windows_os::*;

pub(crate) mod png;

/// Integration tests for the platform-agnostic public API.
///
/// These tests exercise real OS functionality (mouse input, window management,
/// screenshots) and require a running desktop session with appropriate permissions.
/// They are `#[ignore]`d by default so they don't break headless CI.
///
/// Run locally with: `cargo test -- --ignored`
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn test_list_windows() {
        let result = list_windows();
        assert!(result.is_ok(), "list_windows failed: {:?}", result.err());
        let json = result.unwrap();
        assert!(json.contains("\"status\":\"success\""), "Missing success status: {}", json);
        assert!(json.contains("\"windows\""), "Missing windows field: {}", json);
    }

    #[test]
    #[ignore]
    fn test_mouse_move() {
        let result = mouse_move(10, 10);
        assert!(result.is_ok(), "mouse_move failed: {:?}", result.err());
        let json = result.unwrap();
        assert!(json.contains("\"status\":\"success\""), "Missing success status: {}", json);
    }

    #[test]
    #[ignore]
    fn test_mouse_click() {
        let result = mouse_click("left");
        assert!(result.is_ok(), "mouse_click failed: {:?}", result.err());
        let json = result.unwrap();
        assert!(json.contains("\"status\":\"success\""), "Missing success status: {}", json);
    }

    #[test]
    #[ignore]
    fn test_mouse_click_invalid_button() {
        let result = mouse_click("middle");
        assert!(result.is_err(), "Expected error for invalid button");
    }

    #[test]
    #[ignore]
    fn test_key_type() {
        let result = key_type("a");
        assert!(result.is_ok(), "key_type failed: {:?}", result.err());
        let json = result.unwrap();
        assert!(json.contains("\"status\":\"success\""), "Missing success status: {}", json);
    }

    #[test]
    #[ignore]
    fn test_key_press() {
        let result = key_press("ctrl+a");
        assert!(result.is_ok(), "key_press failed: {:?}", result.err());
        let json = result.unwrap();
        assert!(json.contains("\"status\":\"success\""), "Missing success status: {}", json);
    }

    #[test]
    #[ignore]
    fn test_key_press_invalid_combo() {
        let result = key_press("nonexistent_key");
        assert!(result.is_err(), "Expected error for invalid key");
    }

    #[test]
    #[ignore]
    fn test_screenshot_full() {
        let path = "/tmp/gui-tool-test-screenshot.png";
        let _ = std::fs::remove_file(path);

        let result = screenshot_full(path);
        assert!(result.is_ok(), "screenshot_full failed: {:?}", result.err());
        let json = result.unwrap();
        assert!(json.contains("\"status\":\"success\""), "Missing success status: {}", json);
        assert!(std::path::Path::new(path).exists(), "Screenshot file not created");

        // Verify it's a valid PNG (starts with PNG magic bytes)
        let data = std::fs::read(path).unwrap();
        assert!(data.len() > 8, "Screenshot file too small");
        assert_eq!(&data[..4], &[0x89, b'P', b'N', b'G'], "Not a valid PNG file");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    #[ignore]
    fn test_screenshot_window_by_id() {
        // First get a window ID from list
        let list_result = list_windows();
        assert!(list_result.is_ok());
        let json = list_result.unwrap();

        // Extract first window ID
        let windows_str = crate::json::extract_json_string(&json, "windows");
        assert!(windows_str.is_some(), "No windows field in: {}", json);
        let entries = crate::json::split_json_array(windows_str.unwrap());
        assert!(!entries.is_empty(), "No windows found");
        let first_id = crate::json::extract_json_number(entries[0], "id");
        assert!(first_id.is_some(), "No id in first window");

        let path = "/tmp/gui-tool-test-window-screenshot.png";
        let _ = std::fs::remove_file(path);

        let result = screenshot_window_by_id(first_id.unwrap() as u64, path);
        assert!(result.is_ok(), "screenshot_window_by_id failed: {:?}", result.err());
        assert!(std::path::Path::new(path).exists(), "Screenshot file not created");

        let data = std::fs::read(path).unwrap();
        assert_eq!(&data[..4], &[0x89, b'P', b'N', b'G'], "Not a valid PNG");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    #[ignore]
    fn test_find_window_by_title() {
        // Should find at least one window on a running desktop
        let result = find_window_by_title("a");
        assert!(result.is_ok(), "find_window_by_title failed: {:?}", result.err());
        // Result may be None if no window matches — that's OK, we just test it doesn't crash
    }
}
