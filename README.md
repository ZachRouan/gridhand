# Agent Desktop Interface (`gui-tool`)

`Linux` `macOS` `Windows` `Zero Dependencies`

A lightweight, zero-dependency binary that gives AI coding agents reliable native control over local desktops. Screenshots, window management, mouse control, keyboard input — all through raw OS APIs with strict JSON output.

Also works as a standalone CLI tool for shell scripts and automation.

## Why Agents Need This

**Deterministic Output.** Every command returns structured JSON. No HTML to parse, no OCR to fail, no unstructured text to hallucinate over.

```json
{"status":"success","windows":[{"id":1234,"title":"Firefox","pid":5678}]}
```

**Native FFI.** Direct syscalls and OS framework calls — not brittle wrappers around `xdotool`, `osascript`, or `pyautogui`. Works on Wayland where most Linux automation tools don't.

**Secure & Auditable.** Zero third-party crates. Zero subprocess calls. The entire tool is ~3,500 lines of Rust using only the standard library. Minimal attack surface for agents running locally.

## Agent Integration

gui-tool ships as a **Claude Code plugin** with a built-in skill definition. It also follows the [open Agent Skills standard](https://agents.md/) and works with Codex, Gemini CLI, and other compatible agents.

### Claude Code (plugin install)

```bash
# 1. Clone and build
git clone https://github.com/zachr-ux/agent-desktop-interface
cd agent-desktop-interface
./setup.sh

# 2. Copy the plugin to your Claude Code skills directory
cp -r . ~/.claude/skills/gui-tool

# 3. Restart Claude Code — the plugin is auto-discovered
```

The plugin manifest (`.claude-plugin/plugin.json`) and skill definition (`skills/gui-tool/SKILL.md`) are included in the repo. Once copied, the skill appears in `/skills` as `gui-tool:gui-tool`.

### Other agents (Codex, Gemini CLI, etc.)

Add gui-tool to your PATH and reference the skill in your `AGENTS.md` or `GEMINI.md`:

```bash
sudo ln -s $(pwd)/target/release/gui-tool /usr/local/bin/gui-tool
```

The skill definition is at [`skills/gui-tool/SKILL.md`](skills/gui-tool/SKILL.md).

### Examples

**Agent lists windows and clicks one:**
```bash
$ gui-tool windows list
{"status":"success","windows":[{"id":2045481940,"title":"Text Editor","pid":272151}, ...]}

$ gui-tool windows raise 2045481940
{"status":"success"}

$ gui-tool mouse move 500 300
{"status":"success"}

$ gui-tool mouse click
{"status":"success"}
```

**Agent takes a cropped window screenshot:**
```bash
$ gui-tool screenshot --window "Firefox" --output /tmp/firefox.png
{"status":"success","path":"/tmp/firefox.png","window":{...},"bounds":{"x":100,"y":200,"width":800,"height":600}}
```

## Install

Requires the [Rust toolchain](https://rustup.rs/).

```bash
git clone https://github.com/zachr-ux/agent-desktop-interface
cd agent-desktop-interface
./setup.sh
```

The setup script detects your OS, handles platform-specific setup, and builds the release binary.

To use as a Claude Code plugin, copy the repo to `~/.claude/skills/gui-tool` after building (see [Agent Integration](#agent-integration) above).

### Linux

The setup script will:
- Create a udev rule for `/dev/uinput` access (requires sudo)
- Add your user to the `input` group (requires logout/login)
- Install and enable the [window-calls](https://github.com/ickyicky/window-calls) GNOME extension
- Build the release binary

### macOS

The setup script builds the binary. You must then grant permissions manually:
1. **System Settings > Privacy & Security > Accessibility** — add the `gui-tool` binary (required for mouse/keyboard)
2. **System Settings > Privacy & Security > Screen Recording** — add the `gui-tool` binary (required for screenshots)

### Windows

The setup script just builds the binary. No special permissions or setup needed. Run from Git Bash, MSYS2, or any bash-compatible shell. Or build manually:

```bash
cargo build --release
```

### Add to PATH (optional)

```bash
# Linux/macOS
sudo ln -s $(pwd)/target/release/gui-tool /usr/local/bin/gui-tool

# Windows (PowerShell, run as admin)
Copy-Item target\release\gui-tool.exe C:\Windows\System32\
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
# List all windows (IDs, titles, workspace info)
gui-tool windows list

# Bring a window to front by ID
gui-tool windows raise 1234567890
```

### Mouse

```bash
# Move to absolute coordinates
gui-tool mouse move 500 300

# Click
gui-tool mouse click
gui-tool mouse click --button right

# Focus a window first, then click (single process, no focus race)
gui-tool mouse click --window "Firefox"
gui-tool mouse move 500 300 --window-id 2045481940
```

### Keyboard

```bash
# Type text into focused window
gui-tool key type "hello world"

# Press key combinations
gui-tool key press "ctrl+a"
gui-tool key press "alt+f4"
gui-tool key press "ctrl+shift+t"
gui-tool key press "super"

# Type into a specific window
gui-tool key type "hello" --window "Terminal"
gui-tool key press "ctrl+a" --window-id 2045481940
```

## How It Works

Everything is implemented from scratch using only Rust's standard library:

### Linux
- **Input**: `/dev/uinput` kernel interface via inline assembly ioctl syscalls
- **D-Bus**: Full wire protocol — SASL auth, message framing, type marshalling
- **Screenshots**: XDG Desktop Portal via raw D-Bus
- **Windows**: [window-calls](https://github.com/ickyicky/window-calls) GNOME extension via D-Bus

### macOS
- **Input**: `CGEventCreateMouseEvent` / `CGEventCreateKeyboardEvent` via CoreGraphics FFI
- **Screenshots**: `CGWindowListCreateImage` with native window cropping
- **Windows**: `CGWindowListCopyWindowInfo` + Objective-C runtime for activation

### Windows
- **Input**: `SendInput` from user32.dll
- **Screenshots**: `BitBlt` + `GetDIBits` from gdi32.dll
- **Windows**: `EnumWindows` + `SetForegroundWindow` from user32.dll

## Requirements

| Platform | Version | Setup |
|----------|---------|-------|
| Linux | GNOME/Wayland | `input` group + udev rule + window-calls extension |
| macOS | 10.15+ | Accessibility + Screen Recording permissions |
| Windows | 8+ | None |

## License

MIT
