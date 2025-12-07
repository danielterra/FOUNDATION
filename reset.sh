#!/bin/bash
# Reset FOUNDATION: kill all processes and delete database

echo "🔄 Resetting FOUNDATION..."

# Kill all processes
echo "🔪 Killing processes..."
killall -9 FOUNDATION-tauri-app 2>/dev/null
killall -9 cargo 2>/dev/null
killall -9 node 2>/dev/null
killall -9 vite 2>/dev/null

pkill -9 -f "tauri dev" 2>/dev/null
pkill -9 -f "cargo run" 2>/dev/null
pkill -9 -f "npm run dev" 2>/dev/null

lsof -ti:1420 | xargs kill -9 2>/dev/null

sleep 1

# Delete databases
echo "🗑️  Deleting databases..."
rm -f FOUNDATION.db
rm -f FOUNDATION.db-shm
rm -f FOUNDATION.db-wal
rm -rf src-tauri/.foundation-data

echo "✅ FOUNDATION reset complete"
echo "   - All processes killed"
echo "   - Databases deleted"
