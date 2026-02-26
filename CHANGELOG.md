# Changelog

All notable changes to FOUNDATION will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
