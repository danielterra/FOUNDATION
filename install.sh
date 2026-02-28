#!/bin/bash

# FOUNDATION Installer for macOS
# This script installs the FOUNDATION app and removes macOS quarantine

set -e

echo "🚀 Installing FOUNDATION..."

# Check if running on macOS
if [[ "$OSTYPE" != "darwin"* ]]; then
    echo "❌ This installer is only for macOS"
    exit 1
fi

# Get the directory where the script is located
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
DMG_PATH="$SCRIPT_DIR/src-tauri/target/release/bundle/macos/FOUNDATION_0.1.0_aarch64.dmg"

# Check if DMG exists
if [ ! -f "$DMG_PATH" ]; then
    echo "❌ DMG not found at: $DMG_PATH"
    echo "Please build the app first with: npm run tauri build"
    exit 1
fi

# Mount the DMG
echo "📦 Mounting DMG..."
MOUNT_POINT=$(hdiutil attach "$DMG_PATH" | grep Volumes | awk '{print $3}')

# Copy app to Applications
echo "📱 Installing to /Applications..."
sudo cp -R "$MOUNT_POINT/FOUNDATION.app" /Applications/

# Unmount DMG
echo "📤 Unmounting DMG..."
hdiutil detach "$MOUNT_POINT"

# Remove quarantine attribute
echo "🔓 Removing quarantine..."
sudo xattr -cr /Applications/FOUNDATION.app

echo "✅ FOUNDATION installed successfully!"
echo ""
echo "You can now open FOUNDATION from your Applications folder."
echo "Or run: open -a FOUNDATION"
