---
name: new-feature
description: Guides the implementation of a new feature following project architecture and quality rules
disable-model-invocation: true
argument-hint: <description of the feature>
---

# New Feature: $ARGUMENTS

## Current State
!`git status --short`

---

## Architecture (Non-Negotiable)

```
Frontend (Svelte) → Commands → OWL → EAVTO → SQLite
```

- **EAVTO** (`src-tauri/src/eavto/`): only layer with direct SQLite access
- **OWL** (`src-tauri/src/owl/`): must use `eavto::` — never `sqlx::` or `rusqlite::`
- **Commands** (`src-tauri/src/commands/`): must use `owl::` — never `eavto::` directly
- **Frontend**: invokes Tauri commands only

## Code Quality

- **Comments**: only *why* — never *what*, no commented-out blocks
- **Logging**: `error!`/`warn!` and lifecycle events only — no `debug!`, `trace!`, `println!`, `console.*`
- **Error handling**: no `.unwrap()`, no `panic!` — use `?` or `Result`
- **Types**: no `: any` / `: unknown` — use specific types; use logging utility not `console.*`
- **Formatting**: max 100 columns, max 1000 lines per `.rs`/`.svelte` file

## Ontology

If new TTL files are needed, use the `/new-ontology` skill.

## Tests

Changes to `eavto/` or `owl/` must maintain >80% coverage. Add `#[cfg(test)]` modules as needed.

## Steps

1. Read relevant existing files before writing anything
2. Implement bottom-up: EAVTO → OWL → Commands → Frontend
3. Run `cargo check --manifest-path src-tauri/Cargo.toml` after Rust changes
4. Verify: no layer violations, no bad logs/comments, no `.unwrap()`, formatting OK

---

**Output**: files changed, architectural decisions, anything needing user attention (migrations, new deps, config).
