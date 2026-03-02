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

- No `console.*` — use the logging utility
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

## Database Changes

If `eavto/` or schema files changed, flag backward compatibility and migration needs.

---

## Output

For each topic category that has issues, create a file in the `todo/` folder using the Write tool.

**Naming:** `YYYYMMDD-HHMMSS-<topic-slug>.md` — use `date +%Y%m%d-%H%M%S` to get the timestamp prefix.

**Topic slugs:**
- Architecture violations → `layer-violations`
- Rust quality (unwrap/panic) → `rust-quality`
- Frontend quality (console/any) → `frontend-quality`
- What-comments / commented-out code → `what-comments`
- Production logging → `production-logging`
- Line length (>100 cols) → `line-length`
- File size (>1000 lines) → `file-splitting`
- Test coverage → `test-coverage`
- Database compatibility → `db-compatibility`

**Each todo file must contain:**
1. Title: `# Fix: <short description>`
2. **Rule:** the violated rule in one sentence
3. Per-file sections listing exact line numbers and what to change
4. **Validation** block with shell commands to confirm the fix is complete

Only create a file for a topic if that topic has actual violations. Do not create empty or placeholder files.

After creating the todo files, print a summary: files reviewed, issues found by category, and the todo files created.
