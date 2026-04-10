mod json;
mod platform;

/// Minimum dimensions for zoomed crop scale-up.
/// Ensures cropped regions are large enough for vision models to read.
const ZOOM_MIN_WIDTH: u32 = 640;
const ZOOM_MIN_HEIGHT: u32 = 480;

/// Maximum age of cache in seconds before a fresh screenshot is taken.
/// Generous timeout — cache is also invalidated by mouse/key actions.
const CACHE_MAX_AGE_SECS: u64 = 60;

/// Returns a per-user cache path for screenshots.
/// Uses XDG_RUNTIME_DIR (per-user, mode 0700, tmpfs) when available,
/// falls back to ~/.cache/gui-tool/, then to the system temp dir.
fn cache_path() -> String {
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        return format!("{}/gui-tool-screenshot-cache.png", dir);
    }
    if let Ok(home) = std::env::var("HOME") {
        let dir = format!("{}/.cache/gui-tool", home);
        let _ = std::fs::create_dir_all(&dir);
        return format!("{}/screenshot-cache.png", dir);
    }
    // macOS/Windows: temp_dir is already per-user
    let tmp = std::env::temp_dir();
    format!("{}/gui-tool-screenshot-cache.png", tmp.display())
}

const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "\
gui-tool — programmatic GUI interaction for AI agents

USAGE:
    gui-tool <command> [options]

COMMANDS:
    screenshot [options]            Take a screenshot
        --window <title>            Screenshot a specific window (by title substring)
        --window-id <id>            Screenshot a specific window (by numeric ID)
        --grid [WxH]                Overlay a labeled grid (default: auto-scaled)
        --cell <ref>                Crop to a grid cell (supports zoom: B2.C1)
        --output <path>             Output file path

    windows list                    List all open windows
    windows raise <id>              Raise a window by ID

    mouse move <x> <y> [options]    Move mouse to absolute coordinates
        --window <title>            Move relative to window (by title)
        --window-id <id>            Move relative to window (by ID)
        --cell <ref> --window-id <id>  Move to grid cell center
        --grid WxH                  Grid dimensions for cell targeting

    mouse click [options]           Click at current position
        --button left|right         Button to click (default: left)
        --window <title>            Raise window before clicking
        --window-id <id>            Raise window before clicking

    key type <text> [options]       Type text string
        --window <title>            Raise window before typing
        --window-id <id>            Raise window before typing

    key press <combo> [options]     Press key combination (e.g. ctrl+c)
        --window <title>            Raise window before pressing
        --window-id <id>            Raise window before pressing

OPTIONS:
    --help                          Show this help message
    --version                       Show version

