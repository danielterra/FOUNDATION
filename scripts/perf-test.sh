#!/bin/bash

# Performance test runner for FOUNDATION
# Copies the real DB (safely, without WAL issues) and runs ignored perf tests against it.
#
# Usage: npm run test:perf

set -e

BENCH_DB="/tmp/foundation_bench.db"

if [[ "$OSTYPE" == "darwin"* ]]; then
    FOUNDATION_DB="$HOME/Documents/Foundation/FOUNDATION.db"
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    FOUNDATION_DB="$HOME/Documents/Foundation/FOUNDATION.db"
elif [[ "$OSTYPE" == "msys" ]] || [[ "$OSTYPE" == "win32" ]]; then
    FOUNDATION_DB="$USERPROFILE/Documents/Foundation/FOUNDATION.db"
else
    echo "Unsupported platform: $OSTYPE"
    exit 1
fi

if [ ! -f "$FOUNDATION_DB" ]; then
    echo "Database not found: $FOUNDATION_DB"
    exit 1
fi

echo "=== FOUNDATION Performance Tests ==="
echo "Source DB: $FOUNDATION_DB"
echo "Bench DB:  $BENCH_DB"
echo "===================================="
echo ""

echo "Copying database with VACUUM INTO..."
rm -f "$BENCH_DB"
sqlite3 "$FOUNDATION_DB" "VACUUM INTO '$BENCH_DB'"
echo "Done."
echo ""

echo "Running performance tests..."
FOUNDATION_BENCH_DB="$BENCH_DB" cargo test \
    --manifest-path src-tauri/Cargo.toml \
    --lib perf_ \
    -- --ignored --nocapture

echo ""
echo "Cleaning up..."
rm -f "$BENCH_DB"
echo "Done."
