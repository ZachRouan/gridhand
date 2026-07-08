#!/bin/bash
set -e

# --skip-build: do the platform setup only (for users who installed the binary
# via `cargo install gui-tool` or a prebuilt release and don't need to compile).
SKIP_BUILD=0
for arg in "$@"; do
    case "$arg" in
        --skip-build|--setup-only) SKIP_BUILD=1 ;;
    esac
done

OS="$(uname -s)"
echo "=== gui-tool setup ($OS) ==="

# --- Linux-specific setup ---
if [ "$OS" = "Linux" ]; then
    # Must not be run as root — only specific commands use sudo
    if [ "$(id -u)" -eq 0 ]; then
        echo "Do not run this script as root or with sudo. It will prompt for sudo when needed."
        exit 1
    fi

    REAL_USER="$(id -un)"

    # 1. uinput access
    echo ""
    echo "Setting up /dev/uinput access..."
    if [ ! -f /etc/udev/rules.d/99-uinput.rules ]; then
        echo 'KERNEL=="uinput", GROUP="input", MODE="0660"' | sudo tee /etc/udev/rules.d/99-uinput.rules
        sudo udevadm control --reload-rules
        sudo udevadm trigger
        echo "udev rule created."
    else
        echo "udev rule already exists."
    fi

    if [ ! -e /dev/uinput ]; then
        echo "Loading uinput kernel module..."
        sudo modprobe uinput
        echo uinput | sudo tee /etc/modules-load.d/uinput.conf >/dev/null
    fi

    if ! groups | grep -qw input; then
        if sudo usermod -aG input "$REAL_USER"; then
            echo "Added $REAL_USER to 'input' group. You must log out and back in for this to take effect."
        else
            echo "WARNING: Could not add $REAL_USER to 'input' group. Run manually:"
            echo "  sudo usermod -aG input $REAL_USER"
        fi
    else
        echo "User already in 'input' group."
    fi

    # 2. GNOME window-calls extension
    echo ""
    echo "Installing window-calls GNOME extension..."
    EXT_UUID="window-calls@domandoman.xyz"
    if gnome-extensions list 2>/dev/null | grep -q "$EXT_UUID"; then
        gnome-extensions enable "$EXT_UUID" 2>/dev/null
        echo "window-calls extension already installed and enabled."
    else
        TMP_DIR="$(mktemp -d /tmp/window-calls-XXXXXX)"
        echo "Downloading window-calls extension from GitHub..."
        if curl -fsSL "https://github.com/ickyicky/window-calls/archive/refs/heads/main.tar.gz" -o "$TMP_DIR/ext.tar.gz" 2>/dev/null; then
            tar -xzf "$TMP_DIR/ext.tar.gz" -C "$TMP_DIR"
            EXT_SRC="$TMP_DIR/window-calls-main"
            if [ -d "$EXT_SRC" ] && [ -f "$EXT_SRC/metadata.json" ]; then
                (cd "$EXT_SRC" && zip -qr "$TMP_DIR/ext.zip" .)
                if gnome-extensions install "$TMP_DIR/ext.zip" && gnome-extensions enable "$EXT_UUID" 2>/dev/null; then
                    echo "window-calls extension installed and enabled."
                else
                    echo "Failed to install extension. Try manually from:"
                    echo "  https://github.com/ickyicky/window-calls"
                fi
            else
                echo "Unexpected archive layout. Install manually from:"
                echo "  https://github.com/ickyicky/window-calls"
            fi
        else
            echo "Failed to download extension. Install manually from:"
            echo "  https://github.com/ickyicky/window-calls"
        fi
        rm -rf "$TMP_DIR"
    fi
fi

# --- macOS-specific setup ---
if [ "$OS" = "Darwin" ]; then
    echo ""
    echo "macOS detected. No automated setup needed."
    echo "After building, you must grant permissions in System Settings > Privacy & Security:"
    echo "  - Accessibility (for mouse/keyboard input)"
    echo "  - Screen Recording (for screenshots)"
fi

# --- Windows (Git Bash / MSYS2) ---
if echo "$OS" | grep -qi "MINGW\|MSYS\|CYGWIN"; then
    echo ""
    echo "Windows detected. No special setup needed."
fi

# --- Build (all platforms) ---
if [ "$SKIP_BUILD" -eq 1 ]; then
    echo ""
    echo "Skipping build (--skip-build): using an already-installed gui-tool binary."
else
    echo ""
    echo "Building gui-tool..."
    cargo build --release
    echo "Binary at: $(pwd)/target/release/gui-tool"
fi

echo ""
echo "=== Setup complete ==="
if [ "$OS" = "Linux" ]; then
    echo "If you were added to the 'input' group, log out and back in before using gui-tool."
fi
