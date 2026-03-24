# AI Assistant Guidelines for FOUNDATION Project

This document contains specific instructions for AI assistants working on the FOUNDATION project.

## 📋 Meta Rule - Document Maintenance

**⚠️ CRITICAL RULE: Whenever the user corrects you or indicates a preference, IMMEDIATELY UPDATE this CLAUDE.md document to record the correction/preference.**

- When you receive negative feedback about an action taken → add an explicit rule
- When the user indicates "always do X" or "never do Y" → document it here
- When there is a behavior correction → create an appropriate section if necessary
- This document should evolve continuously with the user's preferences
- Always confirm to the user when you update this document

## Debugging Guide

### 🔍 Investigation Workflow

When debugging or investigating problems, follow this systematic approach:

1. **Check the centralized logs** (frontend + backend combined)
2. **Query message history** to understand recent interactions
3. **Inspect the database** for data-related issues
4. **Review the code** only after gathering context from logs and data

**⚠️ CRITICAL: Don't ask questions you can find the answer to yourself** - directly investigate logs, database, and code before asking the user.

### 📋 Application Logs

- **ALWAYS consult the centralized logs when investigating problems**
- All frontend and backend errors are logged centrally in one place
- Logs include timestamps, log levels, and full stack traces

**Commands:**
```bash
npm run logs          # View last 100 lines
npm run logs 500      # View last 500 lines
npm run logs 1000     # View last 1000 lines for deeper investigation
```

**Direct log access:**
```bash
tail -f ~/Library/Application\ Support/org.w3id.foundation/application.log
```

### 💬 Message History

Chat messages are stored as RDF triples in the `triples` table using the `foundation:AIConversationMessage` class.

**Triple properties per message:**
- `rdf:type = foundation:AIConversationMessage`
- `foundation:role`: `'user'` or `'assistant'` (literal in `object_value`)
- `foundation:content`: message content JSON (literal in `object_value`)
- `foundation:sentAt`: Unix timestamp in milliseconds — stored as `object_datetime` (integer), NOT `object_value`
- `foundation:partOfConversation`: conversation IRI (in `object`)
- `foundation:sender` / `foundation:receiver`: participant IRIs (in `object`)

**Common queries:**

```sql
-- View latest messages across all conversations
sqlite3 ~/Documents/Foundation/FOUNDATION.db "
SELECT datetime(t_time.object_datetime / 1000, 'unixepoch', 'localtime') as time,
       t_role.object_value as role,
       substr(t_content.object_value, 1, 80) || '...' as preview
FROM triples t_time
JOIN triples t_role ON t_role.subject = t_time.subject
  AND t_role.predicate = 'foundation:role' AND t_role.retracted = 0
JOIN triples t_content ON t_content.subject = t_time.subject
  AND t_content.predicate = 'foundation:content' AND t_content.retracted = 0
WHERE t_time.predicate = 'foundation:sentAt' AND t_time.retracted = 0
ORDER BY t_time.object_datetime DESC
LIMIT 20;"

-- View messages from a specific conversation (look up the conversation IRI first via search)
sqlite3 ~/Documents/Foundation/FOUNDATION.db "
SELECT datetime(t_time.object_datetime / 1000, 'unixepoch', 'localtime') as time,
       t_role.object_value as role,
       substr(t_content.object_value, 1, 100) || '...' as preview
FROM triples t_time
JOIN triples t_conv ON t_conv.subject = t_time.subject
  AND t_conv.predicate = 'foundation:partOfConversation' AND t_conv.retracted = 0
JOIN triples t_role ON t_role.subject = t_time.subject
  AND t_role.predicate = 'foundation:role' AND t_role.retracted = 0
JOIN triples t_content ON t_content.subject = t_time.subject
  AND t_content.predicate = 'foundation:content' AND t_content.retracted = 0
WHERE t_time.predicate = 'foundation:sentAt' AND t_time.retracted = 0
  AND t_conv.object = '<conversation_iri>'
ORDER BY t_time.object_datetime;"

-- Find messages containing specific text
sqlite3 ~/Documents/Foundation/FOUNDATION.db "
SELECT datetime(t_time.object_datetime / 1000, 'unixepoch', 'localtime') as time,
       t_role.object_value as role,
       t_content.object_value as content
FROM triples t_time
JOIN triples t_role ON t_role.subject = t_time.subject
  AND t_role.predicate = 'foundation:role' AND t_role.retracted = 0
JOIN triples t_content ON t_content.subject = t_time.subject
  AND t_content.predicate = 'foundation:content' AND t_content.retracted = 0
WHERE t_time.predicate = 'foundation:sentAt' AND t_time.retracted = 0
  AND t_content.object_value LIKE '%search_term%'
ORDER BY t_time.object_datetime DESC
LIMIT 10;"

-- Count messages by role
sqlite3 ~/Documents/Foundation/FOUNDATION.db "
SELECT object_value as role, COUNT(DISTINCT subject) as count
FROM triples
WHERE predicate = 'foundation:role' AND retracted = 0
  AND subject IN (
    SELECT subject FROM triples
    WHERE predicate = 'rdf:type' AND object = 'foundation:AIConversationMessage' AND retracted = 0
  )
GROUP BY object_value;"
```

