#!/bin/bash

# Cross-platform script to show application logs
# Usage: ./show-logs.sh [lines]
# Example: ./show-logs.sh 500

LINES=${1:-100}  # Default to 100 lines if no argument provided

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
echo "Showing last $LINES lines"
echo "==================================="
echo ""

tail -n "$LINES" "$LOG_FILE"
