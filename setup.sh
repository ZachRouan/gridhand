#!/bin/bash
set -e

# Must not be run as root — only specific commands use sudo
if [ "$(id -u)" -eq 0 ]; then
    echo "Do not run this script as root or with sudo. It will prompt for sudo when needed."
    exit 1
fi

REAL_USER="$(id -un)"

echo "=== gui-tool setup ==="

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

if ! groups | grep -q input; then
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
EXT_UUID="window-calls@ickyicky.github.io"
if gnome-extensions list 2>/dev/null | grep -q "$EXT_UUID"; then
    echo "window-calls extension already installed."
else
    TMP_DIR="$(mktemp -d /tmp/window-calls-XXXXXX)"
    echo "Downloading window-calls extension from GitHub..."
    if curl -sL "https://github.com/ickyicky/window-calls/archive/refs/heads/main.tar.gz" -o "$TMP_DIR/ext.tar.gz" 2>/dev/null; then
        tar -xzf "$TMP_DIR/ext.tar.gz" -C "$TMP_DIR"
        # Pack as extension zip from the extracted source
        EXT_SRC="$TMP_DIR/window-calls-main"
        if [ -d "$EXT_SRC" ] && [ -f "$EXT_SRC/metadata.json" ]; then
            (cd "$EXT_SRC" && zip -qr "$TMP_DIR/ext.zip" .)
            gnome-extensions install "$TMP_DIR/ext.zip" && echo "window-calls extension installed. Enable it with:" && echo "  gnome-extensions enable $EXT_UUID" || {
                echo "Failed to install extension. Try manually from:"
                echo "  https://github.com/ickyicky/window-calls"
            }
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

# 3. Build
echo ""
echo "Building gui-tool..."
cargo build --release
echo "Binary at: $(pwd)/target/release/gui-tool"

echo ""
echo "=== Setup complete ==="
echo "If you were added to the 'input' group, log out and back in before using gui-tool."
