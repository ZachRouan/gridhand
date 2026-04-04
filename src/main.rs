mod json;
mod platform;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("{}", json::error("Usage: gui-tool <command> [args...]"));
        std::process::exit(1);
    }

    let result = match args[1].as_str() {
        "screenshot" => cmd_screenshot(&args[2..]),
        "windows" => cmd_windows(&args[2..]),
        "mouse" => cmd_mouse(&args[2..]),
        "key" => cmd_key(&args[2..]),
        _ => Err(format!("Unknown command: {}", args[1])),
    };

    match result {
        Ok(output) => println!("{}", output),
        Err(e) => {
            eprintln!("{}", json::error(&e));
            std::process::exit(1);
        }
    }
}

/// Pre-parse args to extract --window and --window-id flags.
/// Returns (remaining_args, Option<(id, x, y, w, h)>).
/// If a window flag is present, raises the window and sleeps for focus.
fn extract_window_flags(args: &[String]) -> Result<(Vec<String>, Option<(u64, i32, i32, u32, u32)>), String> {
    let mut remaining = Vec::new();
    let mut window_title: Option<String> = None;
    let mut window_id: Option<u64> = None;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--window" => {
                i += 1;
                window_title = Some(
                    args.get(i)
                        .ok_or("--window requires a title argument")?
                        .clone(),
                );
            }
            "--window-id" => {
                i += 1;
                let id_str = args.get(i)
                    .ok_or("--window-id requires a numeric ID argument")?;
                window_id = Some(
                    id_str.parse::<u64>()
                        .map_err(|_| format!("Invalid window ID: {}", id_str))?,
                );
            }
            _ => {
                remaining.push(args[i].clone());
            }
        }
        i += 1;
    }

    if window_title.is_some() && window_id.is_some() {
        return Err("Cannot use both --window and --window-id".to_string());
    }

    let resolved_id = if let Some(title) = &window_title {
        let (id, _) = platform::find_window_by_title(title)?
            .ok_or_else(|| format!("No window found matching '{}'", title))?;
        Some(id)
    } else {
        window_id
    };

    if let Some(id) = resolved_id {
        platform::raise_window(id)?;
        std::thread::sleep(std::time::Duration::from_millis(200));
        let (x, y, w, h) = platform::get_window_bounds(id)?;
        Ok((remaining, Some((id, x, y, w, h))))
    } else {
        Ok((remaining, None))
    }
}

