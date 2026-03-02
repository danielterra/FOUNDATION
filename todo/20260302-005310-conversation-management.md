# Feature Plan: Conversation Management

## Overview

Currently, the FOUNDATION chat system is locked to a single hardcoded conversation
(`foundation:MainChatConversation`). This feature replaces that model with a proper
multi-conversation system: the app always starts with a fresh conversation, users can
create new ones at any time, list past ones, and switch back to resume them. There is no
special "main" conversation — all conversations are equal.

## Affected Layers

- **Ontology**: Add `foundation:activeConversation` object property (domain `Person`, range
  `Conversation`) to `Conversation.ttl`.
- **OWL**: No changes needed — existing `Individual` API (`find_by_class_and_properties`,
  `get`, `new().assert()`, `add_property`) covers all conversation operations.
- **Commands**: Remove the hardcoded `CONVERSATION_ID` constant; update all chat commands to
  accept a `conversation_id` parameter; add new commands for conversation lifecycle management;
  add a startup command that creates the first conversation of a session.
- **Frontend**: Add a conversation panel (list + new button), wire active conversation IRI
  into all chat invocations, create a conversation on app start, handle switching.

## Implementation Tasks

### 1. Commands — Update existing chat commands to accept `conversation_id`

**File**: `src-tauri/src/commands/chat.rs`

- Remove `const CONVERSATION_ID: &str = "foundation:MainChatConversation"`.
- Add `conversation_id: String` parameter to:
  - `chat__send_and_reply(conversation_id, ...)`
  - `chat__get_recent_messages(conversation_id, limit, offset)`
  - `chat__recover_pending_tools(conversation_id)`
  - `continue_conversation_after_recovery(conversation_id, ...)` (internal helper)
- All internal usages of `CONVERSATION_ID` become `&conversation_id`.

---

### 2. Commands — Add conversation lifecycle commands

**File**: `src-tauri/src/commands/chat.rs` (extract to `src-tauri/src/commands/conversation.rs`
if the file exceeds ~800 lines after changes)

All conversation operations use the existing `Individual` API directly (same pattern as
`chat_storage.rs` does for messages). `ConversationSummary` struct lives in this file.

New Tauri commands:
- `chat__list_conversations() -> Result<Vec<ConversationSummary>, String>` — uses
  `Individual::find_by_class_and_properties(conn, "foundation:Conversation", &[])` then
  `Individual::get` for each to read label, status, startedAt; joins with message query for
  last_message_at and message_count.
- `chat__create_conversation(label: String) -> Result<String, String>` — mints IRI
  `foundation:Conversation_{timestamp}`, uses `Individual::new(...).assert(...)` +
  `add_property` for startedAt, status, participants; returns the IRI.
- `chat__archive_conversation(conversation_id: String) -> Result<(), String>` — uses
  `Individual::set_property` (or retract + re-assert) to set `conversationStatus = "archived"`.
- `chat__get_conversation_info(conversation_id: String) -> Result<ConversationSummary, String>` —
  replaces the existing stub that currently returns an error.

Register all new commands in `src-tauri/src/lib.rs` `invoke_handler`.

---

### 3. Commands — Add `chat__get_active_conversation` and persist active state

**File**: `src-tauri/src/commands/chat.rs`

The active conversation is stored as a direct relation on `foundation:ThisUser`:
```
foundation:ThisUser  foundation:activeConversation  foundation:Conversation_{timestamp}
```
Switching conversations just calls `add_property` with the new IRI — the EAVTO layer
automatically retracts the previous value.

Add two commands:
- `chat__get_active_conversation() -> Result<String, String>` — reads
  `foundation:activeConversation` on `foundation:ThisUser`; if absent or the referenced
  conversation is archived, calls `create_conversation` internally, sets the relation,
  and returns the IRI.
- `chat__set_active_conversation(conversation_id: String) -> Result<(), String>` — calls
  `individual.add_property(conn, "foundation:activeConversation", ...)` on
  `foundation:ThisUser`. Called on every conversation switch.

`chat__create_conversation` also calls `set_active_conversation` internally after creating.

---

### 4. Frontend — Boot with active conversation from database

**File**: `src/routes/+layout.svelte`

On mount, call `chat__get_active_conversation()` — this returns the persisted IRI (or creates
a fresh conversation if none exists). Store the result as `activeConversationId` (plain
`let`, not a store — it is passed down as a prop). No hardcoded IRI anywhere in the frontend.

