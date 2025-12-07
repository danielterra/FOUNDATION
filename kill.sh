#!/bin/bash
# Kill all FOUNDATION processes

echo "🔪 Killing all FOUNDATION processes..."

# Kill by process name
killall -9 FOUNDATION-tauri-app 2>/dev/null
killall -9 cargo 2>/dev/null
killall -9 node 2>/dev/null
killall -9 vite 2>/dev/null

# Kill by pattern
pkill -9 -f "tauri dev" 2>/dev/null
pkill -9 -f "cargo run" 2>/dev/null
pkill -9 -f "npm run dev" 2>/dev/null

# Kill port 1420 (Vite dev server)
lsof -ti:1420 | xargs kill -9 2>/dev/null

sleep 1

echo "✅ All FOUNDATION processes killed"
