use crate::{ZOOM_MIN_WIDTH, ZOOM_MIN_HEIGHT};

/// Auto-select grid density for the initial (full) screenshot.
/// Dense grid for maximum first-pass precision.
/// Caps at 16 columns (A-P) and 9 rows (1-9).
pub fn auto_grid(width: u32, height: u32) -> (u32, u32) {
    let max_cols = (width / 40).clamp(3, 16);
    let max_rows = (height / 40).clamp(3, 9);
    (max_cols, max_rows)
}

/// Auto-select grid density for zoomed sub-grids.
/// Coarser than the initial grid — zoomed views need fewer, larger cells
/// since the agent is already narrowed to a small region.
/// Caps at 8 columns and 6 rows.
pub fn auto_grid_zoom(width: u32, height: u32) -> (u32, u32) {
    let max_cols = (width / 40).clamp(3, 8);
    let max_rows = (height / 40).clamp(3, 6);
    (max_cols, max_rows)
}

/// Parse a grid density string like "8x6" or "8X6" into (cols, rows).
pub fn parse_grid(s: &str) -> Result<(u32, u32), String> {
    let lower = s.to_ascii_lowercase();
    let parts: Vec<&str> = lower.split('x').collect();
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

/// Two adjacent (col, row) cells named by a between-cell reference.
pub type CellPair = ((u32, u32), (u32, u32));

/// Parse a between-cell reference like "D3+E3" into two (col, row) pairs.
/// Validates that the two cells are adjacent (horizontally, vertically, or diagonally).
pub fn parse_between_ref(s: &str) -> Result<CellPair, String> {
    let halves: Vec<&str> = s.split('+').collect();
    if halves.len() != 2 {
        return Err(format!("Invalid between-cell reference '{}'. Use format like D3+E3", s));
    }
    let (col1, row1) = parse_cell_ref(halves[0])?;
    let (col2, row2) = parse_cell_ref(halves[1])?;
    let dcol = (col2 as i32 - col1 as i32).abs();
    let drow = (row2 as i32 - row1 as i32).abs();
    if dcol > 1 || drow > 1 || (dcol == 0 && drow == 0) {
        return Err(format!(
            "Cells in '{}' must be adjacent (horizontally, vertically, or diagonally)", s
        ));
    }
    Ok(((col1, row1), (col2, row2)))
}

/// Half-open pixel span [start, end) of cell `i` of `n` across `total` pixels.
/// Proportional boundaries floor(i*total/n): cells partition the space exactly
/// (no unreachable remainder) and stay within 1px of uniform. Crop, click, and
/// overlay drawing must all derive cell geometry from here — three separate
/// computations of "cell size" is how the overlay drifted from the click path.
pub fn cell_span(total: u32, n: u32, i: u32) -> (u32, u32) {
    let start = (i as u64 * total as u64 / n as u64) as u32;
    let end = ((i as u64 + 1) * total as u64 / n as u64) as u32;
    (start, end)
}

/// Walk a cell chain like "B2.C1" within a WxH pixel space, returning the
/// center point of the innermost cell (or the boundary midpoint for a
/// between-cell ref) in that space.
/// Uses f64 throughout to avoid integer division drift.
/// Auto-scales grid density at each recursion level based on region size,
/// simulating the same scale-up that screenshot zoom applies (min 640x480)
/// so that grid densities match between screenshot and mouse move.
/// If `explicit_grid` is Some, uses that fixed density instead of auto-scaling.
fn cell_chain_center(
    cell_chain: &str,
    space_w: u32,
    space_h: u32,
    explicit_grid: Option<(u32, u32)>,
) -> Result<(f64, f64), String> {
    let mut x = 0f64;
    let mut y = 0f64;
    let mut w = space_w as f64;
    let mut h = space_h as f64;

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
            auto_grid_zoom(scaled_w.0, scaled_w.1)
        } else {
            auto_grid(w as u32, h as u32)
        };

        // Cell geometry comes from cell_span — the same function the
        // screenshot crop loop and the overlay renderer use. Any local
        // re-derivation here WILL drift from what the agent saw drawn.
        let (probe_w0, probe_w1) = cell_span(w as u32, grid_cols, 0);
        let (probe_h0, probe_h1) = cell_span(h as u32, grid_rows, 0);
        if probe_w1 == probe_w0 || probe_h1 == probe_h0 {
            return Err(format!(
                "Zoom chain '{}' is too deep: cell size reaches zero at level {}. Use fewer levels.",
                cell_chain, i + 1
            ));
        }

        if part.contains('+') {
            let ((col1, row1), (col2, row2)) = parse_between_ref(part)?;
            if col1 >= grid_cols || row1 >= grid_rows || col2 >= grid_cols || row2 >= grid_rows {
                return Err(format!(
                    "Cell '{}' out of range for {}x{} grid",
                    part, grid_cols, grid_rows
                ));
            }
            let (s1x, e1x) = cell_span(w as u32, grid_cols, col1);
            let (s2x, _) = cell_span(w as u32, grid_cols, col2);
            let (s1y, e1y) = cell_span(h as u32, grid_rows, row1);
            let (s2y, _) = cell_span(h as u32, grid_rows, row2);
            let span_w = e1x - s1x;
            let span_h = e1y - s1y;
            // Integer midpoint of the two cell origins, clamped so a
            // span_w-wide region starting there stays inside the space —
            // mirrors the screenshot crop loop exactly.
            let mid_x = ((s1x + s2x) / 2).min((w as u32).saturating_sub(span_w));
            let mid_y = ((s1y + s2y) / 2).min((h as u32).saturating_sub(span_h));
            x += mid_x as f64;
            y += mid_y as f64;
            w = span_w as f64;
            h = span_h as f64;
        } else {
            let (col, row) = parse_cell_ref(part)?;
            if col >= grid_cols || row >= grid_rows {
                return Err(format!(
                    "Cell '{}' out of range for {}x{} grid",
                    part, grid_cols, grid_rows
                ));
            }
            let (sx, ex) = cell_span(w as u32, grid_cols, col);
            let (sy, ey) = cell_span(h as u32, grid_rows, row);
            x += sx as f64;
            y += sy as f64;
            w = (ex - sx) as f64;
            h = (ey - sy) as f64;
        }
    }

    Ok((x + w / 2.0, y + h / 2.0))
}

