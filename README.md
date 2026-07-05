<div align="center">

# Agent Desktop Interface — `gui-tool`

**Let an AI agent click any desktop app by naming a grid cell — not guessing pixels.**

A cross-platform Rust CLI for GUI automation: screenshots, window management, mouse and keyboard control, strict JSON in / JSON out. **Zero dependencies** — no crates, one small binary, direct OS APIs.

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square)](LICENSE)
![Platforms](https://img.shields.io/badge/platform-Linux%20%C2%B7%20macOS%20%C2%B7%20Windows-informational?style=flat-square)
![Zero dependencies](https://img.shields.io/badge/dependencies-0-success?style=flat-square)
![Rust](https://img.shields.io/badge/Rust-std--only-orange?style=flat-square)
[![GitHub stars](https://img.shields.io/github/stars/ZachRouan/agent-desktop-interface?style=flat-square)](https://github.com/ZachRouan/agent-desktop-interface/stargazers)

<img src="https://raw.githubusercontent.com/ZachRouan/agent-desktop-interface/main/assets/demo.gif" alt="gui-tool grid-targeting demo: orient to a labeled grid, zoom into a cell, click the crosshair, verify" width="440" />

<sub>Real `gui-tool` output — orient to a labeled grid → zoom into a cell → click the crosshair → verify.</sub>

</div>

---

Built for AI desktop agents (Claude Code, Codex, Gemini CLI, and friends), but it works fine as a general-purpose GUI automation tool.

**Why it's different:** agents are bad at guessing pixel coordinates from a screenshot, and every other automation tool makes them try. `gui-tool` removes pixels entirely — it overlays a **labeled grid** with a crosshair at each cell's center, the agent names a cell, and the click lands exactly on that crosshair. Need more precision? Zoom into the cell for a sub-grid and name a sub-cell (`C7.F3`). It's the whole workflow in one line: **orient → zoom → click → verify.**

**It sees pixels, not an accessibility tree.** Most desktop-automation tools for agents find elements by reading the OS accessibility tree — which only helps when that tree *exists*, is *complete*, and is *correct*. `gui-tool` never touches it. It works from the actual rendered screen, so it drives what tree-based tools can't see: games, `<canvas>` / WebGL apps, Flutter and other custom-drawn UIs, remote desktops and VNC, video, and any app with missing, mislabeled, or wrong accessibility data. **If a human can see it, `gui-tool` can click it.**

Everything is hand-rolled against raw OS APIs — its own PNG encoder, D-Bus client, JSON output, and DEFLATE — so there are no crates to audit, no build surprises, and it runs natively on **GNOME/Wayland** where `xdotool` and `pyautogui` give up.

<div align="center">

<img src="https://raw.githubusercontent.com/ZachRouan/agent-desktop-interface/main/assets/demo_browser.gif" alt="gui-tool driving a Firefox private window: navigate to a site, click a search field, type a query, and submit — entirely by grid cell" width="720" />

<sub>End-to-end in a real browser — navigate, click a field by cell, type, and submit, no pixel coordinates anywhere.</sub>

</div>

## Features

- **Grid targeting:** Overlay a labeled grid on screenshots with red crosshairs at each cell center. Click by cell label — no pixel coordinates. Supports recursive zoom (`B2.C1`) and between-cell targeting (`D3+E3`).
- **No accessibility tree required:** Targets purely from what's on screen — never AT-SPI, UI Automation, or the AX APIs. Works where the accessibility tree is missing, incomplete, or wrong: games, `<canvas>`/WebGL, custom-drawn UIs (Flutter, etc.), and remote desktops.
- **Contextual zoom:** Zoomed views show the target cell with a coarser sub-grid, surrounded by dimmed context from adjacent cells with parent-level labels for spatial orientation.
- **No dependencies:** Pure std Rust, direct FFI to OS APIs (CoreGraphics, user32.dll, D-Bus). Compiles to a single small binary.
- **Wayland support:** Works natively on GNOME/Wayland via XDG Desktop Portals and the `window-calls` extension, where tools like `xdotool` and `pyautogui` break.
- **JSON output:** Every command returns structured JSON, so agents don't have to parse text output.

## Grid Targeting

The full workflow — **orient → zoom → zoom → ... → click → verify** — in commands:

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

## Install

Pick the easiest that fits. On **Linux and macOS** there's a one-time platform-setup step after you get the binary (input permissions and the GNOME `window-calls` extension on Linux; Accessibility + Screen Recording permissions on macOS) — see [Platform Requirements](#platform-requirements). Windows needs nothing.

**Prebuilt binary** (no Rust toolchain) — grab your platform's archive from the [latest release](https://github.com/ZachRouan/agent-desktop-interface/releases/latest), then:

```bash
tar xzf gui-tool-*.tar.gz                      # or unzip on Windows
sudo install gui-tool-*/gui-tool /usr/local/bin/   # or: mv … ~/.local/bin/
```

**From crates.io** (needs the [Rust toolchain](https://rustup.rs/)):

```bash
cargo install gui-tool
```

**From source:**

```bash
git clone https://github.com/ZachRouan/agent-desktop-interface
cd agent-desktop-interface
./setup.sh          # detects your OS, does platform setup, and builds
```

After a **prebuilt or `cargo install`** on Linux/macOS, run the platform setup without rebuilding — either follow the manual steps in [Platform Requirements](#platform-requirements), or from a clone:

```bash
./setup.sh --skip-build
```

### Platform Requirements

|Platform   |Version      |Setup                                                                                                                 |
|-----------|-------------|----------------------------------------------------------------------------------------------------------------------|
|**Linux**  |GNOME/Wayland|`input` group + udev rule + [window-calls](https://github.com/ickyicky/window-calls) extension (handled by `setup.sh`)|
|**macOS**  |10.15+       |Grant **Accessibility** + **Screen Recording** permissions in System Settings                                         |
|**Windows**|8+           |None (`cargo build --release` in MSYS2, Git Bash, or PowerShell)                                                      |

> The macOS **Accessibility** permission grants the right to *synthesize* mouse/keyboard input (post `CGEvent`s) — it is **not** used to read the accessibility tree. `gui-tool` never inspects the tree on any platform.

## Architecture

~3,500 lines of Rust, no external crates. Each platform uses direct OS APIs:

- **Linux:** `/dev/uinput` for input via ioctl syscalls. Full D-Bus wire protocol implementation (SASL auth, message framing, type marshalling) for XDG Desktop Portal screenshots and GNOME `window-calls` window management.
- **macOS:** CoreGraphics FFI (`CGEventCreateMouseEvent`, `CGEventCreateKeyboardEvent`) for input. `CGWindowListCreateImage` for screenshots. Objective-C runtime bindings for window activation.
- **Windows:** `user32.dll` (`SendInput`, `EnumWindows`, `SetForegroundWindow`, `VkKeyScanW`) and `gdi32.dll` (`BitBlt`, `GetDIBits`) for input, window management, and screenshots.

## License

MIT
