# Agent Desktop Interface (`gui-tool`)

`Linux` `macOS` `Windows` `Zero Dependencies`

Cross-platform Rust CLI for GUI automation — screenshots, window management, mouse/keyboard control, and strict JSON output. No dependencies, single binary, uses direct OS APIs.

Built for AI desktop agents (Claude Code, Codex, Gemini CLI, etc.) but works fine as a general-purpose GUI automation tool.

## Features

- **Grid targeting:** Overlay a labeled grid on screenshots with red crosshairs at each cell center. Click by cell label — no pixel coordinates. Supports recursive zoom (`B2.C1`) and between-cell targeting (`D3+E3`).
- **Contextual zoom:** Zoomed views show the target cell with a coarser sub-grid, surrounded by dimmed context from adjacent cells with parent-level labels for spatial orientation.
- **No dependencies:** Pure std Rust, direct FFI to OS APIs (CoreGraphics, user32.dll, D-Bus). Compiles to a single small binary.
- **Wayland support:** Works natively on GNOME/Wayland via XDG Desktop Portals and the `window-calls` extension, where tools like `xdotool` and `pyautogui` break.
- **JSON output:** Every command returns structured JSON, so agents don't have to parse text output.

## Grid Targeting

The main idea: agents are bad at guessing pixel coordinates from screenshots. Instead, `gui-tool` overlays a labeled grid, and the agent references cells by label. The workflow is **orient → zoom → zoom → ... → click → verify**.

```bash
# Screenshot with labeled grid overlay (auto-scales: 16x9 for full screen)
gui-tool screenshot --window-id 123 --grid --output /tmp/grid.png

# Zoom into a cell — shows sub-grid with dimmed context from neighbors
gui-tool screenshot --window-id 123 --grid --cell B2 --output /tmp/zoom.png

# Recursive zoom for precision
gui-tool screenshot --window-id 123 --grid --cell B2.C1 --output /tmp/zoom2.png

# Click at a cell center (moves + clicks in one step)
gui-tool mouse click --cell B2.C1 --window-id 123

# Target straddles two cells? Zoom/click centered on the boundary
gui-tool mouse click --cell D3+E3 --window-id 123
```

## Commands

### Screenshots

```bash
# Full screen
gui-tool screenshot --output /tmp/screen.png

# Cropped to a specific window
gui-tool screenshot --window "Firefox" --output /tmp/firefox.png

# Screenshot by window ID (cropped)
gui-tool screenshot --window-id 2045481940 --output /tmp/app.png
```

### Window Management

```bash
# List all windows (returns JSON array of IDs, titles, PIDs, and bounds)
gui-tool windows list

# Bring a window to front by ID
gui-tool windows raise 1234567890
```

### Mouse

```bash
# Click at current position
gui-tool mouse click
gui-tool mouse click --button right

# Click at a grid cell center (moves + clicks in one step)
gui-tool mouse click --cell B2 --window-id 2045481940

# Between-cell click (centered on boundary)
gui-tool mouse click --cell D3+E3 --window-id 2045481940
```

All targeting uses `--cell` with grid references. There are no pixel coordinate commands — zoom the grid until a crosshair is on the target, then click.

### Keyboard

```bash
# Type text into focused window
gui-tool key type "hello world"

# Press key combinations
gui-tool key press "ctrl+a"
gui-tool key press "alt+f4"
gui-tool key press "super"
gui-tool key press "ctrl+shift+t"

# Type into a specific window
gui-tool key type "hello" --window "Terminal"
gui-tool key press "ctrl+a" --window-id 2045481940
```

## Agent Integration

A skill definition following the [Agent Skills](https://agentskills.io) standard is included in `skills/gui-tool/SKILL.md`.

**1. Add gui-tool to your PATH** (after building):

```bash
# Linux/macOS
sudo ln -s $(pwd)/target/release/gui-tool /usr/local/bin/gui-tool

# Or without sudo
ln -s $(pwd)/target/release/gui-tool ~/.local/bin/gui-tool
```

**2. Install the skill:**

```bash
# Claude Code
mkdir -p ~/.claude/skills/gui-tool
cp skills/gui-tool/SKILL.md ~/.claude/skills/gui-tool/SKILL.md

# Codex
mkdir -p ~/.codex/skills/gui-tool
cp skills/gui-tool/SKILL.md ~/.codex/skills/gui-tool/SKILL.md
```

## Installation

Requires the [Rust toolchain](https://rustup.rs/).

```bash
git clone https://github.com/ZachRouan/agent-desktop-interface
cd agent-desktop-interface
./setup.sh
```

The setup script detects your OS, handles platform-specific setup, and builds the release binary.

### Platform Requirements

|Platform   |Version      |Setup                                                                                                                 |
|-----------|-------------|----------------------------------------------------------------------------------------------------------------------|
|**Linux**  |GNOME/Wayland|`input` group + udev rule + [window-calls](https://github.com/ickyicky/window-calls) extension (handled by `setup.sh`)|
|**macOS**  |10.15+       |Grant **Accessibility** + **Screen Recording** permissions in System Settings                                         |
|**Windows**|8+           |None (`cargo build --release` in MSYS2, Git Bash, or PowerShell)                                                      |

## Architecture

~3,500 lines of Rust, no external crates. Each platform uses direct OS APIs:

- **Linux:** `/dev/uinput` for input via ioctl syscalls. Full D-Bus wire protocol implementation (SASL auth, message framing, type marshalling) for XDG Desktop Portal screenshots and GNOME `window-calls` window management.
- **macOS:** CoreGraphics FFI (`CGEventCreateMouseEvent`, `CGEventCreateKeyboardEvent`) for input. `CGWindowListCreateImage` for screenshots. Objective-C runtime bindings for window activation.
- **Windows:** `user32.dll` (`SendInput`, `EnumWindows`, `SetForegroundWindow`, `VkKeyScanW`) and `gdi32.dll` (`BitBlt`, `GetDIBits`) for input, window management, and screenshots.

## License

MIT
