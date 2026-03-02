---
name: bug-fix
description: Fixes a bug tracked as a Foundation Issue record
disable-model-invocation: true
argument-hint: "<foundation:Issue_IRI>"
---

# Bug Fix: $ARGUMENTS

Call `remember_thing` with `$ARGUMENTS` to retrieve the full issue description, affected files,
and reproduction steps. Use this as the primary source of truth for the bug.

---

1. **Analyse** — read the issue description, error excerpts, and affected files
2. **Search code** — use Grep on key terms and file paths from the issue
3. **Identify root cause** — trace the failure through the layer architecture
4. **Fix** — follow the layer order: Commands → OWL → EAVTO (never bypass layers)
5. **Validate** — run `cargo check --manifest-path src-tauri/Cargo.toml --message-format=short 2>&1 | head -n 30`
6. **Report** — call `learn_thing_detail` on `$ARGUMENTS`:
   - `foundation:issueStatus` → `foundation:Completed` (value_type: iri)
   - `foundation:causeAnalysis` → description of the root cause identified
   - `foundation:resolution` → description of the fix applied, including files changed
7. **Output** — present the same report to the user: root cause, files changed, fix applied.
