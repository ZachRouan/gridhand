mod json;

#[cfg(target_os = "linux")]
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

fn cmd_screenshot(args: &[String]) -> Result<String, String> {
    let mut window_title: Option<&str> = None;
    let mut output_path: Option<&str> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--window" => {
                i += 1;
                window_title = args.get(i).map(|s| s.as_str());
            }
            "--output" => {
                i += 1;
                output_path = args.get(i).map(|s| s.as_str());
            }
            _ => return Err(format!("Unknown flag: {}", args[i])),
        }
        i += 1;
    }

    let output = output_path.unwrap_or("/tmp/gui-tool-screenshot.png");

    #[cfg(target_os = "linux")]
    {
        if let Some(title) = window_title {
            platform::screenshot_window(title, output)
        } else {
            platform::screenshot_full(output)
        }
    }

    #[cfg(not(target_os = "linux"))]
    Err("Platform not supported yet".to_string())
}

fn cmd_windows(args: &[String]) -> Result<String, String> {
    if args.is_empty() {
        return Err("Usage: gui-tool windows <list|raise> [args...]".to_string());
    }

    match args[0].as_str() {
        "list" => {
            #[cfg(target_os = "linux")]
            { platform::list_windows() }
            #[cfg(not(target_os = "linux"))]
            Err("Platform not supported yet".to_string())
        }
        "raise" => {
            let id: u32 = args.get(1)
                .ok_or("Usage: gui-tool windows raise <id>")?
                .parse()
                .map_err(|_| "Invalid window ID")?;
            #[cfg(target_os = "linux")]
            { platform::raise_window(id) }
            #[cfg(not(target_os = "linux"))]
            Err("Platform not supported yet".to_string())
        }
        _ => Err(format!("Unknown windows subcommand: {}", args[0])),
    }
}

fn cmd_mouse(args: &[String]) -> Result<String, String> {
    if args.is_empty() {
        return Err("Usage: gui-tool mouse <move|click> [args...]".to_string());
    }

    match args[0].as_str() {
        "move" => {
            let x: i32 = args.get(1)
                .ok_or("Usage: gui-tool mouse move <x> <y>")?
                .parse().map_err(|_| "Invalid x coordinate")?;
            let y: i32 = args.get(2)
                .ok_or("Usage: gui-tool mouse move <x> <y>")?
                .parse().map_err(|_| "Invalid y coordinate")?;
            #[cfg(target_os = "linux")]
            { platform::mouse_move(x, y) }
            #[cfg(not(target_os = "linux"))]
            Err("Platform not supported yet".to_string())
        }
        "click" => {
            let mut button = "left";
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--button" => {
                        i += 1;
                        button = args.get(i).map(|s| s.as_str())
                            .unwrap_or("left");
                    }
                    other => {
                        // Also accept positional for convenience
                        button = other;
                    }
                }
                i += 1;
            }
            #[cfg(target_os = "linux")]
            { platform::mouse_click(button) }
            #[cfg(not(target_os = "linux"))]
            Err("Platform not supported yet".to_string())
        }
        _ => Err(format!("Unknown mouse subcommand: {}", args[0])),
    }
}

fn cmd_key(args: &[String]) -> Result<String, String> {
    if args.is_empty() {
        return Err("Usage: gui-tool key <type|press> [args...]".to_string());
    }

    match args[0].as_str() {
        "type" => {
            let text = args.get(1).ok_or("Usage: gui-tool key type <text>")?;
            #[cfg(target_os = "linux")]
            { platform::key_type(text) }
            #[cfg(not(target_os = "linux"))]
            Err("Platform not supported yet".to_string())
        }
        "press" => {
            let combo = args.get(1).ok_or("Usage: gui-tool key press <combo>")?;
            #[cfg(target_os = "linux")]
            { platform::key_press(combo) }
            #[cfg(not(target_os = "linux"))]
            Err("Platform not supported yet".to_string())
        }
        _ => Err(format!("Unknown key subcommand: {}", args[0])),
    }
}
