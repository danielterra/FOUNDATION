---
name: develop-feature
description: Implements a User Story by planning tasks, executing each one, and marking completion
disable-model-invocation: true
argument-hint: "<foundation:UserStory_IRI>"
---

# Develop Feature: $ARGUMENTS

Call `remember_thing` with `$ARGUMENTS` to retrieve the full User Story: capability, benefit,
acceptance criteria, and related plans/tasks. Use this as the primary source of truth.

---

## Phase 1 — Understand

1. Read the User Story from `$ARGUMENTS`
2. Call `remember_things` with `concept_iri=foundation:Task` and filter by tasks that have
   `foundation:contributesTo` pointing to any Plan that has `foundation:contributesTo` = `$ARGUMENTS`
   — alternatively, trace the chain: UserStory → Plans → Tasks
3. For each Plan linked to this story (`foundation:contributesTo` = `$ARGUMENTS`), call
   `remember_thing` on the Plan to read its overview, risks, and tasks
4. Order tasks respecting `foundation:dependsOn` relationships (topological sort)
5. Set the User Story status to In Progress:
   - `learn_thing_detail` on `$ARGUMENTS`: `foundation:storyStatus` → `foundation:InProgress` (value_type: iri)

---

## Phase 2 — Explore Codebase

Before planning tasks, orient yourself:

- Run `git status --short` and `git log --oneline -5` to understand the current state
- Explore the project structure and identify the relevant files and modules
- Identify existing patterns to follow (naming, file structure, layer conventions)

---

## Phase 3 — Create Tasks

Decompose the implementation into concrete, ordered tasks. Design bottom-up: data/ontology layer
first, then business logic, then API/commands, then frontend.

For each task, call `learn_thing` with `concept_iri=foundation:Task`, then set details via
`learn_thing_detail`:
- `foundation:description` → what to implement (files, functions, endpoints, etc.)
- `foundation:notes` → path/to/file — if applicable
- `foundation:status` → `foundation:Pending` (value_type: iri)
- `foundation:dependsOn` → previous Task IRI (value_type: iri) — for ordering
- `foundation:contributesTo` → Plan IRI (value_type: iri) — link to the relevant Plan of the story

If no Plans exist on the UserStory yet, create one first using `learn_thing` with the appropriate
plan concept (`foundation:BackendArchitecturePlan`, `foundation:FrontendArchitecturePlan`, etc.)
and set `foundation:contributesTo` → `$ARGUMENTS` (value_type: iri).

---

## Phase 4 — Execute Tasks

For each task (in dependency order):

### 3a. Mark task In Progress
Call `learn_thing_detail` on the Task IRI:
- `foundation:status` → `foundation:InProgress` (value_type: iri)

### 3b. Implement
- Read all files the task touches before editing
- Follow the layer order: Ontology → EAVTO → OWL → Commands → Frontend
- Follow existing conventions — do not introduce new patterns unless required
- Keep changes minimal and focused on the task description

## Comments

Only *why* comments are acceptable. Flag:
- Comments describing *what* the code does
- Commented-out code blocks
- TODO/FIXME/HACK/XXX markers

## Logging

Flag `debug!`, `trace!`, `println!`, `eprintln!`, `console.log/debug` in production code. Acceptable: `error!`, `warn!`, key lifecycle events.

## Semantic Colors

Never use raw hex, rgb, or rgba values in Svelte components. All colors must use CSS variables
defined in `src/lib/colors.css`. Each variable carries a semantic meaning (interactive, danger,
warning, neutral, transition) — choose based on intent, not appearance. Flag any hardcoded color
as a violation.

## Formatting

- Max 100 columns per line
- Max 1000 lines per `.rs` or `.svelte` file

## Test Coverage

If `eavto/` or `owl/` files changed, verify >80% coverage (blocking if not met):

```
cargo tarpaulin --manifest-path src-tauri/Cargo.toml \
  --include-files "src/eavto/*" "src/owl/*" --out Stdout 2>&1 | tail -n 20
```

### 3c. Validate
Run: `cargo check --manifest-path src-tauri/Cargo.toml --message-format=short 2>&1 | head -n 40`

Fix any compilation errors before proceeding to the next task.

### 3d. Mark task Completed
Call `learn_thing_detail` on the Task IRI:
- `foundation:status` → `foundation:Completed` (value_type: iri)
- `foundation:resolution` → one-sentence summary of what was implemented (value_type: literal)

---

## Phase 5 — Complete User Story

After all tasks are marked Completed:

1. Verify the acceptance criteria from the User Story are met
2. Call `learn_thing_detail` on `$ARGUMENTS`:
   - `foundation:storyStatus` → `foundation:Completed` (value_type: iri)
3. Present a summary to the user:
   - User Story implemented
   - Tasks completed (with brief description of each)
   - Files changed
   - Any remaining risks or follow-up items
