# gui-tool

A zero-dependency Rust CLI for GUI interaction on Linux. Screenshots, window management, mouse control, keyboard input. Use it from the terminal, shell scripts, or as a tool for AI coding agents. JSON-in/JSON-out, single binary, no runtime dependencies beyond the OS.

## Why

GUI automation on Linux is fragmented — tools shell out to `xdotool` (X11 only), require heavy dependencies, or don't work on Wayland. gui-tool does everything through raw kernel interfaces and D-Bus wire protocol. No crates, no subprocess calls, no libc. One binary, works everywhere GNOME/Wayland runs.

It's also designed as a drop-in tool for AI coding agents (Codex, Gemini CLI, etc.) that need to see and interact with the desktop. The JSON output is easy to parse, and a [skill definition](#ai-agent-integration) is included so agents can discover and use it automatically.

## Install

```bash
git clone https://github.com/zachr-ux/gui-tool
cd gui-tool
./setup.sh
```

The setup script:
- Creates a udev rule for `/dev/uinput` access
- Adds your user to the `input` group (requires logout/login)
- Installs and enables the [window-calls](https://github.com/ickyicky/window-calls) GNOME extension
- Builds the release binary

Or build manually:
```bash
cargo build --release
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
- **Windows**: Calls the [window-calls](https://github.com/ickyicky/window-calls) GNOME Shell extension over D-Bus

Zero crates. Zero `extern "C"`. Zero subprocess calls. ~1,700 lines of Rust.

## Requirements

- Linux with GNOME on Wayland
- Rust toolchain (for building)
- User in `input` group + udev rule (for mouse/keyboard)
- window-calls GNOME extension (for window management)
- XDG Desktop Portal (for screenshots — included in GNOME by default)

## AI Agent Integration

A skill definition following the [open Agent Skills standard](https://agents.md/) is included in `skills/SKILL.md`. It works with any agent that supports the standard, including Codex, Gemini CLI, and others.

To install, copy `skills/` to your agent's skill directory. The agent will automatically discover gui-tool and use it when it needs to interact with the desktop.

Or just add gui-tool to your PATH and mention it in your project's `AGENTS.md` / `GEMINI.md` — most agents will figure it out from the `--help` output and JSON responses.

## License

MIT
