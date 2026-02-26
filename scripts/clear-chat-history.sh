#!/bin/bash

# Clear chat history from FOUNDATION database
# This removes all messages, tool uses, tool results, and the main chat conversation
# while preserving all other data (classes, instances, ontology, etc.)

DB_PATH="$HOME/Documents/Foundation/FOUNDATION.db"

if [ ! -f "$DB_PATH" ]; then
    echo "❌ Database not found at: $DB_PATH"
    exit 1
fi

echo "🗑️  Clearing chat history from database..."
echo "📍 Database: $DB_PATH"
echo ""

# Count what will be deleted
MESSAGE_COUNT=$(sqlite3 "$DB_PATH" "SELECT COUNT(DISTINCT subject) FROM triples WHERE subject LIKE 'foundation:Message_%' OR subject = 'foundation:MainChatConversation';")
TOOLUSE_COUNT=$(sqlite3 "$DB_PATH" "SELECT COUNT(DISTINCT subject) FROM triples WHERE subject LIKE 'foundation:ToolUse_%';")
TOOLRESULT_COUNT=$(sqlite3 "$DB_PATH" "SELECT COUNT(DISTINCT subject) FROM triples WHERE subject LIKE 'foundation:ToolResult_%';")

echo "📊 Items to be deleted:"
echo "   - Messages: $MESSAGE_COUNT"
echo "   - ToolUse: $TOOLUSE_COUNT"
echo "   - ToolResult: $TOOLRESULT_COUNT"
echo ""

# Delete messages, tool uses, tool results, and conversation
sqlite3 "$DB_PATH" <<EOF
-- Delete all triples related to Messages (using range scan for index efficiency)
DELETE FROM triples
WHERE subject >= 'foundation:Message_'
  AND subject < 'foundation:Messagf';

-- Delete all triples related to ToolUse
DELETE FROM triples
WHERE subject >= 'foundation:ToolUse_'
  AND subject < 'foundation:ToolUsf';

-- Delete all triples related to ToolResult
DELETE FROM triples
WHERE subject >= 'foundation:ToolResult_'
  AND subject < 'foundation:ToolResultg';

-- Delete the main chat conversation
DELETE FROM triples WHERE subject = 'foundation:MainChatConversation';

-- Vacuum to reclaim space
VACUUM;
EOF

if [ $? -eq 0 ]; then
    echo "✅ Chat history cleared successfully!"
    echo ""

    # Show current database stats
    TOTAL_TRIPLES=$(sqlite3 "$DB_PATH" "SELECT COUNT(*) FROM triples WHERE retracted = 0;")
    TOTAL_ENTITIES=$(sqlite3 "$DB_PATH" "SELECT COUNT(DISTINCT subject) FROM triples WHERE retracted = 0;")

    echo "📊 Current database stats:"
    echo "   - Total triples: $TOTAL_TRIPLES"
    echo "   - Total entities: $TOTAL_ENTITIES"
else
    echo "❌ Error clearing chat history"
    exit 1
fi
