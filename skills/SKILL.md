---
name: gui-tool
description: Interact with the desktop GUI — take screenshots, list/raise windows, move/click mouse, type text, press key combos. Use when you need to see the screen, find windows, click on things, type into applications, or automate any GUI interaction. All commands return JSON.
---

# gui-tool

Use `gui-tool` to interact with the desktop (Linux, macOS, and Windows). Ensure the binary is built (`cargo build --release`) and on your PATH.

## Commands

### Screenshots

```bash
# Full screen screenshot
gui-tool screenshot --output /tmp/screenshot.png

# Screenshot with a specific window raised first
gui-tool screenshot --window "Firefox" --output /tmp/firefox.png
```

Returns: `{"status":"success","path":"/tmp/screenshot.png"}`

Window screenshots are **automatically cropped** to the window bounds and return bounds info:
```json
{"status":"success","path":"/tmp/firefox.png","window":{...},"bounds":{"x":100,"y":200,"width":800,"height":600}}
```

### Window management

```bash
# List all open windows (IDs, titles, workspace, focus state)
gui-tool windows list

# Raise a window by ID (get IDs from windows list)
gui-tool windows raise 1234567
```

### Mouse

```bash
# Move mouse to absolute screen coordinates
gui-tool mouse move 500 300

# Click (default: left)
gui-tool mouse click
gui-tool mouse click --button right
```

### Keyboard

```bash
# Type text into the focused window
gui-tool key type "hello world"

# Press key combos
gui-tool key press "ctrl+a"
gui-tool key press "alt+f4"
gui-tool key press "super"
gui-tool key press "ctrl+shift+t"
```

Supported modifiers: ctrl, shift, alt, super/meta
Supported keys: a-z, 0-9, f1-f12, enter, tab, space, backspace, delete, escape, up, down, left, right, home, end, pageup, pagedown

## Output format

All commands return JSON to stdout on success:
```json
{"status":"success", ...}
```

Errors go to stderr as JSON:
```json
{"status":"error","message":"..."}
```

## Common patterns

**See what's on screen:**
```bash
gui-tool screenshot --output /tmp/screen.png
```
Then read the screenshot image to see the desktop.

**See a specific window (cropped):**
```bash
gui-tool screenshot --window "Firefox" --output /tmp/firefox.png
```
The PNG is cropped to just that window — no need to crop yourself.

**Find and focus a window:**
```bash
gui-tool windows list                    # find the ID
gui-tool windows raise <id>              # bring it to front
```

**Focus a window and interact (no race condition):**
```bash
gui-tool mouse click --window "Firefox"
gui-tool key type "search query" --window-id 2045481940
gui-tool screenshot --window-id 2045481940 --output /tmp/app.png
```
Use `--window` for title matching or `--window-id` for exact ID (from `windows list`).

**Click a specific location:**
```bash
gui-tool mouse move 500 300
gui-tool mouse click
```

**Select all and copy from focused app:**
```bash
gui-tool key press "ctrl+a"
gui-tool key press "ctrl+c"
```

## Requirements

- Linux with GNOME/Wayland, macOS 10.15+, or Windows 8+
- On Linux: user must be in `input` group, `window-calls@domandoman.xyz` extension enabled. Run `setup.sh` if not set up.
- On macOS: grant Accessibility and Screen Recording permissions to the binary in System Settings
- On Windows: no special setup required