### 🗄️ Database Inspection

**Always use SELECT queries** to inspect data before making assumptions:

```sql
-- List all tables
sqlite3 ~/Documents/Foundation/FOUNDATION.db ".tables"

-- View table schema
sqlite3 ~/Documents/Foundation/FOUNDATION.db ".schema table_name"

-- Count triples
sqlite3 ~/Documents/Foundation/FOUNDATION.db "SELECT COUNT(*) FROM triples;"

-- View recent triples
sqlite3 ~/Documents/Foundation/FOUNDATION.db "
SELECT subject, predicate, COALESCE(object, object_value) as value
FROM triples
ORDER BY rowid DESC
LIMIT 20;"
```

## Foundation MCP Tools

Current tools (18 total): `define_class`, `define_property`, `assert_individual`, `add_property_values`, `replace_property_values`, `remove_property_values`, `clear_property`, `retract_individual`, `retract_class`, `retract_property`, `search`, `describe_class`, `describe_individual`, `describe_property`, `class_graph`, `get_process`, `run_process`, `blackboard_update`

- **ALWAYS use MCP tools** to interact with Foundation data
- **NEVER access the database directly** via SQL INSERT/UPDATE/DELETE — always go through MCP tools
- If MCP tools are not available (app not running), report findings and wait for the user to start the app

### Looking Up Foundation IRIs

**⚠️ CRITICAL RULE: NEVER deduce or guess IRIs — ALWAYS look them up using MCP tools.**

- IRIs like `foundation:Planned`, `foundation:Active`, etc. do NOT necessarily exist — never assume
- To find individuals by property value: `search(concept_iri: "foundation:Status", filters: [{detail: "rdfs:label", value: "<label>"}])`
- To find any individual by label or property: use `search` with a `filters` parameter
- **NEVER use SQL SELECT to find IRIs** — use the appropriate MCP tool instead
- **NEVER hardcode an IRI without first confirming it exists via MCP**

## Database & Storage

- **User database**: `~/Documents/Foundation/FOUNDATION.db`
- **Application logs**: `~/Library/Application Support/org.w3id.foundation/application.log` (macOS)
- For direct SQL queries: `sqlite3 ~/Documents/Foundation/FOUNDATION.db "SELECT ..."`
- **⚠️ INVIOLABLE RULE: NEVER, UNDER ANY CIRCUMSTANCES, DELETE THE DATABASE (`rm ~/Documents/Foundation/FOUNDATION.db`)**
- **⚠️ NEVER execute commands that modify the database (UPDATE, DELETE, DROP, TRUNCATE, INSERT) without explicit user confirmation**
- Only SELECT queries are allowed without prior confirmation
- ALWAYS ask the user before modifying any data in the database

### `triples` Table Structure

**⚠️ IMPORTANT:** The `triples` table has a specific structure that you MUST understand to avoid mistakes:

- **`object`**: Contains IRIs or blank nodes when `object_type = 'iri'` or `'blank'`
- **`object_value`**: Contains the lexical value of literals when `object_type = 'literal'`
- **`object_datatype`**: Type of the literal (e.g., `xsd:string`, `xsd:integer`, `xsd:dateTime`)