/// Parse a grid density string like "8x6" into (cols, rows).
fn parse_grid(s: &str) -> Result<(u32, u32), String> {
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
fn parse_cell_ref(s: &str) -> Result<(u32, u32), String> {
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
    Ok((col, row - 1)) // zero-indexed
}

/// Compute absolute screen coordinates from a cell chain like "B2.C1".
/// Uses f64 throughout to avoid integer division drift.
/// Returns the center point of the innermost cell.
fn cell_to_coords(
    cell_chain: &str,
    bounds_x: i32,
    bounds_y: i32,
    bounds_w: u32,
    bounds_h: u32,
    grid_cols: u32,
    grid_rows: u32,
) -> Result<(i32, i32), String> {
    let mut x = bounds_x as f64;
    let mut y = bounds_y as f64;
    let mut w = bounds_w as f64;
    let mut h = bounds_h as f64;

    for part in cell_chain.split('.') {
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

    // Return center of the final cell
    Ok(((x + w / 2.0) as i32, (y + h / 2.0) as i32))
}

fn cmd_screenshot(args: &[String]) -> Result<String, String> {
    let mut output_path: Option<String> = None;
    let mut window_title: Option<String> = None;
    let mut window_id: Option<u64> = None;
    let mut grid: Option<(u32, u32)> = None;
    let mut cell: Option<String> = None;
    let mut i = 0;

    while i < args.len() {
        match args[i].as_str() {
            "--window" => {
                i += 1;
                window_title = Some(
                    args.get(i).ok_or("--window requires a title argument")?.clone(),
                );
            }
            "--window-id" => {
                i += 1;
                let id_str = args.get(i).ok_or("--window-id requires a numeric ID argument")?;
                window_id = Some(id_str.parse::<u64>().map_err(|_| format!("Invalid window ID: {}", id_str))?);
            }
            "--output" => {
                i += 1;
                output_path = Some(
                    args.get(i).ok_or("--output requires a path argument")?.clone(),
                );
            }
            "--grid" => {
                // Check if next arg is a WxH value or another flag
                if let Some(next) = args.get(i + 1) {
                    if !next.starts_with('-') && next.contains('x') {
                        grid = Some(parse_grid(next)?);
                        i += 1;
                    } else {
                        grid = Some((8, 6)); // default
                    }
                } else {
                    grid = Some((8, 6)); // default
                }
            }
            "--cell" => {
                i += 1;
                cell = Some(args.get(i).ok_or("--cell requires a cell reference (e.g., B2)")?.clone());
            }
            _ => return Err(format!("Unknown flag: {}", args[i])),
        }
        i += 1;
    }

    if window_title.is_some() && window_id.is_some() {
        return Err("Cannot use both --window and --window-id".to_string());
    }

    let output = output_path.as_deref().unwrap_or("/tmp/gui-tool-screenshot.png");

    // Take the screenshot
    let result = if let Some(title) = &window_title {
        platform::screenshot_window(title, output)?
    } else if let Some(id) = window_id {
        platform::screenshot_window_by_id(id, output)?
    } else {
        platform::screenshot_full(output)?
    };

    // Post-process: apply cell crop and/or grid overlay
    if cell.is_some() || grid.is_some() {
        let mut img = platform::png::read_png(output)?;
        let (cols, rows) = grid.unwrap_or((8, 6));

        // If --cell is specified, recursively crop through dot-separated refs
        if let Some(cell_chain) = &cell {
            for part in cell_chain.split('.') {
                let (col, row) = parse_cell_ref(part)?;
                if col >= cols || row >= rows {
                    return Err(format!("Cell '{}' out of range for {}x{} grid", part, cols, rows));
                }
                let cell_w = img.width / cols;
                let cell_h = img.height / rows;
                let cx = col * cell_w;
                let cy = row * cell_h;
                img = platform::png::crop(&img, cx, cy, cell_w, cell_h)?;
            }
        }

        // Draw grid overlay on the final (possibly cropped) image
        platform::png::draw_grid(&mut img, cols, rows);

        platform::png::write_png(output, &img)?;
    }

    // Print the original JSON result (path, bounds, etc.)
    Ok(result)
}

fn cmd_windows(args: &[String]) -> Result<String, String> {
    if args.is_empty() {
        return Err("Usage: gui-tool windows <list|raise> [args...]".to_string());
    }

    match args[0].as_str() {
        "list" => platform::list_windows(),
        "raise" => {
            let id: u64 = args.get(1)
                .ok_or("Usage: gui-tool windows raise <id>")?
                .parse()
                .map_err(|_| "Invalid window ID")?;
            platform::raise_window(id)
        }
        _ => Err(format!("Unknown windows subcommand: {}", args[0])),
    }
}

fn cmd_mouse(args: &[String]) -> Result<String, String> {
    if args.is_empty() {
        return Err("Usage: gui-tool mouse <move|click> [args...]".to_string());
    }

    let subcmd = args[0].as_str();
    let sub_args = &args[1..];

    // Extract window flags (also extracts --grid and --cell from remaining)
    let (remaining, window_info) = extract_window_flags(sub_args)?;

    // Extract --cell and --grid from remaining args
    let mut positional = Vec::new();
    let mut cell: Option<String> = None;
    let mut grid_density: (u32, u32) = (8, 6);
    let mut i = 0;
    while i < remaining.len() {
        match remaining[i].as_str() {
            "--cell" => {
                i += 1;
                cell = Some(remaining.get(i).ok_or("--cell requires a cell reference")?.clone());
            }
            "--grid" => {
                if let Some(next) = remaining.get(i + 1) {
                    if !next.starts_with('-') && next.contains('x') {
                        grid_density = parse_grid(next)?;
                        i += 1;
                    }
                }
            }
            "--button" => {
                positional.push(remaining[i].clone());
                i += 1;
                if let Some(val) = remaining.get(i) {
                    positional.push(val.clone());
                }
            }
            _ => {
                positional.push(remaining[i].clone());
            }
        }
        i += 1;
    }

    match subcmd {
        "move" => {
            if let Some(cell_ref) = &cell {
                // Cell-based targeting — requires window bounds
                let (_, wx, wy, ww, wh) = window_info
                    .ok_or("--cell requires --window or --window-id to know the target window")?;
                let (x, y) = cell_to_coords(cell_ref, wx, wy, ww, wh, grid_density.0, grid_density.1)?;
                platform::mouse_move(x, y)
            } else {
                // Coordinate-based targeting
                let mut x: i32 = positional.get(0)
                    .ok_or("Usage: gui-tool mouse move <x> <y> or --cell <ref>")?
                    .parse().map_err(|_| "Invalid x coordinate")?;
                let mut y: i32 = positional.get(1)
                    .ok_or("Usage: gui-tool mouse move <x> <y>")?
                    .parse().map_err(|_| "Invalid y coordinate")?;

                if let Some((_, wx, wy, _, _)) = window_info {
                    x += wx;
                    y += wy;
                }
                platform::mouse_move(x, y)
            }
        }
        "click" => {
            let mut button = "left";
            let mut j = 0;
            while j < positional.len() {
                match positional[j].as_str() {
                    "--button" => {
                        j += 1;
                        if let Some(b) = positional.get(j) {
                            button = b.as_str();
                        }
                    }
                    other => {
                        button = other;
                    }
                }
                j += 1;
            }
            platform::mouse_click(button)
        }
        _ => Err(format!("Unknown mouse subcommand: {}", subcmd)),
    }
}

fn cmd_key(args: &[String]) -> Result<String, String> {
    if args.is_empty() {
        return Err("Usage: gui-tool key <type|press> [args...]".to_string());
    }

    let subcmd = args[0].as_str();
    let sub_args = &args[1..];

    // Extract window flags from the remaining args
    let (remaining, _window_info) = extract_window_flags(sub_args)?;

    match subcmd {
        "type" => {
            let text = remaining.get(0)
                .ok_or("Usage: gui-tool key type <text>")?;
            platform::key_type(text)
        }
        "press" => {
            let combo = remaining.get(0)
                .ok_or("Usage: gui-tool key press <combo>")?;
            platform::key_press(combo)
        }
        _ => Err(format!("Unknown key subcommand: {}", subcmd)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Test that extract_window_flags can't be tested directly without a running
    // desktop (it calls platform::raise_window), so we test the parsing logic
    // by checking the error cases that don't require platform calls.

    #[test]
    fn test_both_window_flags_error() {
        let args: Vec<String> = vec![
            "--window".to_string(), "Firefox".to_string(),
            "--window-id".to_string(), "123".to_string(),
        ];
        let result = extract_window_flags(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Cannot use both"));
    }

    #[test]
    fn test_window_id_missing_value() {
        let args: Vec<String> = vec!["--window-id".to_string()];
        let result = extract_window_flags(&args);
        assert!(result.is_err());
    }

    #[test]
    fn test_window_id_invalid_number() {
        let args: Vec<String> = vec!["--window-id".to_string(), "notanumber".to_string()];
        let result = extract_window_flags(&args);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid window ID"));
    }

    #[test]
    fn test_window_missing_value() {
        let args: Vec<String> = vec!["--window".to_string()];
        let result = extract_window_flags(&args);
        assert!(result.is_err());
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
    fn test_parse_grid_invalid() {
        assert!(parse_grid("abc").is_err());
        assert!(parse_grid("0x0").is_err());
        assert!(parse_grid("27x3").is_err()); // > 26 cols
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
        assert!(parse_cell_ref("A0").is_err()); // row 0 invalid
    }

    #[test]
    fn test_cell_to_coords_single() {
        // 4x3 grid on a 400x300 window at (100, 50)
        // Cell B2 = col 1, row 1 → cell is at (200, 150) size (100, 100) → center (250, 200)
        let (x, y) = cell_to_coords("B2", 100, 50, 400, 300, 4, 3).unwrap();
        assert_eq!(x, 250);
        assert_eq!(y, 200);
    }

    #[test]
    fn test_cell_to_coords_recursive() {
        // 4x3 grid on 400x300 at (0, 0)
        // B2 = (100, 100) size (100, 100)
        // B2.A1 = within B2: (100, 100) size (25, 33) → center (112, 116)
        let (x, y) = cell_to_coords("B2.A1", 0, 0, 400, 300, 4, 3).unwrap();
        assert_eq!(x, 112);
        assert_eq!(y, 116);
    }

    #[test]
    fn test_cell_to_coords_out_of_range() {
        assert!(cell_to_coords("E1", 0, 0, 400, 300, 4, 3).is_err()); // col 4 >= 4
        assert!(cell_to_coords("A4", 0, 0, 400, 300, 4, 3).is_err()); // row 3 >= 3
    }
}
