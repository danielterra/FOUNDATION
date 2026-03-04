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

1. **Start** — call `learn_thing_detail` on `$ARGUMENTS`:
   - `foundation:issueStatus` → `foundation:InProgress` (value_type: iri)
   - `foundation:startedAt` → current ISO datetime (datatype: xsd:dateTime)
2. **Analyse** — read the issue description, error excerpts, and affected files
3. **Search code** — use Grep on key terms and file paths from the issue
4. **Identify root cause** — trace the failure through the layer architecture
5. **Fix** — follow the layer order: Commands → OWL → EAVTO (never bypass layers)
6. **Validate** — run `cargo check --manifest-path src-tauri/Cargo.toml --message-format=short 2>&1 | head -n 30`
7. **Code Review** — invoke the `/code-review` skill on the changed files and resolve any issues found before proceeding
8. **Report** — call `learn_thing_detail` on `$ARGUMENTS`:
   - On success: `foundation:issueStatus` → `foundation:Completed` (value_type: iri)
   - On failure (cannot be fixed): `foundation:issueStatus` → `foundation:Failed` (value_type: iri)
   - `foundation:causeAnalysis` → description of the root cause identified
   - `foundation:resolution` → description of the fix applied or reason it could not be completed, including files changed
9. **Output** — present the same report to the user: root cause, files changed, fix applied (or blocking reason).
