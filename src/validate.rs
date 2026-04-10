/// Validate an output file path for screenshot commands.
/// Rejects null bytes, path traversal (../), and non-.png extensions.
pub fn output_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("Output path cannot be empty".to_string());
    }
    if path.bytes().any(|b| b == 0) {
        return Err("Output path contains null byte".to_string());
    }
    if path.contains("..") {
        return Err("Output path cannot contain '..' (path traversal)".to_string());
    }
    if !path.ends_with(".png") {
        return Err("Output path must end with .png".to_string());
    }
    Ok(())
}

/// Validate mouse coordinates are within reasonable screen bounds.
pub fn coordinates(x: i32, y: i32) -> Result<(), String> {
    // Allow negative coords for multi-monitor but reject absurd values
    const MAX_COORD: i32 = 32768;
    const MIN_COORD: i32 = -32768;
    if !(MIN_COORD..=MAX_COORD).contains(&x) {
        return Err(format!("X coordinate {} is out of range ({} to {})", x, MIN_COORD, MAX_COORD));
    }
    if !(MIN_COORD..=MAX_COORD).contains(&y) {
        return Err(format!("Y coordinate {} is out of range ({} to {})", y, MIN_COORD, MAX_COORD));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_output_path() {
        assert!(output_path("/tmp/screenshot.png").is_ok());
        assert!(output_path("./test.png").is_ok());
    }

    #[test]
    fn test_output_path_traversal() {
        assert!(output_path("/tmp/../etc/screenshot.png").is_err());
        assert!(output_path("../../secret.png").is_err());
    }

    #[test]
    fn test_output_path_bad_extension() {
        assert!(output_path("/tmp/screenshot.jpg").is_err());
        assert!(output_path("/tmp/screenshot").is_err());
    }

    #[test]
    fn test_output_path_null_byte() {
        assert!(output_path("/tmp/scr\0eenshot.png").is_err());
    }

    #[test]
    fn test_output_path_empty() {
        assert!(output_path("").is_err());
    }

    #[test]
    fn test_valid_coordinates() {
        assert!(coordinates(0, 0).is_ok());
        assert!(coordinates(1920, 1080).is_ok());
        assert!(coordinates(-500, 200).is_ok());
    }

    #[test]
    fn test_coordinates_out_of_range() {
        assert!(coordinates(100000, 0).is_err());
        assert!(coordinates(0, -100000).is_err());
    }
}
