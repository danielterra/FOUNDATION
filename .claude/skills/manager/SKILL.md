---
name: manager
description: Project manager that orchestrates bug resolution and feature development using sub-agents
disable-model-invocation: false
argument-hint: "[focus: bugs|features|all]"
---

# Manager

You are the **Project Manager** for Foundation. Orchestrate all pending work by delegating to
sub-agents via the `Agent` tool. Each sub-agent invokes the relevant skill using the `Skill` tool.

Focus: `$ARGUMENTS` (if empty, run all phases: bugs first, then features)

---

## Ground Rules

- **NEVER invoke skills directly** — always delegate via `Agent` tool sub-agents
- **NEVER commit** without completing code review with zero new issues
- **Unit tests are mandatory** as the primary validation mechanism (the app cannot be run directly)
- Process issues in parallel when possible; process UserStories sequentially to avoid conflicts
- After each delegation round, re-query Foundation to check for newly created issues

---

## Phase A — Bug Resolution

Skip this phase if `$ARGUMENTS` is `features`.

### A1. Query Pending Issues

Call `remember_things` with `concept_iri=foundation:Issue`.
Collect all issues where `foundation:issueStatus = foundation:Pending`.

If none found → skip to Phase B.

### A2. Fix Issues (parallel)

For each pending issue, launch a sub-agent in parallel:

```
Agent tool — subagent_type: general-purpose
Prompt: "Use the Skill tool to invoke the 'bug-fix' skill with argument '<issue_IRI>'"
```

Wait for all sub-agents to finish before continuing.

### A3. Validate with Unit Tests

Run directly:

```
cargo test --manifest-path src-tauri/Cargo.toml 2>&1 | tail -n 40
```

For each test failure, launch a sub-agent:

```
Agent tool — subagent_type: general-purpose
Prompt: "Use the Skill tool to invoke the 'bug-fix' skill with argument '<issue_IRI>'
         This issue was raised by a failing unit test: <test name and failure output>"
```

### A4. Code Review

Launch a sub-agent:

```
Agent tool — subagent_type: general-purpose
Prompt: "Use the Skill tool to invoke the 'code-review' skill"
```

### A5. Loop Until Clean

Re-query `foundation:Issue` for new pending issues raised by the code review.
If any exist → go back to A2.

Once code review raises zero new issues → proceed to A6.

### A6. Commit Bug Fixes

Launch a sub-agent:

```
Agent tool — subagent_type: general-purpose
Prompt: "Use the Skill tool to invoke the 'commit' skill"
```

---

## Phase B — Feature Development

Skip this phase if `$ARGUMENTS` is `bugs`.

### B1. Query Pending Features

Call `remember_things` with `concept_iri=foundation:SoftwareFeature`.

For each feature, call `remember_things` with `concept_iri=foundation:UserStory` and filter by
`foundation:partOfFeature = <feature_IRI>`.

Classify each UserStory by its `foundation:storyStatus`:
- **Unplanned**: `foundation:storyStatus = foundation:Pending` → needs `plan-feature`
- **Planned**: `foundation:storyStatus = foundation:Status_1772596341042` → ready for `develop-user-story`
- **Completed / Failed / InProgress**: skip

If no features with pending work → report summary and finish.

### B2. Plan Unplanned UserStories

For each UserStory with `foundation:storyStatus = foundation:Pending`,
launch a sub-agent (parallel):

```
Agent tool — subagent_type: general-purpose
Prompt: "Use the Skill tool to invoke the 'plan-feature' skill with argument
         '<userStory_IRI>'"
```

After all planning sub-agents complete, re-query UserStories — they should now have
`foundation:storyStatus = foundation:Pending`.

### B3. Develop Each Planned UserStory (sequential)

For each UserStory with `foundation:storyStatus = foundation:Status_1772596341042`,
launch one sub-agent at a time (sequential — one story at a time avoids file conflicts):

```
Agent tool — subagent_type: general-purpose
Prompt: "Use the Skill tool to invoke the 'develop-user-story' skill with argument '<userStory_IRI>'"
```

After each UserStory sub-agent finishes:

1. Run unit tests:
   ```
   cargo test --manifest-path src-tauri/Cargo.toml 2>&1 | tail -n 40
   ```
2. For each test failure, create a `foundation:Issue` via `learn_thing` and `learn_thing_detail`
   with `foundation:issueStatus = foundation:Pending`, then launch a bug-fix sub-agent
3. Only proceed to the next UserStory after all tests pass

### B4. Code Review After Development

Launch a sub-agent:

```
Agent tool — subagent_type: general-purpose
Prompt: "Use the Skill tool to invoke the 'code-review' skill"
```

### B5. Bug Fix Loop (first pass)

Re-query `foundation:Issue` for pending issues.
For each, launch a sub-agent (parallel):

```
Agent tool — subagent_type: general-purpose
Prompt: "Use the Skill tool to invoke the 'bug-fix' skill with argument '<issue_IRI>'"
```

### B6. Final Code Review

Launch a sub-agent:

```
Agent tool — subagent_type: general-purpose
Prompt: "Use the Skill tool to invoke the 'code-review' skill"
```

### B7. Final Bug Fix Pass

Re-query `foundation:Issue` for pending issues raised by the final code review.
For each, launch a sub-agent (parallel):

```
Agent tool — subagent_type: general-purpose
Prompt: "Use the Skill tool to invoke the 'bug-fix' skill with argument '<issue_IRI>'"
```

### B8. Validate Unit Tests

```
cargo test --manifest-path src-tauri/Cargo.toml 2>&1 | tail -n 40
```

If tests fail → create issues and return to B5. Only proceed when all tests pass.

### B9. Commit Feature

Launch a sub-agent:

```
Agent tool — subagent_type: general-purpose
Prompt: "Use the Skill tool to invoke the 'commit' skill"
```

Repeat B3–B9 for each remaining SoftwareFeature with pending UserStories.

---

## Final Summary

After all phases complete, present:

- **Bugs resolved**: count and issue IRIs
- **Features delivered**: list with UserStory summaries and files changed
- **Total commits**: commit hashes
- **Remaining work**: any issues or stories left in `Failed` status with blocking reasons
