#!/bin/bash
set -e

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
    sudo usermod -aG input $USER
    echo "Added $USER to 'input' group. You must log out and back in for this to take effect."
else
    echo "User already in 'input' group."
fi

# 2. GNOME window-calls extension
echo ""
echo "Installing window-calls GNOME extension..."
if gnome-extensions list 2>/dev/null | grep -q "window-calls@ickyicky.github.io"; then
    echo "window-calls extension already installed."
else
    gnome-extensions install window-calls@ickyicky.github.io || {
        echo "Failed to install via gnome-extensions. Try installing from:"
        echo "  https://extensions.gnome.org/extension/4724/window-calls/"
    }
fi

# 3. Build
echo ""
echo "Building gui-tool..."
cargo build --release
echo "Binary at: $(pwd)/target/release/gui-tool"

echo ""
echo "=== Setup complete ==="
echo "If you were added to the 'input' group, log out and back in before using gui-tool."
