#!/bin/bash

# Build script for FOUNDATION release
# Generates executables for macOS, Windows, and Linux

set -e  # Exit on error

echo "🚀 FOUNDATION Release Build v0.1.0 (Alpha)"
echo "=========================================="
echo ""

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Get the script directory
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_ROOT"

# Clean previous builds
echo -e "${BLUE}🧹 Cleaning previous builds...${NC}"
rm -rf src-tauri/target/release/bundle
echo ""

# Build frontend
echo -e "${BLUE}📦 Building frontend...${NC}"
npm run build
echo ""

# Build for current platform (macOS in this case)
echo -e "${BLUE}🍎 Building for macOS (Universal)...${NC}"
npm run tauri:build:mac
echo ""

echo -e "${GREEN}✅ Build complete!${NC}"
echo ""
echo "📍 Build artifacts location:"
echo "   macOS: src-tauri/target/release/bundle/dmg/"
echo "   macOS: src-tauri/target/release/bundle/macos/"
echo ""
echo "Note: For Windows and Linux builds, you need to run this script on those platforms"
echo "or use GitHub Actions CI/CD for cross-platform builds."
