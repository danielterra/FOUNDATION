---
name: code-commit
description: Use when committing changes to FOUNDATION. Covers pre-commit validation, conventional commit format in Portuguese, and project-specific commit conventions.
disable-model-invocation: false
---

# Code Commit

## Recent commits (style reference)
!`git log --oneline -n 15`

## Pre-commit checklist
1. ALWAYS run `cargo check --manifest-path src-tauri/Cargo.toml` if any Rust file changed.
2. ALWAYS run `npm run check` if any Svelte/TS file changed.
3. ALWAYS stage ALL FILES `git add -A` or `git add .`.
4. NEVER stage files with secrets (`.env`, `credentials.json`, `*.pem`, etc.).

## Commit message format
- Subject: `type(scope): description` — all lowercase, in Portuguese, imperative mood, no trailing period.
- Types: `fix`, `feat`, `chore`, `refactor`, `docs`, `test`, `perf`, `style`, `ci`.
- Scope: short module/feature name (e.g. `automation`, `paths`, `blackboard`, `mcp`, `inspector`, `ci`).
- Subject under 72 chars when possible.
- Body (optional): explain WHY, not WHAT. Blank line after subject.
- ALWAYS pass message via HEREDOC for correct multiline formatting.

## Rules
- NEVER commit unless user explicitly requests it.
- NEVER amend an existing commit unless user explicitly asks.
- When pre-commit hook fails: ALWAYS fix the issue and create a NEW commit (NEVER amend).
- NEVER skip hooks (`--no-verify`) without explicit user request.
- ALWAYS write subject in English.
- NEVER include emojis in commit messages.
