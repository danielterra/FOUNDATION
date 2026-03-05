---
name: release
description: Creates a new FOUNDATION release — bumps version, updates CHANGELOG, SoftwareRelease.ttl, README, commits, tags, and publishes a GitHub release.
disable-model-invocation: false
---

# Release FOUNDATION

## Current Version
!`grep '^version' src-tauri/Cargo.toml | head -1`

## Recent Commits (since last tag)
!`git log $(git describe --tags --abbrev=0 2>/dev/null || echo "HEAD~20")..HEAD --oneline 2>/dev/null || git log --oneline -15`

## Last Release Entry
!`tail -n 20 core-ontology/SoftwareRelease.ttl`

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

### Step 4 — Update core-ontology/SoftwareRelease.ttl

Append a new individual at the end of the file:

```turtle
foundation:FoundationRelease_X_Y_Z a foundation:SoftwareRelease , owl:NamedIndividual ;
    rdfs:label "FOUNDATION vX.Y.Z" ;
    rdfs:comment "<one-line summary>" ;
    foundation:releaseOf foundation:FoundationProduct ;
    foundation:versionNumber "X.Y.Z" ;
    foundation:licenseType "MIT" ;
    foundation:releaseDate "YYYY-MM-DD"^^xsd:date ;
    foundation:changelog "<semicolon-separated list of commit subjects>" .
```

Use today's date from the system context (`currentDate`).

### Step 5 — Query features from Foundation

Start from the product to get only features actually linked to it:

1. Call `remember_thing(iri: "foundation:FoundationProduct")` to get all `foundation:hasFeature` values.
2. For each feature IRI, call `remember_thing(iri)` to get its label, comment, and status.
3. Include features with status `foundation:Completed` or `foundation:InProgress`. Skip `foundation:Pending`.
4. Use the `comment` field as the one-line description (truncate/summarize if too long).
5. Sort entries alphabetically by label.
6. Map status to display tag:
   - `foundation:Completed` → `` `[finalizado]` ``
   - `foundation:InProgress` → `` `[em desenvolvimento]` ``

### Step 6 — Update README.md

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

### Step 7 — Commit (all files in one amend-friendly commit)

Stage all changed files **by name** (never `git add -A`):
```
git add src-tauri/Cargo.toml src-tauri/Cargo.lock package.json CHANGELOG.md core-ontology/SoftwareRelease.ttl README.md
```

Commit with:
```
chore: release vX.Y.Z
```

### Step 8 — Tag and push

```bash
git tag vX.Y.Z
git push
git push origin vX.Y.Z
```

### Step 9 — GitHub Release

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
