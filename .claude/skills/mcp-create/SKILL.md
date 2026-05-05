---
name: mcp-create
description: Creates a new Foundation MCP tool — implements the Rust handler, registers it in the tool template registry, dispatches it from the executor, and creates the foundation:MCPTool individual in the ontology.
disable-model-invocation: false
---

# MCP Tool Create

## Existing tool definitions (handler signatures)
!`grep -nE 'pub fn [a-z_]+\(conn: &(?:mut )?Connection' src-tauri/src/ai/functions/*.rs | head -40`

## Tool template + dispatch wiring
!`grep -nE 'name: "[a-z_]+"\.to_string|"[a-z_]+" => (file::|individual::|class::|property::)' src-tauri/src/ai/functions/definitions.rs src-tauri/src/ai/functions/mod.rs | head -40`

---

## Instructions

You are creating a new MCP tool exposed by the Foundation MCP server. A tool is a Rust function callable over JSON-RPC by external MCP clients (Claude Code, Claude Desktop) and by the internal chat agent. Each tool has an entry in the runtime registry AND a `foundation:MCPTool` individual in the ontology that documents the contract.

Start by calling `search(class_iri: "foundation:MCPTool")` to see all existing tools and avoid duplicates.

If the user hasn't specified all details, ask for:
- **Tool name** — snake_case identifier (e.g. `read_pdf_page`, `summarise_thread`); used as the JSON-RPC `name`
- **Purpose** — one-sentence description shown to the AI
- **Inputs** — name, type, description, required/optional for each parameter
- **Output** — JSON shape returned in `ToolResult.result`
- **Read-only?** — does it mutate the triple store or filesystem? Read-only tools run on the read pool.

---

### Step 1 — Implement the handler in Rust

Pick the right module under `src-tauri/src/ai/functions/`:
- `file.rs` for filesystem / binary content
- `individual.rs` for individuals (search, describe, assert, mutate)
- `class.rs` for class operations
- `property.rs` for property operations
- new module if none fits

Handler signature:
```rust
pub fn <tool_name>(conn: &Connection, args: &Value) -> ToolResult { ... }
// or for write tools that need the AppHandle for events:
pub fn <tool_name>(conn: &mut Connection, args: &Value, app: Option<&tauri::AppHandle>) -> ToolResult { ... }
```

Validate every required parameter and return `ToolResult { success: false, error: Some(...), .. }` on missing/invalid input. Use `super::ToolResult` and `serde_json::Value`.

If the tool returns large binary data (PDF, image > ~10 KB), persist to `std::env::temp_dir().join("foundation-<purpose>")` and return a path field — never inline base64 in the JSON. The chat layer loads lazily. Pattern to follow: `read_pdf_page` in `file.rs`.

---

### Step 2 — Register the template in `definitions.rs`

Add a `ToolTemplate` entry inside `get_available_tools()`:

```rust
ToolTemplate {
    name: "<tool_name>".to_string(),
    array_mode: false,           // true if input wraps `operations: [...]`
    description: "<purpose — shown to the AI>".to_string(),
    parameters: vec![
        Parameter {
            name: "<param>".to_string(),
            param_type: "string".to_string(),  // "string" | "integer" | "boolean" | "array" | "object"
            description: "<param description>".to_string(),
            required: true,
            schema: None,
        },
    ],
},
```

---

### Step 3 — Wire the dispatch in `mod.rs`

For **read-only** tools, add the name to `is_read_only_tool` and the dispatch arm in `execute_read_only_tool` AND `execute_tool`:
```rust
"<tool_name>" => <module>::<tool_name>(conn, args),
```

For **write** tools, add only to `execute_tool`:
```rust
"<tool_name>" => <module>::<tool_name>(conn, args, app),
```

The MCP server in `src-tauri/src/mcp/mod.rs` picks up new tools automatically through `get_available_tools()` — no edit needed there.

---

### Step 4 — Special handling for binary content

If the tool returns image/PDF for the chat agent, edit `src-tauri/src/commands/chat/tool_execution.rs` to convert the result into a content-block array (image / document) so Claude receives a native vision/document block. Pattern: see the `read_binary_file` / `read_pdf_page` branch.

---

### Step 5 — Verify compilation

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

If you touched `Cargo.toml`/profile/features/deps, run `cargo build` instead — `cargo check` doesn't share codegen cache with `cargo build`.

---

### Step 6 — Register the `foundation:MCPTool` in the ontology

```
assert_individual(operations: [{
  class_iri: "foundation:MCPTool",
  label: "<tool_name>",
  comment: "<purpose>",
  icon: "build",
  properties: [
    { property_iri: "foundation:functionName",    values: ["<tool_name>"] },
    { property_iri: "foundation:toolDescription", values: ["<full description matching the ToolTemplate>"] },
    { property_iri: "foundation:inputSchema",     values: ["<JSON Schema as a string>"] },
    { property_iri: "foundation:outputSchema",    values: ["<JSON Schema as a string>"] },
    { property_iri: "foundation:implementedBy",   values: ["<foundation:SoftwareFeature_... IRI>"] },
    { property_iri: "foundation:hasStatus",       values: ["foundation:Completed"] }
  ]
}])
```

Pick the implementing `foundation:SoftwareFeature` by searching for related tools in the same module — reuse the existing one when the new tool extends an existing feature.

`foundation:functionName` MUST match exactly the `name` in the `ToolTemplate` — `AgentTask.allowedTools` filters by this string.

---

### Step 7 — Reload

The Foundation app recompiles and restarts automatically on file save. Wait for the MCP to reconnect, then call the new tool to confirm it works end-to-end.

---

## Rules

- **`functionName` == `ToolTemplate.name`** — these strings are joined by AgentTask filtering. Mismatch silently disables the tool for the agent.
- **Read-only flag is load-bearing** — read tools run concurrently on the read pool; write tools serialise. Misclassifying a write tool as read-only causes silent corruption under concurrency.
- **Never inline large binaries in the JSON result** — persist to `std::env::temp_dir()` and return a path. The MCP transport relays results as text content blocks; large base64 strings overflow the client's token budget.
- **Always run `cargo check`** after editing — handler signature mismatches fail loudly here.
- **Always include `foundation:hasStatus`** when creating the MCPTool individual — every entity needs a status.
- **Follow `entity-action` skill naming** when creating sibling tools/skills.

## When NOT to use this skill

- Updating an existing tool's signature, schema, or description → `/mcp-change`
- Removing a tool entirely → `/mcp-remove`
