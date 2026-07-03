use std::path::{Component, Path};

/// Validate an output path for screenshots.
/// This is agent-typo protection, not a security boundary: absolute paths to
/// any writable directory are accepted by design.
pub fn output_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("Output path cannot be empty".to_string());
    }
    if path.contains('\0') {
        return Err("Output path contains a null byte".to_string());
    }
    // Component-wise check: "build..v2.png" is a legal filename, "../x.png"
    // is traversal.
    if Path::new(path).components().any(|c| c == Component::ParentDir) {
        return Err(format!("Output path must not contain path traversal ('..'): {}", path));
    }
    if !path.to_ascii_lowercase().ends_with(".png") {
        return Err(format!("Output path must end in .png: {}", path));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_output_path() {
        assert!(output_path("/tmp/screenshot.png").is_ok());
        assert!(output_path("shot.png").is_ok());
    }

    #[test]
    fn test_output_path_empty() {
        assert!(output_path("").is_err());
    }

    #[test]
    fn test_output_path_null_byte() {
        assert!(output_path("/tmp/a\0b.png").is_err());
    }

    #[test]
    fn test_output_path_dots_in_filename_allowed() {
        // ".." as a substring of a filename is not path traversal
        assert!(output_path("/tmp/build..v2.png").is_ok());
    }

    #[test]
    fn test_output_path_uppercase_extension_allowed() {
        assert!(output_path("/tmp/shot.PNG").is_ok());
    }

    #[test]
    fn test_output_path_traversal() {
        assert!(output_path("/tmp/../etc/passwd.png").is_err());
        assert!(output_path("../shot.png").is_err());
    }

    #[test]
    fn test_output_path_bad_extension() {
        assert!(output_path("/tmp/shot.jpg").is_err());
        assert!(output_path("/tmp/shot").is_err());
    }
}