**When writing SQL queries:**
```sql
-- ❌ WRONG - Will return empty for literals
SELECT predicate, object FROM triples WHERE subject = 'foundation:File_123'

-- ✅ CORRECT - Returns both IRIs and literal values
SELECT predicate, object, object_value, object_type FROM triples WHERE subject = 'foundation:File_123'
```

**Practical examples:**
- `foundation:fileName` is a literal → value is in `object_value`, `object` is NULL
- `foundation:hasFileType` is an IRI → value is in `object`, `object_value` is NULL
- If you want the value regardless of type, use `COALESCE(object, object_value)`

## Project Structure

- **Frontend**: Svelte + TypeScript (src/)
- **Backend**: Rust + Tauri (src-tauri/)
- **Ontology**: `core-ontology/ontology.sql` (source of truth, generated by `scripts/dump-ontology`)
- **Database**: SQLite with RDF triples

## Development Commands

- `npm run tauri dev` - Start development server
- `npm run logs` - View application logs (last 100 lines)
- `npm run logs N` - View last N lines of logs
- `cargo check --manifest-path src-tauri/Cargo.toml` - Check Rust code
- `cargo build --manifest-path src-tauri/Cargo.toml` - Build Rust code

**⚠️ EXECUTION RULES:**
- **NEVER run `npm run tauri dev` or `npm run build`** - the user always runs this in their terminal
- **NEVER kill Tauri processes** (pkill, killall, etc.) - the user manages this
- Only run `cargo check` to validate Rust code
- The user is responsible for starting and stopping the development server

## Generated Files — Do Not Edit

**⚠️ CRITICAL RULE: NEVER manually edit `core-ontology/ontology.sql`.**

- `ontology.sql` is auto-generated by `scripts/dump-ontology/src/main.rs`, which dumps all ontology triples from the live Foundation database (`~/Documents/Foundation/FOUNDATION.db`)
- It is called as part of the release workflow (`scripts/release/src/main.rs`) and committed to the repo
- The file is embedded into the Tauri binary at compile time via `include_str!()` in `src-tauri/src/eavto/connection.rs`
- Any manual changes will be overwritten on the next release
- To change ontology content: use MCP tools (`define_class`, `define_property`, `assert_individual`, `retract_property`, etc.) on the live database — the dump will pick up the changes at release time

## Version Management & Releases

- **⚠️ When generating a new release/version of the project:**
  1. Use MCP tools to create a new `foundation:SoftwareRelease` individual with: label, comment, releaseOf (`foundation:FoundationProduct`), versionNumber, licenseType, releaseDate, changelog
  2. Run `scripts/dump-ontology` to regenerate `core-ontology/ontology.sql`
  3. Update the version in `src-tauri/Cargo.toml`
  4. Update the version in `package.json`

## TODO Documentation

When creating instruction documents in the `todo/` folder:

- **Naming pattern:** `YYYYMMDD-HHMMSS-file-name.md`
  - Example: `20260228-192519-layer-violations-fix.md`
  - Use hyphen (-) to separate all parts
  - Timestamp first, descriptive name last
  - Timestamp format: `YYYYMMDD-HHMMSS`

## AI Function Design

### Simplicity Principle
- **Avoid redundant functions**: If a function can be replaced by simple calls to other functions, it is not necessary
- **Removed/merged tools** (no longer exist as separate tools):
  - `learn_thing` / `learn_thing_detail` / `learn_things` → replaced by `assert_individual`, `add_property_values`, `replace_property_values`
  - `learn_concept` / `learn_concepts` → renamed to `define_class`
  - `learn_connection_type` / `learn_properties` → renamed to `define_property`
  - `remember_thing` / `remember_things` / `remember` → renamed to `search`
  - `remember_concept` / `remember_concepts` / `get_concepts` → renamed to `describe_class`
  - `remember_properties` → renamed to `describe_property`
  - `forget_things` → split into `retract_individual`, `remove_property_values`, `clear_property`
  - `forget_concepts` → renamed to `retract_class`
  - `forget_properties` → renamed to `retract_property`
  - `get_things` → renamed to `describe_individual`
  - `get_concept_graph` → renamed to `class_graph`
  - `remember_concept_tree`: For deep hierarchies, call `remember_concepts` recursively

