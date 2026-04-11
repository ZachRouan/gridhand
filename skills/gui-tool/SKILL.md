---
name: gui-tool
description: Interact with the desktop GUI — take screenshots, list/raise windows, click with grid targeting, type text, press key combos. Use when you need to see the screen, find windows, click on things, type into applications, or automate any GUI interaction. All commands return JSON.
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

# Screenshot with grid overlay for targeting
gui-tool screenshot --window-id 123 --grid --output /tmp/grid.png

# Zoom into a grid cell
gui-tool screenshot --window-id 123 --grid --cell B2 --output /tmp/zoom.png
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
# Click at current mouse position (default: left)
gui-tool mouse click
gui-tool mouse click --button right

# Click at a grid cell center (moves + clicks in one step)
gui-tool mouse click --cell B2 --window-id 123
gui-tool mouse click --cell B2.C1 --window-id 123
```

All targeting uses `--cell` with grid references. There are no pixel coordinate commands — zoom the grid until the target cell is precise enough, then click.

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

## Grid Targeting Workflow

The grid system is the only way to click on things. No pixel coordinates exist. Each grid cell has a red crosshair (+) at its center showing exactly where a click would land. **Always zoom before clicking.**

### How it works

The workflow is a loop: **orient → zoom → zoom → ... → click → verify**. You keep zooming until a crosshair is on your target. Each zoom narrows the region. At each level you carry forward your spatial knowledge of where the target sits within the current cell.

### Orient
```bash
gui-tool screenshot --window-id 123 --grid --output /tmp/grid.png
```
Read the image. Identify which cell contains your target. Note the target's position **within** that cell (e.g., "the search bar is in D1, sitting in the lower-left portion of the cell"). This position is critical — you will use it at every subsequent zoom level.

### Zoom (repeat until precise)
```bash
gui-tool screenshot --window-id 123 --grid --cell D1 --output /tmp/zoom.png
```
This crops to D1, scales it up, and draws a new sub-grid with new crosshairs.

**Pick the sub-cell using spatial reasoning.** Zoomed crops are scaled-up pixel regions. Text will be blurry or unreadable. Uniform UI areas (toolbars, headers, sidebars) will look like featureless blocks of color. This is normal — do NOT try to re-identify the target by scanning for text or icons. Instead, translate the position you noted:
- "The search bar was in the lower-left of D1" → pick a sub-cell in the lower-left, like B5 or C5
- "The button was in the upper-right of B3" → pick a sub-cell like G1 or H1

Then zoom again to verify and refine:
```bash
gui-tool screenshot --window-id 123 --grid --cell D1.C5 --output /tmp/zoom2.png
```
Now you can see the sub-region in detail. Is a crosshair on your target? If yes, click. If no, pick a better sub-cell and zoom again:
```bash
gui-tool screenshot --window-id 123 --grid --cell D1.C5.F3 --output /tmp/zoom3.png
```
**Keep zooming until a crosshair is clearly on the target.** Small buttons and icons may need 3+ zoom levels. This is normal.

### Click
```bash
gui-tool mouse click --cell D1.C5.F3 --window-id 123
```
The tool calculates the final crosshair position through all zoom levels and clicks there.

### Verify
Take a plain screenshot (no grid) after clicking to confirm you hit the right element. If you missed, start over from Orient — the screen state has changed.

### Key rules
- **Zoom is a loop, not a single step.** Keep zooming until a crosshair is on the target. 2-3 levels is typical.
- **Spatial reasoning, not visual search.** Pick sub-cells based on where the target sat in the parent cell. Never hunt through zoomed crops looking for text.
- **Carry position forward.** At each zoom level, ask: "where within this cell was my target?" Then pick the matching sub-cell.
- **If you're lost, start over.** Take a fresh full grid screenshot and re-orient. Don't keep zooming into random cells.
- **Dot notation for recursive zoom.** `B2.C1` means "cell C1 within cell B2."
- **Between-cell targeting.** If a target straddles two cells, use `+` to zoom/click centered on the boundary: `D3+E3` (horizontal), `D3+D4` (vertical), `D3+E4` (diagonal). The sub-grid crosshairs will straddle the boundary.
- **Zoom out by shortening the chain.** If `--cell A2.B3` was wrong, try `--cell A2.C3` (different sub-cell, same parent).
- **Zoom is instant.** Screenshots are cached — zooming into different cells of the same parent reuses the same base image.

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

**Focus, click, and type (no race condition):**
```bash
gui-tool mouse click --cell B3 --window-id 2045481940
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
