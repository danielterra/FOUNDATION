---
name: release-create
description: Creates a new FOUNDATION release — bumps version, updates CHANGELOG, SoftwareRelease.ttl, README, commits, tags, and publishes a GitHub release.
disable-model-invocation: false
---

# Release FOUNDATION

## Current Version
!`grep '^version' src-tauri/Cargo.toml | head -1`

## Recent Commits (since last tag)
!`git log $(git describe --tags --abbrev=0 2>/dev/null || echo "HEAD~20")..HEAD --oneline 2>/dev/null || git log --oneline -15`

## Last Release Entry
!`sqlite3 ~/Documents/Foundation/FOUNDATION.db "SELECT t_ver.object_value, t_date.object_value, t_log.object_value FROM triples t_ver JOIN triples t_date ON t_date.subject = t_ver.subject AND t_date.predicate = 'foundation:releaseDate' AND t_date.retracted = 0 LEFT JOIN triples t_log ON t_log.subject = t_ver.subject AND t_log.predicate = 'foundation:changelog' AND t_log.retracted = 0 WHERE t_ver.predicate = 'foundation:versionNumber' AND t_ver.retracted = 0 AND t_ver.subject LIKE 'foundation:FoundationRelease_%' ORDER BY t_date.object_value DESC LIMIT 1;" 2>/dev/null || echo "(no releases found)"`

---

## Instructions

### Step 1 — Determine the next version

Analyze the commits since the last tag:
- Any `feat:` commit → **minor** bump (e.g. `0.4.1` → `0.5.0`)
- Only `fix:` / `refactor:` / `chore:` commits → **patch** bump (e.g. `0.5.0` → `0.5.1`)
- Breaking changes → **major** bump

State the new version clearly before proceeding.

### Step 2 — Update version files

Update the version string in **both** files atomically (read each before editing):
- `src-tauri/Cargo.toml` — `version = "X.Y.Z"`
- `package.json` — `"version": "X.Y.Z"`

### Step 3 — Update CHANGELOG.md

Prepend a new entry at the top of `CHANGELOG.md` (after the header), following Keep a Changelog format:

```markdown
## [X.Y.Z] - YYYY-MM-DD

### Added
- ...

### Changed
- ...

### Fixed
- ...

### Refactored
- ...
```

Only include sections that have content. Derive entries from the commit list.

### Step 4 — Verify, Dump, and Verify Ontology

1. `cargo run --manifest-path scripts/verify-code-iris/Cargo.toml` — must pass with zero missing IRIs; if it fails, create missing entities via MCP before proceeding
2. `cargo run --manifest-path scripts/dump-ontology/Cargo.toml`
3. `cargo run --manifest-path scripts/verify-ontology/Cargo.toml` — must pass with zero differences
4. Include `core-ontology/ontology.sql` in the `git add` on Step 8

### Step 5 — Create SoftwareRelease individual via MCP

Use `assert_individual` to create the release, then `add_property_values` to set each property:

```
assert_individual(operations: [{
  class_iri: "foundation:SoftwareRelease",
  label: "FOUNDATION vX.Y.Z",
  comment: "<one-line summary>",
  properties: [
    {property_iri: "foundation:releaseOf",    values: ["foundation:FoundationProduct"]},
    {property_iri: "foundation:versionNumber", values: ["X.Y.Z"]},
    {property_iri: "foundation:licenseType",   values: ["MIT"]},
    {property_iri: "foundation:releaseDate",   values: ["YYYY-MM-DD"]},
    {property_iri: "foundation:changelog",     values: ["<semicolon-separated list of commit subjects>"]},
    {property_iri: "foundation:hasStatus",     values: ["foundation:Completed"]}
  ]
}])
```

Use today's date from the system context (`currentDate`).

### Step 5b — Sync MCPTool records

Ensure Foundation MCPTool records match the current implementation in `src-tauri/src/ai/functions/definitions.rs`:

1. Call `search(concept_iri: "foundation:MCPTool", limit: 50)` to list all existing records.
2. Compare against the tools returned by `get_available_tools()` in definitions.rs.
3. For each tool that is **new** (exists in code but not in Foundation):
   - Call `assert_individual` with `class_iri: "foundation:MCPTool"`, correct `toolDescription`, `inputSchema`, `outputSchema`, `implementedBy`, `functionName` (exact tool name string), `hasStatus: foundation:Completed`.
4. For each tool that is **renamed** (label mismatch): call `replace_property_values` to update `rdfs:label`, `foundation:functionName`, `toolDescription`, `inputSchema`, `outputSchema`.
5. For each tool that is **removed** (exists in Foundation but not in code): call `retract_individual`.
6. For each tool with **stale schemas**: call `replace_property_values` to update `toolDescription`, `inputSchema`, `outputSchema`.

### Step 6 — Query features from Foundation

Start from the product to get only features actually linked to it:

1. Call `describe_individual(iris: ["foundation:FoundationProduct"])` to get all `foundation:hasFeature` values.
2. For each feature IRI, call `describe_individual` to get its label, comment, and status.
3. Include features with status `foundation:Completed` or `foundation:InProgress`. Skip `foundation:Pending`.
4. Use the `comment` field as the one-line description (truncate/summarize if too long).
5. Sort entries alphabetically by label.
6. Map status to display tag:
   - `foundation:Completed` → `` `[finalizado]` ``
   - `foundation:InProgress` → `` `[em desenvolvimento]` ``

### Step 7 — Update README.md

- Update `**Version X.Y.Z**` line
- Update download badge URLs to the new version
- Update the Changelog badge version label
- If installers are not yet built, keep the download badges commented out
- Replace (or create) the `## Features` section **between the badges and `## Quick Start`** with the list from Step 5:

```markdown
## Features

- **Feature Name**: one-line description. `[finalizado]`
- **Feature Name**: one-line description. `[em desenvolvimento]`
```

One entry per feature, single sentence, status tag at the end.

### Step 8 — Commit (all files in one amend-friendly commit)

Stage all changed files **by name** (never `git add -A`):
```
git add src-tauri/Cargo.toml src-tauri/Cargo.lock package.json CHANGELOG.md core-ontology/ontology.sql README.md
```

Commit with:
```
chore: release vX.Y.Z
```

### Step 9 — Tag and push

```bash
git tag vX.Y.Z
git push
git push origin vX.Y.Z
```

### Step 10 — GitHub Release

```bash
gh release create vX.Y.Z --title "FOUNDATION vX.Y.Z" --notes "..."
```

Release notes should mirror the CHANGELOG entry in condensed form.

---

## Rules

- **Never amend a commit that has already been pushed and tagged** without explicit user request
- **Always read files before editing** them
- If `Cargo.lock` changed (it will, due to version bump), include it in the same commit
- The tag must point to the release commit — if a follow-up commit is needed, move the tag with `git tag -d` + recreate + `git push origin --force vX.Y.Z`
- Confirm the final version with the user if the bump type is ambiguous
