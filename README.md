# Agent Desktop Interface (`gui-tool`)

`Linux` `macOS` `Windows` `Zero Dependencies` `MCP Ready`

A lightweight, single-binary Rust CLI engineered to serve as the highly deterministic "hands and eyes" (Layer 3 Actuator) for AI desktop agents. 

By utilizing pure standard library Rust and direct OS APIs, `gui-tool` bypasses the severe dependencies, latency, and fragility of traditional Python automation frameworks (like `pyautogui` or `xdotool`). It is built specifically to bridge the spatial reasoning gap of Vision-Language Models (VLMs) via **recursive visual grids**.

## Why Agents Need This

The paradigm of AI automation relies heavily on visual grounding, but foundational models struggle with zero-shot pixel math. `gui-tool` solves the Agent-Computer Interface (ACI) bottleneck:

* **Recursive Grid Targeting:** VLMs are notoriously bad at guessing raw X/Y pixel coordinates. `gui-tool` superimposes an alphanumeric grid over screenshots. If an element is too small, the agent can request a localized sub-grid (e.g., `--cell B2.C1`), granting the LLM infinite pixel precision via simple text-token matching.
* **Zero Python Bloat & Native FFI:** No virtual environments and no Docker sandboxes required. It uses direct syscalls and OS framework calls (CoreGraphics, user32.dll, raw D-Bus). The entire tool compiles to a single minimal binary.
* **Wayland & Cross-Platform Native:** Works natively on modern Linux Wayland compositors (via XDG Portals and GNOME extensions) where legacy X11 tools fail, alongside perfect native support for Windows and macOS.
* **Deterministic JSON (MCP Ready):** Every command returns structured JSON. No HTML to parse, no OCR to fail, no unstructured terminal stdout to hallucinate over. It operates as the mathematically perfect primitive for Model Context Protocol (MCP) servers and Agent Skills.

## Agent Integration

A skill definition following the [Agent Skills](https://agentskills.io) standard is included in `skills/gui-tool/SKILL.md`. It works natively with Claude Code, Codex, Gemini CLI, and other compatible agents.

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

The agent will automatically discover `gui-tool` and use it to interact directly with the host desktop.

## Precision Targeting (The VLM Superpower)

Agents read cell labels from grid images instead of computing pixel coordinates. One or two zoom levels provide button-level precision on any resolution, drastically reducing prompt token sizes and eliminating spatial hallucination.

```bash
# Screenshot with labeled grid overlay (default auto-scales, e.g., 8x6)
gui-tool screenshot --window-id 123 --grid --output /tmp/grid.png

# Custom grid density
gui-tool screenshot --window-id 123 --grid 6x4 --output /tmp/grid.png

# Zoom into a cell with sub-grid (Recursive Zoom)
gui-tool screenshot --window-id 123 --grid --cell B2 --output /tmp/zoom.png

# Click a cell (automatically calculates the exact center of that cell)
gui-tool mouse move --cell B2 --window-id 123

# Recursive: click cell C1 strictly within the boundaries of parent cell B2
gui-tool mouse move --cell B2.C1 --window-id 123
gui-tool mouse click --window-id 123
```

## Standard Commands

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

### Mouse Automation
```bash
# Move to absolute screen coordinates
gui-tool mouse move 500 300

# Move relative to a window's top-left corner
gui-tool mouse move 100 200 --window-id 2045481940

# Click
gui-tool mouse click
gui-tool mouse click --button right

# Focus a window first, then click (single process, no focus race)
gui-tool mouse click --window "Firefox"
```
*Note: When `--window` or `--window-id` is used with `mouse move`, coordinates are relative to the window — pixel positions from a cropped screenshot map directly to mouse coordinates.*

### Keyboard Automation
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

## Installation & Setup

Requires the [Rust toolchain](https://rustup.rs/).

```bash
git clone https://github.com/ZachRouan/agent-desktop-interface
cd agent-desktop-interface
./setup.sh
```
The setup script detects your OS, handles platform-specific configurations, and builds the release binary.

### OS-Specific Requirements

| Platform | Version | Setup Required |
|----------|---------|-------|
| **Linux** | GNOME/Wayland | `input` group + udev rule + [window-calls](https://github.com/ickyicky/window-calls) extension (Handled by `setup.sh`) |
| **macOS** | 10.15+ | **Accessibility** + **Screen Recording** permissions must be granted manually in System Settings. |
| **Windows** | 8+ | None. (Can be built manually via `cargo build --release` in MSYS2, Git Bash, or PowerShell) |

## Architecture: How It Works

`gui-tool` achieves its tiny ~3,500 line footprint by strictly utilizing Rust's standard library to interface with raw OS mechanisms:

* **Linux:** Uses `/dev/uinput` kernel interface via inline assembly ioctl syscalls for input. Implements the full D-Bus wire protocol from scratch (SASL auth, message framing, type marshalling) to interface with XDG Desktop Portals for screenshots and GNOME `window-calls` for Wayland window management.
* **macOS:** Utilizes `CGEventCreateMouseEvent` / `CGEventCreateKeyboardEvent` via CoreGraphics FFI for input. Uses `CGWindowListCreateImage` for native window cropped screenshots and Objective-C runtime bindings for window activation.
* **Windows:** Directly binds to `user32.dll` (`SendInput`, `EnumWindows`, `SetForegroundWindow`, `VkKeyScanW`) and `gdi32.dll` (`BitBlt`, `GetDIBits`) for unmediated, low-latency execution.

## License

MIT
