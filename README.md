# gui-tool

A zero-dependency Rust CLI for GUI interaction on Linux, macOS, and Windows. Screenshots, window management, mouse control, keyboard input. Use it from the terminal, shell scripts, or as a tool for AI coding agents. JSON-in/JSON-out, single binary, no runtime dependencies beyond the OS.

## Why

GUI automation is fragmented — tools shell out to `xdotool` (X11 only) or `osascript`, require heavy dependencies, or don't work on Wayland. gui-tool does everything through raw kernel interfaces and native FFI. No crates, no subprocess calls. One binary per platform.

It's also designed as a drop-in tool for AI coding agents (Codex, Gemini CLI, etc.) that need to see and interact with the desktop. The JSON output is easy to parse, and a [skill definition](#ai-agent-integration) is included so agents can discover and use it automatically.

## Install

Requires the [Rust toolchain](https://rustup.rs/).

```bash
git clone https://github.com/zachr-ux/gui-tool
cd gui-tool
./setup.sh
```

The setup script detects your OS, handles platform-specific setup, and builds the release binary.

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

## Usage

### Screenshots

```bash
# Full screen
gui-tool screenshot --output /tmp/screen.png

# Raise a window by title, then screenshot
gui-tool screenshot --window "Firefox" --output /tmp/firefox.png
```

### Window Management

```bash
# List all windows (returns JSON with IDs, titles, workspace info)
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
```

## Output Format

Success (stdout):
```json
{"status":"success","path":"/tmp/screen.png"}
```

Error (stderr):
```json
{"status":"error","message":"Failed to open /dev/uinput: Permission denied. Is user in 'input' group?"}
```

## How It Works

Everything is implemented from scratch using only Rust's standard library:

- **Mouse/Keyboard**: Writes `input_event` structs to `/dev/uinput` via `ioctl` syscalls (inline assembly, syscall 16 on x86_64)
- **D-Bus**: Full wire protocol implementation over Unix domain sockets — SASL EXTERNAL auth, message framing, type marshalling, method calls, signal reception
- **Screenshots**: XDG Desktop Portal via raw D-Bus — predicts request handle, subscribes to Response signal, waits for URI
- **Windows (Linux)**: Calls the [window-calls](https://github.com/ickyicky/window-calls) GNOME Shell extension over D-Bus
- **Input (macOS)**: CoreGraphics event injection (`CGEventCreateMouseEvent`, `CGEventCreateKeyboardEvent`)
- **Screenshots (macOS)**: `CGWindowListCreateImage` with native window cropping
- **Windows (macOS)**: `CGWindowListCopyWindowInfo` + Objective-C runtime for window activation
- **Input (Windows)**: `SendInput` from user32.dll for mouse and keyboard injection
- **Screenshots (Windows)**: `BitBlt` + `GetDIBits` from gdi32.dll, raw pixel extraction to PNG
- **Windows (Windows)**: `EnumWindows` + `SetForegroundWindow` from user32.dll

Zero crates. Zero subprocess calls. ~3,500 lines of Rust.

## Requirements

### Linux
- GNOME on Wayland
- User in `input` group + udev rule (for mouse/keyboard)
- window-calls GNOME extension (for window management)
- XDG Desktop Portal (for screenshots — included in GNOME by default)

### macOS
- macOS 10.15+
- Accessibility permission (System Settings > Privacy & Security > Accessibility) — required for mouse/keyboard input
- Screen Recording permission (System Settings > Privacy & Security > Screen Recording) — required for screenshots

### Windows
- Windows 8+
- No special permissions required

## AI Agent Integration

A skill definition following the [open Agent Skills standard](https://agents.md/) is included in `skills/SKILL.md`. It works with any agent that supports the standard, including Codex, Gemini CLI, and others.

To install, copy `skills/` to your agent's skill directory. The agent will automatically discover gui-tool and use it when it needs to interact with the desktop.

Or just add gui-tool to your PATH and mention it in your project's `AGENTS.md` / `GEMINI.md` — most agents will figure it out from the `--help` output and JSON responses.

## License

MIT
