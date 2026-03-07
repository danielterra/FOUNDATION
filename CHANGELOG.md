# Changelog

All notable changes to FOUNDATION will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.7.0] - 2026-03-07

### Added

- **Global search modal**: Search entities across the entire knowledge base with enriched results including concept type, status, and matched properties
- **Polymorphic subclass expansion**: Instance queries now expand to include all subclass descendants
- **`matchedProperties` in search results**: `remember_things` returns the properties that matched the query
- **Optional icon in `learn_thing`**: Icon field is now optional and inherits from the concept when omitted
- **Enriched entity search**: Results enriched with concept type, status icon/color, and matched property values

### Fixed

- Label argument now satisfies `rdfs:label` required field restriction
- Exclude `rdfs:label` from `matchedProperties` in `remember_things` results
- Inspector updates correctly on property changes
- Blank nodes filtered from `superClasses` in class results
- Accept RFC3339 format for `xsd:dateTime` values (in addition to Unix milliseconds)
- Support multiple inheritance in `update_concept` via `super_concepts` parameter
- Entity-updated events emitted after transaction COMMIT (not before)
- Orphaned references cleaned up when deleting an entity
- Fixed `include_retracted` bug in `search_things`

### Performance

- Markdown rendering moved to a Web Worker to avoid blocking the main thread
- Global entity query optimized to 30× faster; unused document data removed

### Refactored

- EAVTO calls moved from AI layer into `OWL Individual` methods
- AI tool API simplified to 7 tools; dead code removed; matched property values truncated
- Large source files split into focused modules; production `panic!` in setup replaced with graceful return

## [0.6.0] - 2026-03-06

### Added

- **Ontology required fields validation**: `update_concept` now validates that each property in `required_fields` actually exists as a `DatatypeProperty` or `ObjectProperty` before saving; returns a descriptive error if any property is undefined
- **Required fields display**: Inspector property list shows required fields with an asterisk
- **Cardinality validation on detail removal**: Validates cardinality constraints when removing a thing detail
- **Required fields management in AI functions**: AI can now manage required fields and cardinality constraints on concepts
- **Enriched `remember_concept`**: Now returns statuses, requiredFields, and incomingProperties
- **Enriched `remember_thing`**: Now returns retracted facts, allowed statuses, and required fields
- **Partial property updates in `update_thing`**: Supports status validation and partial updates

### Fixed

- **Inspector backlinks limit**: Backlinks in inspector now limited to 15 per group with real total count
- **`replace_all_property_iris` batch save**: Fixed retracting previous values by using batch `assert_triples`

### Refactored

- **`update_thing` IRI event deduplication**: Uses `HashSet` to avoid duplicate IRI event emissions
- **Removed `learn_thing_detail`**: Replaced by `update_thing` for a cleaner API
- **`learn_thing_detail` → `update_thing` migration**: Removed redundant function

---

## [0.5.3] - 2026-03-05

### Fixed

