---
name: mcp-change
description: Use when modifying an existing Foundation MCP tool — updating its handler logic, parameters, description, or input/output schema. Keeps the Rust template, the dispatch wiring, and the foundation:MCPTool individual in sync. NOT for creating new tools (use mcp-create) or removing them (use mcp-remove).
disable-model-invocation: false
---

# MCP Tool Change

## Common change targets
- Handler logic only → edit `src-tauri/src/ai/functions/<module>.rs`. No registry changes.
- Parameters / description → update `ToolTemplate` in `definitions.rs` AND the `foundation:MCPTool` individual.
- Output shape → update handler + `outputSchema` on the individual + (if it returns binary) the converter in `commands/chat/tool_execution.rs`.
- Read-only ↔ read-write reclassification → move dispatch arm and `is_read_only_tool` membership in `mod.rs`.
- Function name rename → all of the above plus a fresh ontology individual (treat as deprecate-old + create-new).

---

## Steps

### 1. Locate the existing tool
- Find the MCPTool individual: `search(class_iri: "foundation:MCPTool", filters: [{detail: "foundation:functionName", value: "<tool_name>"}])`. Capture the IRI.
- Find the Rust handler: `grep -n "pub fn <tool_name>" src-tauri/src/ai/functions/*.rs`.
- Find the template + dispatch: `grep -nE 'name: "<tool_name>"|"<tool_name>" =>' src-tauri/src/ai/functions/{definitions.rs,mod.rs}`.

### 2. Apply the change
- **Handler**: edit the function. Keep the signature contract (`fn(conn, args)` for read-only, `fn(conn, args, app)` for write).
- **Template**: edit the `ToolTemplate` entry in `definitions.rs` — name, description, parameters list.
- **Dispatch**: only edit `mod.rs` if the function name, module, or read/write classification changed. Read-only adds go into `is_read_only_tool`, `execute_read_only_tool`, AND `execute_tool`. Write-only goes into `execute_tool` only.
- **Binary content path**: if input/output type changes (e.g. inline base64 → page_path), update the branch in `src-tauri/src/commands/chat/tool_execution.rs`.

### 3. Update the ontology
Use `replace_property_values` on the MCPTool IRI for any field that changed:
- `foundation:functionName` (only if you renamed the tool — then also retract the old individual)
- `foundation:toolDescription` (must match `ToolTemplate.description`)
- `foundation:inputSchema` / `foundation:outputSchema` (JSON Schema strings)
- `foundation:implementedBy` (only if the implementing SoftwareFeature changed)

### 4. Verify
- `cargo check --manifest-path src-tauri/Cargo.toml` after Rust edits.
- If you changed `Cargo.toml`/profile/features/deps, use `cargo build` instead.
- The Foundation app recompiles and restarts automatically. Confirm the tool answers correctly with a fresh call.

---

## Rules

- **`foundation:functionName` MUST equal `ToolTemplate.name`** at all times. Don't change one without the other in the same edit.
- **`foundation:toolDescription` MUST equal `ToolTemplate.description`** — these strings are read by different layers and the ontology drives `AgentTask.allowedTools` UX. Drift causes confusion.
- **NEVER move a write tool into `is_read_only_tool`** — read-only tools run concurrently on the read pool; misclassification breaks consistency under load.
- **NEVER hardcode IRIs** — resolve the MCPTool IRI via `search(class_iri: "foundation:MCPTool", filters: [...])` first.
- **ALWAYS keep input/output schema in lockstep** with the handler. If a field is added in Rust, add it to the schema.
- **ALWAYS run `cargo check`** before declaring done — signature mismatches fail loudly.
- Status flips (e.g. Completed → Pending Change → Completed) follow normal entity rules — use `replace_property_values` on `foundation:hasStatus`.

## When NOT to use this skill

- Creating a brand-new tool → `/mcp-create`
- Removing a tool entirely → `/mcp-remove`
