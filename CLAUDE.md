# AI Assistant Guidelines for FOUNDATION Project

This document contains specific instructions for AI assistants working on the FOUNDATION project.

## 📋 Meta Rule - Document Maintenance

**⚠️ CRITICAL RULE: Whenever the user corrects you or indicates a preference, IMMEDIATELY UPDATE this CLAUDE.md document to record the correction/preference.**

- When you receive negative feedback about an action taken → add an explicit rule
- When the user indicates "always do X" or "never do Y" → document it here
- When there is a behavior correction → create an appropriate section if necessary
- This document should evolve continuously with the user's preferences
- Always confirm to the user when you update this document

## Logs & Debugging

- **ALWAYS consult the centralized logs when investigating problems**
- All frontend and backend errors are logged centrally
- Use `npm run logs` to view the latest logs (shows last 100 lines)
- When investigating problems, ALWAYS check the logs before making assumptions
- **Don't ask questions you can find the answer to yourself** - directly investigate logs, database, and code before asking

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
- **Ontology**: TTL files (core-ontology/)
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

## Version Management & Releases

- **⚠️ When generating a new release/version of the project:**
  1. Update the version in `src-tauri/Cargo.toml`
  2. Update the version in `package.json`
  3. **ALWAYS add the new version in `core-ontology/SoftwareRelease.ttl`**
     - Create a new entry `foundation:FoundationRelease_X_Y_Z`
     - Include: label, comment, releaseOf, versionNumber, licenseType, releaseDate, changelog
  4. Check if there are other ontology files that need to be updated

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
- **Removed functions** (kept as dead_code for reference):
  - `remember_connection_types`: Concepts already return connections via `remember_concept`
  - `remember_concept_tree`: For deep hierarchies, call `remember_concept` recursively

## Communication

- **ALWAYS communicate in English** - All responses, documentation, comments, and messages must be in English
- Never use Portuguese or other languages unless explicitly requested by the user

## Ontology Design Principles

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

**Import example:**
```turtle
<http://foundation.local/ontology/MyOntology>
  a owl:Ontology ;
  owl:imports <http://foundation.local/ontology/City> ,
              <http://foundation.local/ontology/Company> ,
              <http://foundation.local/ontology/EmailAddress> ,
              <http://foundation.local/ontology/PhoneNumber> ;
.
```

## Best Practices

- NEVER suppress warnings or errors
- When finishing a task, always review what was done to identify and resolve redundancies and ambiguities
- Always check the logs before making assumptions about problems
- Use SELECT queries to investigate the database before proposing changes
- **ALWAYS use RUST to create scripts** - Don't use Node.js, Python, or other languages for automation scripts
