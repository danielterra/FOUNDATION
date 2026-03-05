# Changelog

All notable changes to FOUNDATION will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
