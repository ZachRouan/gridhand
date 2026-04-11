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
}
