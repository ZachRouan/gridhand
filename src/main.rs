mod grid;
mod json;
mod platform;
mod validate;

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
#[allow(clippy::type_complexity)]
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
    if let Ok(meta) = std::fs::metadata(cache_path())
        && let Ok(modified) = meta.modified()
            && let Ok(elapsed) = modified.elapsed() {
                return elapsed.as_secs() < CACHE_MAX_AGE_SECS;
            }
    false
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
                if let Some(next) = args.get(i + 1)
                    && !next.starts_with('-') && next.contains('x') {
                        grid = Some(grid::parse_grid(next)?);
                        i += 1;
                    }
                    // else: no explicit value, auto-scale will be used
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
    validate::output_path(output)?;

    // When zooming with --cell, try to reuse cached screenshot instead of taking a new one
    let use_cache = cell.is_some() && cache_is_fresh();

    let result = if use_cache {
        // Reuse cached screenshot — no new screenshot needed
        json::success_with(vec![("path", json::JsonValue::Str(output))])
    } else if let Some(title) = &window_title {
        let r = platform::screenshot_window(title, output)?;
        let _ = std::fs::copy(output, cache_path());
        r
    } else if let Some(id) = window_id {
        let r = platform::screenshot_window_by_id(id, output)?;
        let _ = std::fs::copy(output, cache_path());
        r
    } else {
        let r = platform::screenshot_full(output)?;
        let _ = std::fs::copy(output, cache_path());
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
                let (cols, rows) = grid.unwrap_or_else(|| grid::auto_grid(img.width, img.height));
                let (col, row) = grid::parse_cell_ref(part)?;
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
        let (final_cols, final_rows) = grid.unwrap_or_else(|| grid::auto_grid(img.width, img.height));
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
                if let Some(next) = remaining.get(i + 1)
                    && !next.starts_with('-') && next.contains('x') {
                        explicit_grid = Some(grid::parse_grid(next)?);
                        i += 1;
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
                let (x, y) = grid::cell_to_coords(cell_ref, wx, wy, ww, wh, explicit_grid)?;
                platform::mouse_move(x, y)
            } else {
                // Coordinate-based targeting
                let mut x: i32 = positional.first()
                    .ok_or("Usage: gui-tool mouse move <x> <y> or --cell <ref>")?
                    .parse().map_err(|_| "Invalid x coordinate")?;
                let mut y: i32 = positional.get(1)
                    .ok_or("Usage: gui-tool mouse move <x> <y>")?
                    .parse().map_err(|_| "Invalid y coordinate")?;

                if let Some((_, wx, wy, _, _)) = window_info {
                    x += wx;
                    y += wy;
                }
                validate::coordinates(x, y)?;
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
            let text = remaining.first()
                .ok_or("Usage: gui-tool key type <text>")?;
            let result = platform::key_type(text);
            invalidate_cache();
            result
        }
        "press" => {
            let combo = remaining.first()
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
    fn test_unknown_flag_error() {
        let result = cmd_screenshot(&["--bogus".to_string()]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Unknown flag"));
    }

    #[test]
    fn test_screenshot_path_traversal_blocked() {
        let result = cmd_screenshot(&["--output".to_string(), "/tmp/../etc/bad.png".to_string()]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("path traversal"));
    }

    #[test]
    fn test_screenshot_bad_extension_blocked() {
        let result = cmd_screenshot(&["--output".to_string(), "/tmp/test.jpg".to_string()]);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains(".png"));
    }

    #[test]
    fn test_json_output_has_status() {
        let success = json::success();
        assert!(success.contains("\"status\":\"success\""));
        let err = json::error("test");
        assert!(err.contains("\"status\":\"error\""));
        assert!(err.contains("\"message\":\"test\""));
    }

    #[test]
    fn test_windows_unknown_subcommand() {
        let result = cmd_windows(&["bogus".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_key_unknown_subcommand() {
        let result = cmd_key(&["bogus".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_mouse_no_args() {
        let result = cmd_mouse(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_key_no_args() {
        let result = cmd_key(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn test_windows_no_args() {
        let result = cmd_windows(&[]);
        assert!(result.is_err());
    }
}