## Communication

- **ALWAYS communicate in English** - All responses, documentation, comments, and messages must be in English
- Never use Portuguese or other languages unless explicitly requested by the user

## Ontology Workflow

**⚠️ CRITICAL RULE: ALWAYS use MCP tools to create or modify ontology classes and properties.**

- Classes → `define_class`
- Properties → `define_property`
- Calculated fields → `define_property` with a `formula` parameter
- Individuals → `assert_individual`
- Always invoke the `/new-ontology` skill to guide the process

## Ontology Design Principles

### Property Placement Rule

**⚠️ CRITICAL RULE: ALWAYS set `domain` to the class the property belongs to, never to the range class.**

- A property with `rdfs:domain foundation:Task` is created with `domain: foundation:Task`
- A property with `rdfs:domain owl:Thing` is universal and omits the domain parameter

**Examples:**

- ✅ `foundation:dependsOn` (domain: owl:Thing) → no domain parameter
- ✅ `foundation:hasStatus` (domain: owl:Thing, range: foundation:Status) → no domain, NOT domain: foundation:Status
- ✅ `foundation:userRole` (domain: foundation:UserStory) → domain: foundation:UserStory, NOT foundation:Persona
- ❌ Never set domain to the range class just because the property references it

### Use References Over Primitives

**⚠️ CRITICAL RULE: When creating or modifying ontologies, ALWAYS use references to existing ontology classes instead of primitive types (xsd:string, xsd:decimal, etc.) whenever a corresponding ontology exists.**

- **Always review if an ontology already exists** before using primitive types
- Use `owl:ObjectProperty` with `rdfs:range` pointing to the ontology class
- Add appropriate `owl:imports` declarations for referenced ontologies

**Common cases:**

- ✅ **Cities/Municipalities**: Use `foundation:City` instead of `xsd:string`
  ```turtle
  foundation:municipality a owl:ObjectProperty ;
      rdfs:range foundation:City .
  # NOT: foundation:municipality a owl:DatatypeProperty ; rdfs:range xsd:string .
  ```

- ✅ **Companies/Organizations**: Use `foundation:Company` or `foundation:Organization`
  ```turtle
  foundation:employer a owl:ObjectProperty ;
      rdfs:range foundation:Company .
  ```

- ✅ **Email addresses**: Use `foundation:EmailAddress`
  ```turtle
  foundation:contactEmail a owl:ObjectProperty ;
      rdfs:range foundation:EmailAddress .
  ```

- ✅ **Phone numbers**: Use `foundation:PhoneNumber`
  ```turtle
  foundation:contactPhone a owl:ObjectProperty ;
      rdfs:range foundation:PhoneNumber .
  ```

- ✅ **Addresses**: Use `foundation:Address`
- ✅ **Financial institutions**: Use `foundation:FinancialInstitution`
- ✅ **Geographic locations**: Use `foundation:Country`, `foundation:State`, `foundation:City`, etc.
- ✅ **People**: Use `foundation:Person`
- ✅ **Currencies**: Use QUDT `currency:*` (e.g., `currency:BRL`, `currency:USD`)

**When to use primitive types:**

- Simple scalar values (numbers, booleans, dates)
- Identifiers and codes (CNPJ, CPF, tax codes)
- Free-text descriptions and notes
- Amounts and quantities (combined with currency/unit references)


## Code Comments

Only *why* comments are acceptable. Flag and remove:
- Comments describing *what* the code does
- Commented-out code blocks
- TODO/FIXME/HACK/XXX markers

## Best Practices

- NEVER suppress warnings or errors
- When finishing a task, always review what was done to identify and resolve redundancies and ambiguities
- Always check the logs before making assumptions about problems
- Use SELECT queries to investigate the database before proposing changes
- **ALWAYS use RUST to create scripts** - Don't use Node.js, Python, or other languages for automation scripts
