---
name: code-review
description: Use when reviewing pending code changes (working tree + staged) before commit. Checks against FOUNDATION conventions, architecture layer rules, validates compilation, and flags violations of CLAUDE.md / development.md rules.
disable-model-invocation: false
---

# Code Review

## Pending changes
!`git status --short`

## Diff stats
!`git diff --stat HEAD`

## Steps
1. Read the full diff: `git diff HEAD` (or `git diff --staged` for staged-only).
2. For each changed file, verify against the rules below. Read full files (not just hunks) when checking call sites.
3. If Rust changed → `cargo check --manifest-path src-tauri/Cargo.toml`.
4. If Svelte/TS changed → `npm run check`.
5. Report findings grouped by severity: **blockers** (rule violations, broken builds), **warnings** (smells, conventions), **suggestions** (optional improvements).

## Architecture layer rules (CRITICAL — from docs/development.md)
- Frontend → Commands → OWL → EAVTO → SQLite. Each layer ONLY uses the layer directly below.
- NEVER allow `commands/` to import from `eavto/` directly — must go through `owl/`.
- NEVER allow `owl/` to execute raw SQL — must go through `eavto/`.
- NEVER allow `commands/` or `owl/` to bypass abstractions and hit `rusqlite::Connection` SQL directly for ontology data.

## Project rule violations to flag
- Scripts in Node/Python/shell — only Rust scripts allowed.
- Comments explaining WHAT instead of WHY.
- Commented-out code or `TODO`/`FIXME` markers.
- Suppressed warnings (`#[allow(...)]`, `// eslint-disable`, etc.) without justification.
- Redundant wrapper functions when existing helpers cover the case.
- Hardcoded IRIs not coming from `search(...)` MCP results.
- Raw SQL `INSERT`/`UPDATE`/`DELETE`/`DROP`/`TRUNCATE` outside `eavto/` layer.
- Edits to `core-ontology/ontology.sql` (auto-generated; mutate via MCP on live DB).
- New `Cargo.toml` deps without justification or with conflicting features per platform.

## Quality checklist for new code
- Names self-document — no line-comments restating the code.
- No half-finished implementations or premature abstractions.
- No backwards-compat shims (renamed `_unused`, removed-code comments, dead re-exports).
- Error handling at boundaries only; trust internal contracts.
- No feature flags or compatibility shims when the code can simply change.

## Triple store conventions
- `retracted = 0` filter alone is insufficient for current values — for property reads ALWAYS use `tx = (SELECT MAX(tx) ... )` per immutability model in CLAUDE.md.
- Reads of literal values must check both `object` and `object_value` (use `COALESCE`).
- Datetime literals live in `object_datetime` (Unix ms), NOT `object_value`.

## Rules
- ALWAYS read full files when verifying that a function isn't called incorrectly elsewhere.
- NEVER auto-fix during review — report only, let user decide.
- ALWAYS group findings by severity (blocker / warning / suggestion).
- NEVER skip the build/check step — broken builds are blockers.
- ALWAYS report a clean review when nothing is found, rather than padding with low-value suggestions.
