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

## Precision Targeting (Grid Workflow)

The grid system eliminates pixel coordinate guessing. The agent reads cell labels from images instead of computing coordinates.

### Step 1: Get a grid view
```bash
gui-tool screenshot --window-id 123 --grid --output /tmp/grid.png
```
The screenshot has a labeled 8x6 grid overlay (cells A1 through D3).

### Step 2: Identify the target cell
Read the grid image. Find which cell contains your target (e.g., the button is in B2).

### Step 3: Zoom if needed
If the target is small within the cell, zoom into it:
```bash
gui-tool screenshot --window-id 123 --grid --cell B2 --output /tmp/zoom.png
```
This crops to cell B2 and draws a new sub-grid. Find the sub-cell (e.g., C1).

You can zoom multiple levels with dot notation:
```bash
gui-tool screenshot --window-id 123 --grid --cell B2.C1 --output /tmp/zoom2.png
```

### Step 4: Click
```bash
gui-tool mouse move --cell B2.C1 --window-id 123
gui-tool mouse click --window-id 123
```
The tool calculates the center of cell C1 within cell B2 and moves there.

### Key rules
- **No pixel math.** Cell references from grid images map directly to mouse positions.
- **Default grid is 8x6.** Override with `--grid 6x4` for denser grids.
- **Dot notation for recursive zoom.** `B2.C1` means "cell C1 within cell B2."
- **`--grid` density must match.** If you used `--grid 6x4` for the screenshot, pass `--grid 6x4` to `mouse move` too.

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
