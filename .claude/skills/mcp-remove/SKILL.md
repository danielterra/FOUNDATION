---
name: mcp-remove
description: Use when permanently removing a Foundation MCP tool — retracts the foundation:MCPTool individual, deletes the Rust handler, removes the ToolTemplate registration, and removes the dispatch branch.
disable-model-invocation: false
---

# MCP Tool Remove

## Steps

### 1. Locate everything tied to the tool
- MCPTool individual: `search(class_iri: "foundation:MCPTool", filters: [{detail: "foundation:functionName", value: "<tool_name>"}])`. Capture the IRI.
- Rust handler: `grep -n "pub fn <tool_name>" src-tauri/src/ai/functions/*.rs`.
- Template + dispatch + read-only flag: `grep -nE 'name: "<tool_name>"|"<tool_name>" =>|"<tool_name>"$' src-tauri/src/ai/functions/{definitions.rs,mod.rs}`.
- Content-block converter (if it returned binary): `grep -nE '"<tool_name>"' src-tauri/src/commands/chat/tool_execution.rs`.

### 2. Confirm with the user
This deletes Rust code AND retracts an ontology individual AND breaks any `foundation:AgentTask` that lists `<tool_name>` in `foundation:allowedTools`. Mention it before proceeding.

### 3. Retract the ontology individual
```
retract_individual({ iri: "<foundation:MCPTool_...>" })
```

### 4. Delete the Rust handler
Remove the `pub fn <tool_name>(...)` function from its module file under `src-tauri/src/ai/functions/`. Also remove any helper functions only used by it. If the module becomes empty, delete the module file and its declaration in `mod.rs`.

### 5. Remove the `ToolTemplate` entry
Edit `src-tauri/src/ai/functions/definitions.rs` and delete the entire `ToolTemplate { name: "<tool_name>", ... }` block.

### 6. Remove the dispatch wiring
Edit `src-tauri/src/ai/functions/mod.rs`:
- Remove the `"<tool_name>"` arm from `is_read_only_tool` (if present).
- Remove the `"<tool_name>" => ...` arm in `execute_read_only_tool` (if present).
- Remove the `"<tool_name>" => ...` arm in `execute_tool`.

### 7. Remove special-case content-block handling (if any)
If `commands/chat/tool_execution.rs` had a branch checking for `"<tool_name>"`, remove it. If the branch was shared with another tool (e.g. `name == "read_binary_file" || name == "<tool_name>"`), drop only the removed name.

### 8. Audit `foundation:AgentTask.foundation:allowedTools`
`search(class_iri: "foundation:automation_AgentTask")` and check whether any task lists the removed tool. Use `remove_property_values` to drop it from each match. Mention findings to the user — they may want to redesign those tasks.

### 9. Verify
```bash
cargo check --manifest-path src-tauri/Cargo.toml
```
Compilation will catch a forgotten dispatch arm or template entry.

---

## Rules

- **ALWAYS confirm with the user before removing** — destructive across ontology + Rust + dispatch + agent-task allowlists.
- **NEVER skip the dispatch removal** — leaving an arm that calls a deleted function fails to compile (this is a feature, not a bug — let `cargo check` catch your misses).
- **NEVER hardcode IRIs** — resolve the MCPTool IRI via `search(class_iri: "foundation:MCPTool", filters: [...])` first.
- **ALWAYS audit `foundation:allowedTools`** in AgentTasks — silent removal here doesn't break compile but breaks live agents.
- The MCP server (`src-tauri/src/mcp/mod.rs`) auto-derives the tool list from `get_available_tools()` — no edit needed there.

## When NOT to use this skill

- Updating an existing tool → `/mcp-change`
- Creating a new tool → `/mcp-create`
