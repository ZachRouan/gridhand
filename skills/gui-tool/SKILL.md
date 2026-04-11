---
name: gui-tool
description: Interact with the desktop GUI — take screenshots, list/raise windows, click with grid targeting, type text, press key combos. Use when you need to see the screen, find windows, click on things, type into applications, or automate any GUI interaction. All commands return JSON.
---

# gui-tool

Desktop GUI automation CLI. All commands return JSON to stdout (errors to stderr).

```bash
gui-tool screenshot --output /tmp/screen.png                              # full screen
gui-tool screenshot --window-id 123 --output /tmp/app.png                 # cropped to window
gui-tool screenshot --window-id 123 --grid --output /tmp/grid.png         # grid overlay
gui-tool screenshot --window-id 123 --grid --cell B2 --output /tmp/z.png  # zoom into cell

gui-tool windows list                                # JSON list of all windows
gui-tool windows raise 123                           # bring window to front

gui-tool mouse click                                 # click at current position
gui-tool mouse click --button right                  # right-click
gui-tool mouse click --cell B2.C1 --window-id 123   # click cell (recursive zoom OK)

gui-tool key type "hello" --window-id 123            # type text
gui-tool key press "ctrl+a" --window-id 123          # key combo (ctrl/shift/alt/super + key)
```

---

## Grid Targeting

No pixel coordinates exist. The grid is the only way to click. Each cell has a red crosshair (+) at its center — a click on that cell lands **exactly on that crosshair, nowhere else**. If the crosshair isn't on your target, the click will miss. Zoom until a crosshair sits directly on the target.

### Orient → Zoom → Click → Verify

**Orient.** Take a grid screenshot. Identify which cell contains your target and note where within that cell it sits (e.g. "the button is in D1, near the bottom-left").

**Zoom.** Crop into that cell to get a sub-grid. Pick the sub-cell matching your position note — don't re-read text or hunt for icons in zoomed views, they'll be blurry. Translate spatially: "bottom-left of D1" → sub-cell B5 or C5. Keep zooming (append with dots: `D1.C5.F3`) until a crosshair is on the target. 2–3 levels is typical.

**Click.** `gui-tool mouse click --cell D1.C5 --window-id 123`

**Verify.** Zoom into the area you just clicked to check the result — a full-page screenshot is too small to see subtle state changes (button color, focus ring, selection highlight). If the click missed, re-orient from scratch — the screen state may have changed.

### Navigating Without Zooming Out

Zoomed views show **dimmed context from adjacent parent cells with their labels visible**. If your target is in a neighboring cell, reference it directly — do not zoom all the way out:

```bash
# Zoomed into G1, but target is actually in H1 (visible in dimmed context)
# WRONG: take a fresh full-grid screenshot and start over
# RIGHT: just use H1 directly
gui-tool screenshot --window-id 123 --grid --cell H1 --output /tmp/zoom.png
```

Only take a fresh full-grid screenshot (no `--cell`) when the screen has changed (after clicking, typing, or switching windows) or when you're genuinely lost.

### Between-Cell Targeting

Target straddles two cells? Use `+` to center on the boundary:

```bash
gui-tool mouse click --cell D3+E3 --window-id 123   # horizontal
gui-tool mouse click --cell D3+D4 --window-id 123   # vertical
gui-tool mouse click --cell D3+E4 --window-id 123   # diagonal
```

### Small Icons and Buttons

Tiny targets (like/dislike buttons, close icons, checkboxes) need 3+ zoom levels. On the first zoom, note the icon's position carefully — at deeper zooms it becomes a few blurry pixels. Trust your spatial note, not what the zoomed crop looks like.

### Dot Notation

`B2.C1` = sub-cell C1 within parent B2. Change the last segment to try a neighbor at the same depth (`B2.D2`). Append to go deeper (`B2.C1.F3`).
