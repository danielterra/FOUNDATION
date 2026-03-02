---
name: commit
description: Stages all changes, generates a concise commit message, commits, and pushes
disable-model-invocation: false
---

# Commit & Push

## Current Branch
!`git branch --show-current`

## Git Status
!`git status --short`

## Staged + Unstaged Diff Summary
!`git diff HEAD --stat`

## Full Diff
!`git diff HEAD`

---

## Instructions

1. Stage all changes: `git add -A`
2. Write a concise commit message:
   - Format: `<type>: <short summary>` (e.g. `feat: add X`, `fix: Y`, `refactor: Z`)
   - Max 72 characters
   - No bullet points, no body — subject line only
3. Commit using a HEREDOC so the message is properly formatted
4. Push to the current remote branch: `git push`

Do it now. No confirmation needed.
