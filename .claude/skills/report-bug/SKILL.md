---
name: report-bug
description: Investigates a bug, gathers evidence from logs and code, and creates a Foundation Issue record
disable-model-invocation: true
argument-hint: "<description of the bug — omit to auto-detect from logs>"
---

# Report Bug: $ARGUMENTS

## Recent Errors
!`npm run logs 500 2>/dev/null | grep -i "error\|panic\|exception\|failed" | tail -n 30 || echo "No recent errors found"`

## Compilation
!`cargo check --manifest-path src-tauri/Cargo.toml --message-format=short 2>&1 | head -n 30`

---

## Steps

1. **Identify the bug** from `$ARGUMENTS`, the error log above, and the compilation output.
   If `$ARGUMENTS` is empty, derive the bug title and description from the most prominent error.

2. **Gather context** — call `remember_things` with a relevant `concept_iri` or `query` to inspect
   data or recent AI conversation messages related to the bug.

3. **Search for related code** — use Grep on key terms from the error to locate the affected area.

4. **Summarise findings** — compose:
   - A short bug title (one line)
   - A detailed description (what fails, when, observed vs expected)
   - Relevant error excerpts (copy from log output above)
   - Steps to reproduce (if determinable)
   - Affected files (from code search)

## Create the Issue via MCP

Call `learn_thing` with `concept_iri=foundation:Issue`, label = short bug title. The IRI is auto-generated.

Then call `learn_thing_detail` for the returned IRI:
- `foundation:description` → detailed description + observed error excerpts (value_type: literal)
- `foundation:stepsToReproduce` → reproduction steps if known (value_type: literal)
- `foundation:affectedFiles` → comma-separated list of affected file paths (value_type: literal)
- `foundation:issueStatus` → `foundation:Pending` (value_type: iri)
- `foundation:reportedAt` → ISO datetime (datatype: xsd:dateTime)

---

**Output**: print the created Issue IRI (e.g. `foundation:Issue_1772475705471`) so the user can
pass it directly to `/bug-fix`.
