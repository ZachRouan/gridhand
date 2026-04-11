use crate::{ZOOM_MIN_WIDTH, ZOOM_MIN_HEIGHT};

/// Auto-select grid density based on image dimensions.
/// Larger images get denser grids; smaller (zoomed) images get coarser grids.
/// Grid cells target ~40px minimum to keep labels readable while maximizing precision.
pub fn auto_grid(width: u32, height: u32) -> (u32, u32) {
    let max_cols = (width / 40).clamp(3, 8);
    let max_rows = (height / 40).clamp(3, 6);
    (max_cols, max_rows)
}

/// Parse a grid density string like "8x6" into (cols, rows).
pub fn parse_grid(s: &str) -> Result<(u32, u32), String> {
    let parts: Vec<&str> = s.split('x').collect();
    if parts.len() != 2 {
        return Err(format!("Invalid grid format '{}'. Use WxH (e.g., 4x3)", s));
    }
    let cols: u32 = parts[0].parse().map_err(|_| format!("Invalid grid columns: {}", parts[0]))?;
    let rows: u32 = parts[1].parse().map_err(|_| format!("Invalid grid rows: {}", parts[1]))?;
    if cols == 0 || rows == 0 || cols > 26 || rows > 9 {
        return Err("Grid dimensions must be 1-26 columns and 1-9 rows".to_string());
    }
    Ok((cols, rows))
}

/// Parse a single cell reference like "B2" into (col, row) zero-indexed.
pub fn parse_cell_ref(s: &str) -> Result<(u32, u32), String> {
    let bytes = s.as_bytes();
    if bytes.len() < 2 || bytes.len() > 3 {
        return Err(format!("Invalid cell reference '{}'. Use format like A1 or B2", s));
    }
    let col_char = bytes[0].to_ascii_uppercase();
    if !col_char.is_ascii_uppercase() {
        return Err(format!("Invalid column in cell '{}': must be A-Z", s));
    }
    let col = (col_char - b'A') as u32;
    let row_str = &s[1..];
    let row: u32 = row_str.parse::<u32>()
        .map_err(|_| format!("Invalid row in cell '{}': must be 1-9", s))?;
    if row == 0 {
        return Err(format!("Row must be 1 or greater in cell '{}'", s));
    }
    Ok((col, row - 1))
}