OUTPUT:
    All output is JSON to stdout. Errors are JSON to stderr.";

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.len() < 2 {
        eprintln!("{}", json::error("Usage: gui-tool <command> [args...]. Try 'gui-tool --help'"));
        std::process::exit(1);
    }

    let result = match args[1].as_str() {
        "--help" | "-h" | "help" => {
            println!("{}", HELP);
            std::process::exit(0);
        }
        "--version" | "-V" => {
            println!("gui-tool {}", VERSION);
            std::process::exit(0);
        }
        "screenshot" => cmd_screenshot(&args[2..]),
        "windows" => cmd_windows(&args[2..]),
        "mouse" => cmd_mouse(&args[2..]),
        "key" => cmd_key(&args[2..]),
        _ => Err(format!("Unknown command: {}. Try 'gui-tool --help'", args[1])),
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

/// Invalidate the screenshot cache (called after actions that change screen state).
fn invalidate_cache() {
    let _ = std::fs::remove_file(cache_path());
}

/// Check if the screenshot cache file exists and is recent enough to reuse.
fn cache_is_fresh() -> bool {
    if let Ok(meta) = std::fs::metadata(&cache_path()) {
        if let Ok(modified) = meta.modified() {
            if let Ok(elapsed) = modified.elapsed() {
                return elapsed.as_secs() < CACHE_MAX_AGE_SECS;
            }
        }
    }
    false
}

/// Auto-select grid density based on image dimensions.
/// Larger images get denser grids; smaller (zoomed) images get coarser grids.
/// Grid cells target ~40px minimum to keep labels readable while maximizing precision.
fn auto_grid(width: u32, height: u32) -> (u32, u32) {
    let max_cols = (width / 40).max(3).min(8);
    let max_rows = (height / 40).max(3).min(6);
    (max_cols, max_rows)
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
/// Auto-scales grid density at each recursion level based on region size,
/// simulating the same scale-up that screenshot zoom applies (min 640x480)
/// so that grid densities match between screenshot and mouse move.
/// If `explicit_grid` is Some, uses that fixed density instead of auto-scaling.
/// Returns the center point of the innermost cell.
fn cell_to_coords(
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
        // For sub-cells (not the first level), simulate the scale-up that
        // screenshot applies to cropped regions before computing auto_grid.
        // This ensures grid density matches between screenshot and mouse move.
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

    // Return center of the final cell
    Ok(((x + w / 2.0) as i32, (y + h / 2.0) as i32))
}

fn cmd_screenshot(args: &[String]) -> Result<String, String> {
    let mut output_path: Option<String> = None;
    let mut window_title: Option<String> = None;
    let mut window_id: Option<u64> = None;
    let mut grid_enabled = false;
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
                grid_enabled = true;
                // Check if next arg is an explicit WxH value
                if let Some(next) = args.get(i + 1) {
                    if !next.starts_with('-') && next.contains('x') {
                        grid = Some(parse_grid(next)?);
                        i += 1;
                    }
                    // else: no explicit value, auto-scale will be used
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

    // When zooming with --cell, try to reuse cached screenshot instead of taking a new one
    let use_cache = cell.is_some() && cache_is_fresh();

    let result = if use_cache {
        // Reuse cached screenshot — no new screenshot needed
        json::success_with(vec![("path", json::JsonValue::Str(output))])
    } else if let Some(title) = &window_title {
        let r = platform::screenshot_window(title, output)?;
        let _ = std::fs::copy(output, &cache_path());
        r
    } else if let Some(id) = window_id {
        let r = platform::screenshot_window_by_id(id, output)?;
        let _ = std::fs::copy(output, &cache_path());
        r
    } else {
        let r = platform::screenshot_full(output)?;
        let _ = std::fs::copy(output, &cache_path());
        r
    };

    // Post-process: apply cell crop and/or grid overlay
    if cell.is_some() || grid_enabled {
        // Read from cache if available, otherwise from the output
        let source = if use_cache { &cache_path() } else { output };
        let mut img = platform::png::read_png(source)?;

        // If --cell is specified, recursively crop through dot-separated refs
        if let Some(cell_chain) = &cell {
            for part in cell_chain.split('.') {
                // Auto-scale or use explicit grid for this level
                let (cols, rows) = grid.unwrap_or_else(|| auto_grid(img.width, img.height));
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

        // Scale up small crops so content is readable in vision models
        if cell.is_some() {
            img = platform::png::scale_up(&img, ZOOM_MIN_WIDTH, ZOOM_MIN_HEIGHT);
        }

        // Draw grid overlay on the final (possibly cropped and scaled) image
        let (final_cols, final_rows) = grid.unwrap_or_else(|| auto_grid(img.width, img.height));
        platform::png::draw_grid(&mut img, final_cols, final_rows);

        platform::png::write_png(output, &img)?;

        // Build clean JSON output with grid info
        let grid_info = format!("{}x{}", final_cols, final_rows);
        return Ok(json::success_with(vec![
            ("path", json::JsonValue::Str(output)),
            ("grid", json::JsonValue::OwnedStr(grid_info)),
        ]));
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
    let mut explicit_grid: Option<(u32, u32)> = None;
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
                        explicit_grid = Some(parse_grid(next)?);
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
                let (x, y) = cell_to_coords(cell_ref, wx, wy, ww, wh, explicit_grid)?;
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
            let result = platform::mouse_click(button);
            invalidate_cache();
            result
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
            let result = platform::key_type(text);
            invalidate_cache();
            result
        }
        "press" => {
            let combo = remaining.get(0)
                .ok_or("Usage: gui-tool key press <combo>")?;
            let result = platform::key_press(combo);
            invalidate_cache();
            result
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
        // Explicit 4x3 grid on a 400x300 window at (100, 50)
        // Cell B2 = col 1, row 1 → cell is at (200, 150) size (100, 100) → center (250, 200)
        let (x, y) = cell_to_coords("B2", 100, 50, 400, 300, Some((4, 3))).unwrap();
        assert_eq!(x, 250);
        assert_eq!(y, 200);
    }

    #[test]
    fn test_cell_to_coords_recursive() {
        // Explicit 4x3 grid on 400x300 at (0, 0)
        // B2 = (100, 100) size (100, 100)
        // B2.A1 = within B2: (100, 100) size (25, 33) → center (112, 116)
        let (x, y) = cell_to_coords("B2.A1", 0, 0, 400, 300, Some((4, 3))).unwrap();
        assert_eq!(x, 112);
        assert_eq!(y, 116);
    }

    #[test]
    fn test_cell_to_coords_out_of_range() {
        assert!(cell_to_coords("E1", 0, 0, 400, 300, Some((4, 3))).is_err()); // col 4 >= 4
        assert!(cell_to_coords("A4", 0, 0, 400, 300, Some((4, 3))).is_err()); // row 3 >= 3
    }

    #[test]
    fn test_auto_grid() {
        // Full window: 1280/40=32 clamped to 8, 800/40=20 clamped to 6
        assert_eq!(auto_grid(1280, 800), (8, 6));
        // Zoomed cell: 160/40=4, 133/40=3
        assert_eq!(auto_grid(160, 133), (4, 3));
        // Medium: 640/40=16 clamped to 8, 400/40=10 clamped to 6
        assert_eq!(auto_grid(640, 400), (8, 6));
        // Small: 80/40=2 clamped to 3, 80/40=2 clamped to 3
        assert_eq!(auto_grid(80, 80), (3, 3));
    }

    #[test]
    fn test_cell_to_coords_auto_grid() {
        // Auto-scale: 1280x800 → 8x6, cell B2 = col 1, row 1
        // cell_w = 160.0, cell_h = 133.33, center = (160 + 80, 133.33 + 66.67) = (240, 200)
        let (x, y) = cell_to_coords("B2", 0, 0, 1280, 800, None).unwrap();
        assert_eq!(x, 240);
        assert_eq!(y, 200);
    }
}
