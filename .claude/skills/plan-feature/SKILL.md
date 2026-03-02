---
name: plan-feature
description: Plans a new feature by analyzing the codebase and generating a structured todo/ file
disable-model-invocation: true
argument-hint: "<description of the feature to plan>"
---

# Plan Feature: $ARGUMENTS

## Current State
!`git status --short`

## Recent Commits
!`git log --oneline -10`

---

## Architecture (Non-Negotiable)

```
Frontend (Svelte) → Commands → OWL → EAVTO → SQLite
```

- **EAVTO** (`src-tauri/src/eavto/`): only layer with direct SQLite access
- **OWL** (`src-tauri/src/owl/`): must use `eavto::` — never `sqlx::` or `rusqlite::`
- **Commands** (`src-tauri/src/commands/`): must use `owl::` — never `eavto::` directly
- **Frontend**: invokes Tauri commands only

## Code Quality Rules (apply to all planned code)

- **Comments**: only *why* — never *what*, no commented-out blocks
- **Logging**: `error!`/`warn!` and lifecycle events only — no `debug!`, `trace!`, `println!`, `console.*`
- **Error handling**: no `.unwrap()`, no `panic!` — use `?` or `Result`
- **Types**: no `: any` / `: unknown` — use specific types
- **Formatting**: max 100 columns, max 1000 lines per `.rs`/`.svelte` file
- **Ontology**: if new TTL files are needed, use the `/new-ontology` skill

---

## Steps

1. **Understand the feature** from `$ARGUMENTS`
2. **Explore the codebase** — read relevant files in all affected layers before planning anything
3. **Identify all touchpoints** — which files in EAVTO, OWL, Commands, and Frontend need to change or be created
4. **Design the implementation order** — always bottom-up: EAVTO → OWL → Commands → Frontend
5. **Check ontology needs** — does this feature require new or modified TTL files?
6. **Identify risks** — migrations, breaking changes, new dependencies, test coverage gaps

---

## Output

Generate a single todo file in the `todo/` folder using the Write tool.

**Filename:** `YYYYMMDD-HHMMSS-<feature-slug>.md` — get the timestamp with `date +%Y%m%d-%H%M%S`

**The feature slug** is a short hyphen-separated name derived from the feature description (e.g., `user-authentication`, `file-export`, `graph-search`).

**The todo file must contain:**

```markdown
# Feature Plan: <Feature Name>

## Overview
One paragraph describing what this feature does and why it is needed.

## Affected Layers
List each layer (EAVTO / OWL / Commands / Frontend / Ontology) and what changes are needed there.

## Implementation Tasks

Ordered list of concrete tasks, bottom-up. Each task must include:
- Which file(s) to create or modify
- What to add/change (function names, structs, Tauri commands, Svelte components, etc.)
- Any architectural constraints to respect

## Ontology Changes
List any new TTL files or modifications to existing ones. If none, write "None".

## Risks & Notes
- Migration needs (schema changes, data transforms)
- New dependencies (Cargo crates, npm packages)
- Test coverage requirements (eavto/ and owl/ changes need >80% coverage)
- Anything else the implementer must know before starting

## Validation
Shell commands or manual steps to confirm the feature is working correctly after implementation.
```

After writing the file, print a summary: the feature planned, layers affected, number of tasks, and the todo file path.
