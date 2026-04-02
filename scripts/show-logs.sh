#!/bin/bash

# Cross-platform script to show application logs
# Usage: ./show-logs.sh [lines] [--watch]
# Example: ./show-logs.sh 500
# Example: ./show-logs.sh --watch
# Example: ./show-logs.sh 500 --watch

LINES=100
WATCH=false

for arg in "$@"; do
    if [[ "$arg" == "--watch" ]]; then
        WATCH=true
    elif [[ "$arg" =~ ^[0-9]+$ ]]; then
        LINES=$arg
    fi
done

if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS
    LOG_FILE="$HOME/Library/Application Support/org.w3id.foundation/application.log"
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    # Linux
    LOG_FILE="$HOME/.local/share/org.w3id.foundation/application.log"
elif [[ "$OSTYPE" == "msys" ]] || [[ "$OSTYPE" == "win32" ]]; then
    # Windows (Git Bash or similar)
    LOG_FILE="$LOCALAPPDATA/org.w3id.foundation/application.log"
else
    echo "Unsupported platform: $OSTYPE"
    exit 1
fi

if [ ! -f "$LOG_FILE" ]; then
    echo "Log file not found: $LOG_FILE"
    exit 1
fi

echo "=== FOUNDATION Application Logs ==="
echo "Current date/time: $(date '+%Y-%m-%d %H:%M:%S')"
if $WATCH; then
    echo "Showing last $LINES lines (watching for changes...)"
else
    echo "Showing last $LINES lines"
fi
echo "==================================="
echo ""

if $WATCH; then
    tail -n "$LINES" -f "$LOG_FILE"
else
    tail -n "$LINES" "$LOG_FILE"
fi
