#!/usr/bin/env bash
set -e

echo "Building caffeine-applet..."
cargo build --release


INSTALL_DIR="$HOME/.local/bin"
DESKTOP_DIR="$HOME/.local/share/applications"
ICON_DIR="$HOME/.local/share/icons/hicolor/scalable/apps"

echo "Installing binary to $INSTALL_DIR..."
mkdir -p "$INSTALL_DIR"
cp target/release/caffeine-applet "$INSTALL_DIR/"

echo "Copying .desktop file to $DESKTOP_DIR..."
mkdir -p "$DESKTOP_DIR"
cp assets/caffeine-applet.desktop "$DESKTOP_DIR/"

echo "Copying icon to $ICON_DIR..."
mkdir -p "$ICON_DIR"
cp assets/coffee-full.svg "$ICON_DIR/caffeine-applet.svg"

echo "Installation complete!"
echo "You can run 'caffeine-applet' from your terminal or add it as a cosmic applet."
