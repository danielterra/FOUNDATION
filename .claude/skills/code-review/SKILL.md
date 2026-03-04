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

- Run `npm run build` to catch TypeScript and Svelte compilation errors
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

## Magic Numbers

All numeric literals that carry business or domain meaning must be named constants. Flag any bare number that is not:
- `0` or `1` used as neutral identity/boundary values
- Array indices in trivially obvious context
- A named constant (`const`, `static`, `let … = …` with a descriptive name)

**Rust** — scan for integer and float literals outside of trivial positions:

```
grep -n "[^a-zA-Z_][0-9]\{2,\}\|[^a-zA-Z_][0-9]\+\.[0-9]\+" src-tauri/src/**/*.rs
```

**TypeScript / Svelte** — same idea:

```
grep -n "[^a-zA-Z_][0-9]\{2,\}\|[^a-zA-Z_][0-9]\+\.[0-9]\+" src/**/*.{ts,svelte}
```

For each violation, report: file, line number, the literal value, what it likely represents, and suggest a constant name (e.g., `MAX_RETRY_COUNT`, `TOKEN_ESTIMATE_IMAGE`).

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

## Output — Report

Print a summary with:

- **Files reviewed**
- **Result**: `PASSED` if no violations were found, `FAILED` otherwise
- For each category with violations: category name, list of file:line references, and what must be fixed