/// Compute absolute screen coordinates from a cell chain like "B2.C1".
/// Uses f64 throughout to avoid integer division drift.
/// Auto-scales grid density at each recursion level based on region size,
/// simulating the same scale-up that screenshot zoom applies (min 640x480)
/// so that grid densities match between screenshot and mouse move.
/// If `explicit_grid` is Some, uses that fixed density instead of auto-scaling.
/// Returns the center point of the innermost cell.
pub fn cell_to_coords(
    cell_chain: &str,
    bounds_x: i32,
    bounds_y: i32,
    bounds_w: u32,
    bounds_h: u32,
    explicit_grid: Option<(u32, u32)>,
) -> Result<(i32, i32), String> {
    let mut x = bounds_x as f64;
    let mut y = bounds_y as f64;
    let mut w = bounds_w as f64;
    let mut h = bounds_h as f64;

    let parts: Vec<&str> = cell_chain.split('.').collect();

    for (i, part) in parts.iter().enumerate() {
        let (grid_cols, grid_rows) = if let Some(g) = explicit_grid {
            g
        } else if i > 0 {
            // Simulate scale-up to minimum dimensions (matches screenshot behavior)
            let scaled_w = if (w as u32) < ZOOM_MIN_WIDTH || (h as u32) < ZOOM_MIN_HEIGHT {
                let scale_x = if w > 0.0 { (ZOOM_MIN_WIDTH as f64 / w).ceil() as u32 } else { 1 };
                let scale_y = if h > 0.0 { (ZOOM_MIN_HEIGHT as f64 / h).ceil() as u32 } else { 1 };
                let scale = scale_x.max(scale_y).max(1);
                (w as u32 * scale, h as u32 * scale)
            } else {
                (w as u32, h as u32)
            };
            auto_grid(scaled_w.0, scaled_w.1)
        } else {
            auto_grid(w as u32, h as u32)
        };

        let (col, row) = parse_cell_ref(part)?;
        if col >= grid_cols || row >= grid_rows {
            return Err(format!(
                "Cell '{}' out of range for {}x{} grid",
                part, grid_cols, grid_rows
            ));
        }
        let cell_w = w / grid_cols as f64;
        let cell_h = h / grid_rows as f64;
        x += col as f64 * cell_w;
        y += row as f64 * cell_h;
        w = cell_w;
        h = cell_h;
    }

    Ok(((x + w / 2.0) as i32, (y + h / 2.0) as i32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_grid_default() {
        assert_eq!(parse_grid("4x3").unwrap(), (4, 3));
    }

    #[test]
    fn test_parse_grid_custom() {
        assert_eq!(parse_grid("6x4").unwrap(), (6, 4));
        assert_eq!(parse_grid("10x8").unwrap(), (10, 8));
    }

    #[test]
    fn test_parse_grid_invalid() {
        assert!(parse_grid("abc").is_err());
        assert!(parse_grid("0x0").is_err());
        assert!(parse_grid("27x3").is_err());
    }

    #[test]
    fn test_parse_cell_ref() {
        assert_eq!(parse_cell_ref("A1").unwrap(), (0, 0));
        assert_eq!(parse_cell_ref("B2").unwrap(), (1, 1));
        assert_eq!(parse_cell_ref("D3").unwrap(), (3, 2));
    }

    #[test]
    fn test_parse_cell_ref_invalid() {
        assert!(parse_cell_ref("").is_err());
        assert!(parse_cell_ref("1A").is_err());
        assert!(parse_cell_ref("A0").is_err());
    }

    #[test]
    fn test_cell_to_coords_single() {
        let (x, y) = cell_to_coords("B2", 100, 50, 400, 300, Some((4, 3))).unwrap();
        assert_eq!(x, 250);
        assert_eq!(y, 200);
    }

    #[test]
    fn test_cell_to_coords_recursive() {
        let (x, y) = cell_to_coords("B2.A1", 0, 0, 400, 300, Some((4, 3))).unwrap();
        assert_eq!(x, 112);
        assert_eq!(y, 116);
    }

    #[test]
    fn test_cell_to_coords_out_of_range() {
        assert!(cell_to_coords("E1", 0, 0, 400, 300, Some((4, 3))).is_err());
        assert!(cell_to_coords("A4", 0, 0, 400, 300, Some((4, 3))).is_err());
    }

    #[test]
    fn test_auto_grid() {
        assert_eq!(auto_grid(1280, 800), (8, 6));
        assert_eq!(auto_grid(160, 133), (4, 3));
        assert_eq!(auto_grid(640, 400), (8, 6));
        assert_eq!(auto_grid(80, 80), (3, 3));
    }

    #[test]
    fn test_cell_to_coords_auto_grid() {
        let (x, y) = cell_to_coords("B2", 0, 0, 1280, 800, None).unwrap();
        assert_eq!(x, 240);
        assert_eq!(y, 200);
    }

    #[test]
    fn test_cell_to_coords_recursive_auto_grid_uses_scaled_density() {
        // A1 on 1280x800 (auto 8x6) → 160x133 region.
        // At level 1, this region is conceptually scaled up to 640x532 (4x)
        // to match the grid drawn on zoomed screenshots, giving auto 8x6.
        // C1 in that 8x6 sub-grid → center at (50, 11).
        // Without the scale-up simulation, auto_grid(160,133) = (4,3),
        // and C1 would target (100, 22) — the wrong spot.
        let (x, y) = cell_to_coords("A1.C1", 0, 0, 1280, 800, None).unwrap();
        assert_eq!(x, 50);
        assert_eq!(y, 11);
    }
}
