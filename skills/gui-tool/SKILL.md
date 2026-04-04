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

# Move mouse relative to a window's top-left corner
gui-tool mouse move 100 200 --window-id 2045481940

# Click (default: left)
gui-tool mouse click
gui-tool mouse click --button right
```

When `--window` or `--window-id` is used with `mouse move`, coordinates are **relative to the window's top-left corner**, not the screen. This eliminates manual offset math.

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

## Coordinate workflow for agents

When you need to click something inside a window, follow this sequence:

### 1. Get the window ID
```bash
gui-tool windows list
```
Parse the JSON to find the target window's `id` field.

### 2. Take a cropped screenshot
```bash
gui-tool screenshot --window-id <id> --output /tmp/target.png
```
This gives you an image of **just that window**, cropped to its bounds. Analyze this image to find your click target.

### 3. Click using window-relative coordinates
```bash
gui-tool mouse move <x> <y> --window-id <id>
gui-tool mouse click --window-id <id>
```
When `--window-id` is used with `mouse move`, coordinates are **relative to the window's top-left corner**. So if a button appears at pixel (200, 150) in the cropped screenshot, use those exact values — no screen offset math needed.

### Key rules
- **Always use `--window-id`** for multi-step interactions. It prevents focus race conditions by raising the window in the same process.
- **Coordinates with `--window-id` are window-relative.** The screenshot you took is already cropped to the window, so pixel positions in the image map directly to mouse coordinates.
- **Coordinates without `--window-id` are absolute screen positions.** Only use this if you're working with the full desktop.
- **Prefer `--window-id` over `--window`** when you have the ID. Title matching (`--window`) is fuzzy and may grab the wrong window if multiple have similar titles.

## Common patterns

**See what's on screen:**
```bash
gui-tool screenshot --output /tmp/screen.png
```
Then read the screenshot image to see the desktop.

**See a specific window (cropped):**
```bash
gui-tool screenshot --window-id 2045481940 --output /tmp/app.png
```

**Focus and interact (no race condition):**
```bash
gui-tool mouse move 200 150 --window-id 2045481940
gui-tool mouse click --window-id 2045481940
gui-tool key type "hello" --window-id 2045481940
```

**Select all and copy from a specific window:**
```bash
gui-tool key press "ctrl+a" --window-id 2045481940
gui-tool key press "ctrl+c" --window-id 2045481940
```

## Requirements

- Linux with GNOME/Wayland, macOS 10.15+, or Windows 8+
- On Linux: user must be in `input` group, `window-calls@domandoman.xyz` extension enabled. Run `setup.sh` if not set up.
- On macOS: grant Accessibility and Screen Recording permissions to the binary in System Settings
- On Windows: no special setup required