- **Web tools gated by ontology capability**: `web_search` and `web_fetch` are now only included in API requests when the configured model has `foundation:modelCapability = web_tools`; Haiku models (which don't support these tools) no longer receive a 400 error
- Added `web_tools` capability to all Sonnet and Opus model individuals in the ontology; `GenerateRequest.supports_web_tools` is resolved at request time from the database instead of being hardcoded

---

## [0.5.2] - 2026-03-05

### Refactored

- **AI function layer compliance**: `ai/functions/class.rs` and `ai/functions/concept.rs` no longer access the EAVTO layer directly; all data operations now go through OWL layer methods (`Class::find_all_iris`, `Class::retract_all`, `Class::set_label/icon/comment/super_class`, etc.)
- **OWL class helpers**: added `Class::find_all_iris`, `Class::get_subclass_iris`, `Class::set_label`, `Class::set_icon`, `Class::set_comment`, `Class::set_super_class`, and `Class::retract_all` to the OWL layer
- **OWL module helpers**: added `get_all_iri_properties` and `replace_all_property_iris` utility functions to `owl/mod.rs`
- **`chat.rs` split**: decomposed 1 357-line file into `chat/mod.rs`, `chat/tool_execution.rs`, `chat/message_utils.rs`, `chat/settings.rs`, and `chat/recovery.rs` (all under 750 lines)
- **Magic number constants**: replaced bare numeric literals in `providers.rs` with `WEB_TOOL_MAX_USES`, `WEB_FETCH_MAX_CONTENT_TOKENS`, and `CLAUDE_CACHE_READ_PRICE_PER_MILLION_TOKENS`
- **Test icon fixtures**: replaced placeholder icon names with URL-based icons in `individual.rs` and `functions/mod.rs` tests so they pass without a seeded database

---

## [0.5.1] - 2026-03-05

### Changed

- **Ontology embedded as SQL**: replaced 104 TTL files with a single `core-ontology/ontology.sql` file loaded via `include_str!` at compile time; eliminates `rio_turtle`, `rio_xml`, `rio_api` dependencies and per-startup file checks
- **OWL module made public**: `mod owl` is now `pub mod owl` in `lib.rs`, enabling external tooling to use `Class::get` and `Individual::get`

### Fixed

- Retracted 1,271 corrupt blank node triples (OWL Restriction nodes with colliding blank node IDs from TTL import) from the ontology dataset; zero blank nodes remain in the active dataset

### Refactored

- `namespaces.rs`: removed unused `compress_iri` function (only referenced by deleted turtle module)
- `connection.rs`: removed TTL parsing infrastructure (`import_rdf_core`, `import_dtype`, ontology file hash check); initialization now calls `execute_batch(ONTOLOGY_SQL)` directly

---

## [0.5.0] - 2026-03-05

### Added

- **Multi-conversation support**: create and list named conversations; all chat commands accept an optional `conversation_id` parameter (defaults to `foundation:MainChatConversation` for backward compatibility)
- **API call cost tracking**: every Claude API call is persisted as a `foundation:AIAPICall` entity with input/output/cache tokens and estimated cost based on the model's pricing
- **Atomic property setting on thing creation**: `learn_thing` now accepts a `properties` array to set multiple properties in a single transactional call; rolled back on any failure
- **`allowedStatus` validation**: setting `foundation:hasStatus` on a thing is rejected if the status IRI is not in the concept's `allowedStatus` list
- **`allowedStatuses` in concept queries**: `remember_concept` now returns the `allowedStatuses` array alongside `allowedValues`
- **`update_concept` supports `allowed_statuses`**: AI can set or replace the full list of allowed statuses for a concept
- **Date range filtering in `remember_things`**: new `from_millis` / `to_millis` parameters with inclusive semantics and timezone-aware documentation
- **Comparison operators in `remember_things_by_details`**: properties can now use `=`, `>=`, `<=`, `>`, `<` operators
- **Multi-domain support for connection types**: `learn_connection_type` `domain` parameter now accepts a string or an array of strings
- **xsd datatype validation on literals**: storing a literal with `xsd:dateTime`, `xsd:date`, `xsd:integer`, `xsd:decimal`, or `xsd:boolean` datatype now validates the value format at write time
- **Status icon inheritance**: `resolve_status_appearance` walks `foundation:parentStatus` recursively to inherit `icon` and `color` when absent on the status itself; `StatusInfo` now includes `icon`
- **`chat__create_conversation` / `chat__list_conversations`** Tauri commands exposed to the frontend

### Changed

- **DateTime storage unified to Unix milliseconds (i64)**: `Object::DateTime` is now stored and read as a Unix millisecond timestamp; ISO 8601 strings are no longer accepted as `xsd:dateTime` literals
- **Cache stable index extended**: prompt caching now marks the message at `len - 6` (instead of `len - 2`) as the stable cache point, improving cache hit rates in long conversations
- **Tool chip UI redesigned**: tool executions in chat are now shown as compact inline chips instead of full accordion panels; action buttons moved inside the message bubble on hover
- **`remember_things` query**: `prop_retracted_filter` is now applied per-property instead of globally, fixing incorrect results when `include_retracted` is false

### Fixed

- Removed no-op `.map_err(|e| e)` in conversation recovery path
- Removed dead `has_tool_result` condition in `chat__recover_pending_tools`
- `chat__recover_pending_tools` now scans all `foundation:AIConversation` individuals in addition to the legacy default conversation

### Refactored

- AI tool functions reorganized into `concept`, `thing`, and `detail` submodules
- `parse_timestamp` helper extracted to eliminate duplicated timestamp parsing across chat commands

---

## [0.1.0] - 2026-02-26 (Alpha Release)

### 🎉 Initial Release

FOUNDATION's first public alpha release! An AI-powered ontology management system with long-term memory.

### Added

#### Core Features
- **AI Chat Interface** with Claude API integration
  - Persistent chat history with full conversation context
  - Real-time tool execution visualization
  - Chat export functionality
  - API key management UI

#### AI Capabilities
- **15+ Tool Functions** for ontology management:
  - `learn_concept`: Create new classes/concepts
  - `learn_thing`: Create new instances
  - `learn_thing_detail`: Add properties to instances
  - `learn_connection_type`: Define new properties
  - `remember_concept`: Query class information
  - `remember_thing`: Get instance details
  - `remember_concepts`: Search classes
  - `remember_things`: Search instances
  - `remember_connection_types`: List properties
  - `remember_concept_tree`: Get class hierarchy
  - `remember_things_by_details`: Find instances by property
  - `forget_concept`: Delete classes
  - `forget_thing`: Delete instances
  - `forget_connection_type`: Delete properties
  - `forget_thing_detail`: Remove property values
  - `update_concept`: Modify class metadata
  - `update_thing`: Modify instance metadata

#### Tool Execution System
- **Dynamic Error Handling**: AI receives error feedback and can react
- **Tool Result Tracking**: Full visibility into tool execution
- **Token Management**: Intelligent context window management using tiktoken
- **Iterative Execution**: AI can make multiple tool calls in sequence

#### Infrastructure
- **Centralized Logging System**:
  - File-based logging to `~/Library/Application Support/org.w3id.foundation/application.log`
  - Structured log format with timestamps and levels
  - CLI tool for viewing logs (`npm run logs`)

- **Chat History Management**:
  - Persistent storage in SQLite
  - Tool use/result tracking in ontology
  - Clear history command (`npm run clear:chat`)

#### Ontology System
- **Base Ontology Classes**:
  - `AIModel`: AI model configurations
  - `Message`: Chat message structure
  - `ToolUse`: Tool invocation tracking
  - `ToolResult`: Tool execution results
  - `Conversation`: Multi-party conversations
  - Extended Person, Organization, Location classes

#### Build System
- **Multi-platform Support**:
  - macOS Universal (Apple Silicon + Intel)
  - Windows x64
  - Linux x64
- **GitHub Actions CI/CD**: Automated release builds
- **Release Scripts**: `build-release.sh` for local builds

### Technical Details

#### Architecture
- **Frontend**: Svelte 5 with SvelteKit
- **Backend**: Rust (Tauri 2.0)
- **Database**: SQLite with EAVTO (append-only triple store)
- **AI**: Claude API with tool calling
- **Token Counting**: tiktoken-rs for accurate token management

#### Dependencies
- Added `tiktoken-rs` for token counting
- Added `reqwest` for HTTP client
- Updated to Tauri 2.0 stable

### Known Issues

- Tool execution errors are now properly handled but validation is still strict (by design)
- Some ontology properties may need to be created dynamically during first use
- Cross-platform builds require native compilation on each platform

### Migration Notes

This is the first alpha release. No migration needed.

### Notes

This release removes the local AI model dependency (previously 2.4GB) and migrates fully to Claude API, making the application much lighter and more flexible.

[0.1.0]: https://github.com/danielterra/FOUNDATION/releases/tag/v0.1.0