/// Compute absolute screen coordinates from a cell chain by doing all grid
/// math in the screenshot's pixel space — the frame the agent actually looked
/// at — then mapping the point linearly onto the window bounds. This keeps
/// cell labels meaning the same region in the image and the click even when
/// the screenshot is scaled relative to the bounds (e.g. HiDPI backing
/// stores, where a 400x300-pt window yields an 800x600-px capture).
#[allow(clippy::too_many_arguments)]
pub fn cell_to_screen_coords(
    cell_chain: &str,
    img_w: u32,
    img_h: u32,
    bounds_x: i32,
    bounds_y: i32,
    bounds_w: u32,
    bounds_h: u32,
    explicit_grid: Option<(u32, u32)>,
) -> Result<(i32, i32), String> {
    if img_w == 0 || img_h == 0 {
        return Err("Screenshot image has zero dimensions".to_string());
    }
    let (cx, cy) = cell_chain_center(cell_chain, img_w, img_h, explicit_grid)?;
    let sx = bounds_x as f64 + cx * bounds_w as f64 / img_w as f64;
    let sy = bounds_y as f64 + cy * bounds_h as f64 / img_h as f64;
    Ok((sx as i32, sy as i32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cell_span_partitions_exactly() {
        // Every pixel belongs to exactly one cell: spans tile [0, total).
        for &(total, n) in &[(95u32, 8u32), (800, 9), (1280, 16), (7, 3), (100, 8)] {
            let mut covered = 0u32;
            for i in 0..n {
                let (start, end) = cell_span(total, n, i);
                assert_eq!(start, covered, "gap before cell {i} of {n} over {total}");
                assert!(end > start || total < n, "empty cell {i}");
                covered = end;
            }
            assert_eq!(covered, total, "dead zone after last cell: {covered} != {total}");
        }
    }

    #[test]
    fn test_cell_span_uniformity() {
        // Neighboring cells differ by at most 1px.
        let widths: Vec<u32> = (0..8).map(|i| { let (s, e) = cell_span(95, 8, i); e - s }).collect();
        let min = *widths.iter().min().unwrap();
        let max = *widths.iter().max().unwrap();
        assert!(max - min <= 1, "spans not uniform: {widths:?}");
    }

    /// Bounds-space targeting is the img == bounds identity case of
    /// cell_to_screen_coords (the 1:1 fallback used when no screenshot
    /// exists). Kept as a helper so the coordinate expectations below stay
    /// written in the familiar bounds-space form.
    fn cell_to_coords(
        chain: &str,
        bx: i32,
        by: i32,
        bw: u32,
        bh: u32,
        grid: Option<(u32, u32)>,
    ) -> Result<(i32, i32), String> {
        cell_to_screen_coords(chain, bw, bh, bx, by, bw, bh, grid)
    }

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
    fn test_parse_grid_uppercase() {
        assert_eq!(parse_grid("4X3").unwrap(), (4, 3));
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
        assert_eq!(auto_grid(1920, 1080), (16, 9));
        assert_eq!(auto_grid(1280, 800), (16, 9));
        assert_eq!(auto_grid(640, 480), (16, 9));
        assert_eq!(auto_grid(640, 400), (16, 9));
        assert_eq!(auto_grid(320, 240), (8, 6));
        assert_eq!(auto_grid(160, 133), (4, 3));
        assert_eq!(auto_grid(80, 80), (3, 3));
    }

    #[test]
    fn test_cell_to_coords_auto_grid() {
        // 1280x800 → auto_grid = (16, 9), cell = 80x88 (truncated)
        // B2 = col 1, row 1 → center at (120, 132)
        let (x, y) = cell_to_coords("B2", 0, 0, 1280, 800, None).unwrap();
        assert_eq!(x, 120);
        assert_eq!(y, 132);
    }

    #[test]
    fn test_parse_between_ref() {
        let ((c1, r1), (c2, r2)) = parse_between_ref("D3+E3").unwrap();
        assert_eq!((c1, r1), (3, 2));
        assert_eq!((c2, r2), (4, 2));
    }

    #[test]
    fn test_parse_between_ref_vertical() {
        let ((c1, r1), (c2, r2)) = parse_between_ref("D3+D4").unwrap();
        assert_eq!((c1, r1), (3, 2));
        assert_eq!((c2, r2), (3, 3));
    }

    #[test]
    fn test_parse_between_ref_diagonal() {
        let ((c1, r1), (c2, r2)) = parse_between_ref("D3+E4").unwrap();
        assert_eq!((c1, r1), (3, 2));
        assert_eq!((c2, r2), (4, 3));
    }

    #[test]
    fn test_parse_between_ref_non_adjacent() {
        assert!(parse_between_ref("A1+C3").is_err());
        assert!(parse_between_ref("A1+A1").is_err());
    }

    #[test]
    fn test_cell_to_coords_between() {
        // D3+E3 on 400x300 with 4x3 grid: cells are 100x100.
        // D=col3, E=col4 — wait, 4x3 grid only has cols 0-3 (A-D).
        // Use 8x6 grid on 800x600: cells are 100x100.
        // D3+E3: col1=3,col2=4, row1=2,row2=2
        // x = (3+4)/2 * 100 = 350, y = (2+2)/2 * 100 = 200
        // center: (350+50, 200+50) = (400, 250)
        let (x, y) = cell_to_coords("D3+E3", 0, 0, 800, 600, Some((8, 6))).unwrap();
        assert_eq!(x, 400);
        assert_eq!(y, 250);
    }

    #[test]
    fn test_cell_to_coords_between_vertical() {
        // D3+D4: col1=3,col2=3, row1=2,row2=3
        // x = (3+3)/2 * 100 = 300, y = (2+3)/2 * 100 = 250
        // center: (300+50, 250+50) = (350, 300)
        let (x, y) = cell_to_coords("D3+D4", 0, 0, 800, 600, Some((8, 6))).unwrap();
        assert_eq!(x, 350);
        assert_eq!(y, 300);
    }

    #[test]
    fn test_cell_to_screen_coords_identity_matches_bounds_space() {
        // When the screenshot has the same dimensions as the window bounds,
        // image-space targeting must equal the bounds-space computation.
        let via_image = cell_to_screen_coords("B2", 400, 300, 100, 50, 400, 300, Some((4, 3))).unwrap();
        let via_bounds = cell_to_coords("B2", 100, 50, 400, 300, Some((4, 3))).unwrap();
        assert_eq!(via_image, via_bounds);
        assert_eq!(via_image, (250, 200));
    }

    #[test]
    fn test_cell_to_screen_coords_retina_2x() {
        // Screenshot at 2x backing scale (800x600 image for a 400x300-pt
        // window). The cell center must land on the same screen point as the
        // 1x case — the grid is computed on the image, then mapped to bounds.
        let (x, y) = cell_to_screen_coords("B2", 800, 600, 100, 50, 400, 300, Some((4, 3))).unwrap();
        assert_eq!((x, y), (250, 200));
    }

    #[test]
    fn test_cell_to_screen_coords_density_comes_from_image() {
        // Auto density must come from the image the agent looked at
        // (auto_grid(800,600) = (16,9)), not from the bounds
        // (auto_grid(400,300) = (10,7)) — otherwise cell labels name
        // different regions in the screenshot and the click.
        // Image space: cell = 50x66; B2 center = (75, 99).
        // Mapped to bounds at origin: (75 * 400/800, 99 * 300/600) = (37, 49).
        let (x, y) = cell_to_screen_coords("B2", 800, 600, 0, 0, 400, 300, None).unwrap();
        assert_eq!((x, y), (37, 49));
    }

    #[test]
    fn test_zoom_chain_too_deep_errors() {
        // By the 4th level the cell size reaches zero. Both the screenshot
        // crop and the click must report "too deep" — previously the
        // screenshot errored opaquely while the click silently fired at a
        // degenerate point.
        let result = cell_to_screen_coords("A1.A1.A1.A1", 1280, 800, 0, 0, 1280, 800, None);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("deep"), "error should say the chain is too deep");
    }

    #[test]
    fn test_between_cell_midpoint_matches_crop_math() {
        // Odd cell width: integer midpoint math must match the crop loop
        // (proportional spans), not keep the exact .5.
        // 810x600, 8x6 grid. Spans over width 810/8: col 3 -> (303, 405),
        // col 4 -> (405, 506). span_w = 405-303 = 102.
        // mid_x = (303+405)/2 = 354 (floored). Center: 354 + 102/2 = 405.
        let (x, _) = cell_to_screen_coords("D3+E3", 810, 600, 0, 0, 810, 600, Some((8, 6))).unwrap();
        assert_eq!(x, 405);
    }

    #[test]
    fn test_cell_to_coords_recursive_auto_grid_uses_scaled_density() {
        // A1 on 1280x800 (auto 16x9) → 80x88 region.
        // At level 1, scaled up 8x to 640x704 → auto_grid_zoom (8, 6).
        // C1 in that 8x6 sub-grid → center at (25, 7).
        // Without scale-up simulation, auto_grid_zoom(80,88) = (3,3),
        // and C1 would target (66, 14) — the wrong spot.
        let (x, y) = cell_to_coords("A1.C1", 0, 0, 1280, 800, None).unwrap();
        assert_eq!(x, 25);
        assert_eq!(y, 7);
    }
}
