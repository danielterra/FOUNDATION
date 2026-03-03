---
name: code-review
description: Reviews staged changes or specified files for quality and best practices
disable-model-invocation: true
argument-hint: "<files...>"
---

# Code Review

## Changed Files
!`git diff --cached --name-only && git diff --name-only | sort -u`

Files to review: $ARGUMENTS (if empty, review all changed files above)

---

## Architecture (Rust)

- `src-tauri/src/owl/` MUST use `eavto::` — never `sqlx::` or `rusqlite::` directly
- `src-tauri/src/commands/` must NOT use `eavto::` — must go through OWL

```
grep -n "sqlx::\|rusqlite::" src-tauri/src/owl/*.rs
grep -n "eavto::" src-tauri/src/commands/*.rs
```

## Rust Quality

- Run `cargo check --manifest-path src-tauri/Cargo.toml`
- No `.unwrap()` — use `?` or `map_err`
- No `panic!` — use `Result`

## Frontend Quality

- No `console.log` / `console.debug` / `console.info` — verbose logging not appropriate for production
- `console.error` and `console.warn` are acceptable: `src/lib/logging.js` globally wraps them to forward to the backend log file
- No `: any` / `: unknown` — use specific types

## Comments

Only *why* comments are acceptable. Flag:
- Comments describing *what* the code does
- Commented-out code blocks
- TODO/FIXME/HACK/XXX markers

```
grep -n "^[[:space:]]*//" src-tauri/src/**/*.rs
grep -n "//.*TODO\|//.*FIXME\|//.*HACK\|//.*XXX" src-tauri/src/**/*.rs
```

## Semantic Colors (Svelte)

All colors must use CSS variables from `src/lib/colors.css`. Flag any hardcoded color value.

```
grep -n "#[0-9a-fA-F]\{3,6\}\|rgb(\|rgba(" src/**/*.{ts,svelte}
```

## Logging

Flag `debug!`, `trace!`, `println!`, `eprintln!`, `console.log/debug` in production code. Acceptable: `error!`, `warn!`, key lifecycle events.

```
grep -n "debug!\|trace!\|println!\|eprintln!" src-tauri/src/**/*.rs
grep -n "console\.\(log\|debug\|info\)" src/**/*.{ts,svelte}
```

## Formatting

- Max 100 columns per line
- Max 1000 lines per `.rs` or `.svelte` file

```
awk 'length > 100 {print FILENAME ":" NR ": " length " cols"}' src-tauri/src/**/*.rs src/**/*.{ts,svelte}
wc -l src-tauri/src/**/*.rs src/**/*.svelte | awk '$1 > 1000 {print $2 ": " $1 " lines"}'
```

## Test Coverage

If `eavto/` or `owl/` files changed, verify >80% coverage (blocking if not met):

```
cargo tarpaulin --manifest-path src-tauri/Cargo.toml \
  --include-files "src/eavto/*" "src/owl/*" --out Stdout 2>&1 | tail -n 20
```

## Output — Create Issues in Foundation via MCP

For each category that has violations, call `learn_thing` to create a `foundation:Issue` instance:

- `concept_iri`: `foundation:Issue`
- `label`: `"<Category>: <brief description>"` — e.g., `"Layer violation in commands/widget.rs"`
- `icon`: `bug_report`
- `comment`: full description with file paths, line numbers, and what to change

Then call `learn_thing_detail`:
- `foundation:issueType` → `"bug"` (value_type: literal)
- `foundation:issueStatus` → `foundation:Pending` (value_type: iri)

One Issue per category with violations. Use a clear label that names the category and the most affected file, e.g. `"Layer violation in commands/widget.rs"` or `"Unwrap calls in owl/individual.rs"`. Only create Issues for categories that have actual violations.

After creating Issues, print a summary: files reviewed, issues found by category, and the IRIs of created Issue instances.