---

### 5. Frontend — Add ConversationPanel component

**File**: `src/lib/components/ConversationPanel.svelte` (new file)

A collapsible sidebar panel that:
- Calls `chat__list_conversations` on mount and after create/archive actions.
- Shows each conversation as a clickable row: label, relative time of last message.
- Hides archived conversations by default (toggle to show them).
- Has a **"New conversation"** button that calls `chat__create_conversation` (default label
  "New Conversation") and immediately switches to the new conversation.
- Highlights the currently active conversation.
- Provides an archive action per row (icon button on hover).

**Types**: Add `ConversationSummary` type in `src/lib/types.ts` mirroring the Rust struct.

---

### 6. Frontend — Wire active conversation into ChatWindow

**File**: `src/lib/components/ChatWindow.svelte`

- Accept `activeConversationId: string` as a required prop (no default — caller must provide).
- Pass it as the first argument to all `invoke('chat__send_and_reply', ...)` and
  `invoke('chat__get_recent_messages', ...)` calls.
- When `activeConversationId` changes, clear messages and reload from the new conversation.

---

### 7. Frontend — Layout integration

**File**: `src/routes/+layout.svelte`

- `activeConversationId` is plain local state loaded from DB on mount (task 4).
- On `ConversationPanel` conversation-select event → call `chat__set_active_conversation` →
  update local `activeConversationId`.
- On `ConversationPanel` new-conversation event → `chat__create_conversation` (which persists
  the setting internally) → update local `activeConversationId`.
- Pass `activeConversationId` down to `ChatWindow` as a prop.

Suggested layout with panel collapsed by default:
```
[ConversationPanel] | [Main content area] | [ChatWindow]
```
The panel is toggled via an icon button in the chat header area.

---

## Ontology Changes

**Modified**: `core-ontology/Conversation.ttl`

Add `foundation:activeConversation` as an `owl:ObjectProperty`:
```turtle
foundation:activeConversation
    a owl:ObjectProperty ;
    rdfs:label "active conversation" ;
    rdfs:domain foundation:Person ;
    rdfs:range foundation:Conversation ;
    owl:cardinality 1 .
```

This property is set on `foundation:ThisUser` to track which conversation is currently open.

---

## Risks & Notes

- **Legacy data**: Existing messages reference `foundation:partOfConversation =
  "foundation:MainChatConversation"` which has no corresponding instance. These messages
  become accessible only if the user specifically queries that IRI. They will NOT appear in
  the conversation list (since no `rdf:type foundation:Conversation` triple exists for it).
  If preserving legacy messages matters, add a one-time migration in the startup command:
  check if any messages reference `MainChatConversation`, and if so, create a proper instance
  for that IRI with a label like "Legacy Chat" before creating the new session conversation.

- **Frontend breaking change**: Every existing `invoke('chat__send_and_reply', ...)` call
  gains a required `conversation_id` argument. Search all frontend `invoke` calls before
  implementing.

- **File size limit**: `chat.rs` is already substantial. If new commands push it past 1000
  lines, extract conversation commands into `commands/conversation.rs`.

- **Token budget**: Starting a new conversation means an empty message history. Verify the
  token-counting logic handles zero prior messages correctly.

- **Status values**: `conversationStatus` values ("active", "archived") must be consistent
  between Rust structs, Tauri serialization, and Svelte display logic.

- **No new Cargo dependencies** expected.

---

## Validation

```bash
# 1. Check Rust compiles cleanly after changes
cargo check --manifest-path src-tauri/Cargo.toml

# 2. After app start, verify a new conversation was created
sqlite3 ~/Documents/Foundation/FOUNDATION.db "
SELECT subject, predicate, COALESCE(object, object_value) as value
FROM triples
WHERE subject LIKE 'foundation:Conversation_%'
ORDER BY subject DESC, predicate
LIMIT 30;"

# 3. Create a second conversation via UI, send a message, verify it is stored under the new IRI
sqlite3 ~/Documents/Foundation/FOUNDATION.db "
SELECT t_conv.object_value as conversation, COUNT(*) as messages
FROM triples t_conv
WHERE t_conv.predicate = 'foundation:partOfConversation' AND t_conv.retracted = 0
GROUP BY t_conv.object_value;"

# 4. Switch back to a previous conversation in UI — verify its messages load correctly
# (manual test)

# 5. Archive a conversation — verify it is hidden from the active list
# (manual test)
```
