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
/// Returns (remaining_args, window_id_if_focused).
/// If a window flag is present, raises the window and sleeps for focus.
fn extract_window_flags(args: &[String]) -> Result<(Vec<String>, Option<u64>), String> {
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

    // Error if both flags provided
    if window_title.is_some() && window_id.is_some() {
        return Err("Cannot use both --window and --window-id".to_string());
    }

    // Resolve title to ID if needed
    let resolved_id = if let Some(title) = &window_title {
        let (id, _) = platform::find_window_by_title(title)?
            .ok_or_else(|| format!("No window found matching '{}'", title))?;
        Some(id)
    } else {
        window_id
    };

    // Raise window and wait for focus
    if let Some(id) = resolved_id {
        platform::raise_window(id)?;
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    Ok((remaining, resolved_id))
}

fn cmd_screenshot(args: &[String]) -> Result<String, String> {
    let mut output_path: Option<String> = None;
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
            "--output" => {
                i += 1;
                output_path = Some(
                    args.get(i)
                        .ok_or("--output requires a path argument")?
                        .clone(),
                );
            }
            _ => return Err(format!("Unknown flag: {}", args[i])),
        }
        i += 1;
    }

    if window_title.is_some() && window_id.is_some() {
        return Err("Cannot use both --window and --window-id".to_string());
    }

    let output = output_path
        .as_deref()
        .unwrap_or("/tmp/gui-tool-screenshot.png");

    if let Some(title) = &window_title {
        platform::screenshot_window(title, output)
    } else if let Some(id) = window_id {
        platform::screenshot_window_by_id(id, output)
    } else {
        platform::screenshot_full(output)
    }
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

    // First arg is the subcommand
    let subcmd = args[0].as_str();
    let sub_args = &args[1..];

    // Extract window flags from the remaining args
    let (remaining, _) = extract_window_flags(sub_args)?;

    match subcmd {
        "move" => {
            let x: i32 = remaining.get(0)
                .ok_or("Usage: gui-tool mouse move <x> <y>")?
                .parse().map_err(|_| "Invalid x coordinate")?;
            let y: i32 = remaining.get(1)
                .ok_or("Usage: gui-tool mouse move <x> <y>")?
                .parse().map_err(|_| "Invalid y coordinate")?;
            platform::mouse_move(x, y)
        }
        "click" => {
            let mut button = "left";
            let mut i = 0;
            while i < remaining.len() {
                match remaining[i].as_str() {
                    "--button" => {
                        i += 1;
                        if let Some(b) = remaining.get(i) {
                            button = b.as_str();
                        }
                    }
                    other => {
                        button = other;
                    }
                }
                i += 1;
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
    let (remaining, _) = extract_window_flags(sub_args)?;

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
}
