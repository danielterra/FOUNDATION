---
name: bug-fix
description: Systematically investigates and helps fix a bug
disable-model-invocation: true
argument-hint: "<description of the bug — omit to auto-detect from logs>"
---

# Bug Fix: $ARGUMENTS

## Recent Errors
!`npm run logs 500 2>/dev/null | grep -i "error\|panic\|exception\|failed" | tail -n 20 || echo "No recent errors found"`

## Recent AI Interactions
!`sqlite3 ~/Documents/Foundation/FOUNDATION.db "SELECT datetime(timestamp, 'unixepoch', 'localtime') as time, role, substr(content, 1, 80) || '...' as preview FROM message_history ORDER BY timestamp DESC LIMIT 10;"`

## Database Status
!`sqlite3 ~/Documents/Foundation/FOUNDATION.db "SELECT 'Active triples: ' || COUNT(*) FROM triples WHERE retracted = 0 UNION ALL SELECT 'Retracted: ' || COUNT(*) FROM triples WHERE retracted = 1;"`

## Compilation
!`cargo check --manifest-path src-tauri/Cargo.toml --message-format=short 2>&1 | head -n 30`

---

If no bug description was provided, identify the bug from the logs and compilation output above before proceeding.

1. Analyze context — errors, DB state, compilation issues
2. Search for related code using Grep on key terms from the identified bug
3. Identify root cause
4. Fix following the layer architecture: Commands → OWL → EAVTO (never bypass layers)
5. Run `cargo check` to validate

**Output**: bug identified, root cause, files changed, fix applied.
