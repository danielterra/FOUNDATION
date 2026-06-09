# FOUNDATION — AI Assistant Rules

## Meta
- ALWAYS update this file immediately when user corrects you or states a preference.
- NEVER use the memory system (`memory/` files, MEMORY.md). ALL learnings, feedback and preferences live in THIS file (CLAUDE.md) — it is the single source of truth.
- ALWAYS respond to the user in Portuguese.
- ALWAYS ask one validation item at a time, NEVER several at once; for multiple questions use AskUserQuestion (the UI component), NEVER plain-text question lists.
- NEVER touch git without an explicit order (no stash/checkout/reset/commit/push); validate fixes on the current working tree. Read-only git (status/diff/log) is free.
- ALWAYS keep this file in en-US, terse, bullet-only with ALWAYS/NEVER prefixes, H1/H2 headers only, zero prose or boilerplate.
- ALWAYS name skills as `entity-action` (e.g. `ontology-create`, `ontology-change`, `widget-remove`, `release-create`). NEVER use other naming patterns.

## Foundation principles — non-negotiable
- **OWNERSHIP** — local-first; data lives on the user's machine; no Big Tech servers; AGPL-3.0 prevents closed cloud forks. ALWAYS reject designs that require a centralized backend, hosted SaaS dependency or telemetry/exfiltration.
- **ONTOLOGY-FIRST** — model via classes/properties/individuals in the live DB (via MCP), never via ad-hoc tables. ALWAYS use `foundation:*`/`anthropic:*` vocabulary; NEVER hardcode IRIs that didn't come from `search`/`describe_*`.
- **IMMUTABLE STORE** — Datomic-style append-only triples with monotonic `tx`. ALWAYS update by writing a new triple with higher tx; NEVER `UPDATE` rows; `retracted=1` only to permanently delete a fact.
- **AUTOMATION-REACTIVE** — data acts on itself: writes notify, readevopsrs run, widgets refresh. ALWAYS route entity writes through the notify→emit path so readevopsrs and UI converge automatically; NEVER bypass with direct `app.emit` or out-of-band SQL.
- ALWAYS evaluate every design/code decision against these four pillars. A change that violates one is a blocker.

## Design defaults — pre-production
- NEVER add fallback paths, backward-compatibility shims, legacy branches or graceful-degradation defaults without EXPRESS user permission — the system is pre-production.
- ALWAYS implement ONE canonical behavior; when the model changes, migrate existing data/individuals to the new model instead of branching to keep the old one working.
- ALWAYS surface any design tension that tempts a fallback and ask the user before adding one.
- NEVER add a fallback/default branch in render dispatch maps either (e.g. a `bpmn_FlowNode → NodeTask` catch-all in a frontend node-type map) — fix the real cause so every concrete type maps to its own component; a missing mapping is a bug to fix, not to mask.

## Foundation vocabulary — domain entities
- `foundation:Project` / `foundation:SoftwareFeature` / `foundation:UserStory` / `foundation:ArchitectureDecisionRecord` (ADR) — work planning.
- `foundation:SoftwareAgent` — product-side agent personas only: NOVA (`foundation:LocalAIAssistant`), Automator, etc. **Dev-process personas (architect, support, ux, developer-backend, developer-frontend, qa, devops) live in [.claude/agents/](.claude/agents/) — NEVER in the ontology**. Process roles are tooling, not product.
- `foundation:Persona` — end-user personas (audience of the product): João (non-tech, `_1772476248172`), Daniel (power user, `_1773783644387`), AI Agent (`_1773180459062`).
- `foundation:MCPTool` — MCP tool registered on the agent surface; `foundation:functionName` MUST match the Rust `ToolTemplate.name` exactly or AgentTask silently drops the tool.
- `foundation:WidgetType` — blackboard widget definition with `widgetTypeId`, `widgetSupportedClass`, `widgetDefaultWidth/Height`, `widgetUsageNote` (WHY+HOW; presence marks AI-creatable).
- `foundation:Blackboard` / `foundation:Conversation` / `foundation:AIConversationMessage` / `foundation:AINotification` — chat & lousa surface.
- `foundation:Automation` (BPMN 2.0) / `foundation:WorkflowExecution` / `foundation:StepExecution` / `foundation:Task` — automation engine.
- `foundation:Bug` / `foundation:TestCase` — quality.
- ALWAYS naming: SoftwareFeature label ≤ 3 words, noun-based. UserStory label uses "Como [persona] quero [capability] para [benefit]".
- ALWAYS set `foundation:hasStatus` on every individual at creation time — even drafts. NEVER assert without status.

## Status IRIs — canonical (use IRI, NEVER label)
- `foundation:Pending` — Pendente.
- `foundation:Status_1772596341042` — Planejado.
- `foundation:Status_1773079329634` — Pronto para Desenvolvimento.
- `foundation:InProgress` — Em Progresso.
- `foundation:Status_1772600993751` — Em Validação (QA) / Testando.
- `foundation:Completed` — Concluído.
- `foundation:Status_1773581282341` — Mudança Pendente.
- `foundation:Blocked` — Bloqueado.
- `foundation:Rejected` — Rejeitado.
- `foundation:Status_1772570972069` — Cancelado.

## Development process — PO → Architect → specialists
- Roles: **PO** (Claude principal / user) → **architect** (single coordination point) → **ux** + **developer-backend** + **developer-frontend** → **qa** → **devops**. **Bug flow** adds **support** before `architect`: Bug → **support** (technical dossier) → **architect** (Modo Triagem) → dev → **qa** → Concluído.
- ALWAYS route every planning / implementation / bug request through `architect`. NEVER let the PO invoke `developer-backend`, `developer-frontend`, `ux` or `support` directly **for a fix**.
- `architect` has three modes: **Planejamento** (designs architecture, writes `foundation:implementationPlan`, moves US to Planejado), **Execução** (delegates the plan's "Fatia de execução" to specialists in parallel, costura builds + `## Como testar` + changelog, moves US to Em Validação (QA)), and **Triagem de Bug** (reads `support` dossier, picks the dev, delegates, costura, moves Bug to Em Validação (QA)).
- `architect` NEVER writes code, NEVER investigates bugs (that's `support`), NEVER closes items. Specialists never move status or persist plan/dossier — `architect` costura everything.
- `support` is the bug entry point. Investigates (logs → messages → DB → code), produces dossier in `foundation:Bug`, moves to Pronto para Dev. NEVER edits code, NEVER picks the dev, NEVER closes the bug.
- `ux` is invoked by `architect` ONLY in Planejamento — by heuristic (US touches `src/**` or mentions interface/widget/screen/form). It writes the `## UX/UI` section of the plan (including a "Validação pelo usuário" checklist that the QA merges into `## Como testar`). `ux` does NOT enter Execução — there is no way for it to see the rendered screen; visual validation happens in QA with the human user. `ux` NEVER edits code.
- `developer-backend` owns `src-tauri/**` (Rust/Tauri, MCP, ontology mutations via MCP). NEVER touches `src/**`.
- `developer-frontend` owns `src/**` (Svelte 5, TS, widgets, realtime subscriptions). NEVER touches `src-tauri/**`.
- **`qa` is the SINGLE gate before Concluído — for User Stories AND Bugs.** NEVER move a US or Bug from Em Progresso / Em Validação (QA) directly to Concluído without `qa` validation. `qa` enters when `architect` finishes Execução / Triagem; `devops` enters after for code review / security / release. NEITHER edits feature code.
- Skills `/userstory-plan`, `/userstory-implement`, `/bug-fix` are thin proxies: they invoke `architect` (and `support` for bugs). NEVER duplicate the protocol inside the skills.
- Skills `/feature-plan` and `/feature-implement` orchestrate per-US via the userstory skills, so they inherit the hierarchy automatically.

## Debugging order
- ALWAYS investigate in this order: logs → message history → DB → code.
- Logs: `npm run logs [N]` or `~/Library/Application Support/org.w3id.foundation/application.log`.
- DB: `~/Documents/Foundation/FOUNDATION.db`.
- Chat messages: class `foundation:AIConversationMessage`. `foundation:role` / `foundation:content` in `object_value`. `foundation:sentAt` is Unix ms in `object_datetime` (NOT `object_value`). `foundation:partOfConversation` / `foundation:sender` / `foundation:receiver` are IRIs in `object`.

## Database
- NEVER delete the database.
- NEVER run INSERT/UPDATE/DELETE/DROP/TRUNCATE without explicit user confirmation — SELECT only.
- ALWAYS use MCP tools for ALL DB access (reads and writes) — never raw SQL.
- If app is not running, report findings and wait for user to start it.
- If a Foundation MCP tool is unavailable, ALWAYS wait 1 minute and retry automatically — on auto, never stopping to ask; NEVER fall back to curl/SQLite and NEVER ask the user to reconnect.

## Triples table columns
- `object`: IRIs/blank nodes (`object_type = 'iri'` or `'blank'`).
- `object_value`: literal lexical value (`object_type = 'literal'`).
- `object_datatype`: e.g. `xsd:string`, `xsd:integer`, `xsd:dateTime`.
- ALWAYS use `COALESCE(object, object_value)` when object type is unknown.

## Immutability (TX is the source of truth)
- ALWAYS treat the largest `tx` as truth — NOT the `retracted` field.
- ALWAYS update a value by inserting a new triple with higher `tx`. Latest wins.
- NEVER set `retracted = 1` to "update" a value — only to permanently delete a fact.
- ALWAYS filter `AND tx = (SELECT MAX(tx) FROM triples WHERE subject = ? AND predicate = ?)` for current values.
- NEVER rely solely on `WHERE retracted = 0` — that returns items from all historical TXs, including removed ones.
- Multi-valued properties: the entire latest TX set is the truth. `TX1=(A,B,C)` → `TX2=(A,B)` means C was removed without retraction.

## System prompt
- Base prompt is loaded at runtime from `foundation:DefaultSystemPromptSetting` → `foundation:settingValue` — NOT hardcoded.
- ALWAYS edit via `replace_property_values`; takes effect immediately (no recompile).
- Loaded by `load_base_system_prompt(conn)` in `src-tauri/src/commands/chat/settings.rs`, called from `load_agent_config` and `AgentTask`/`Task` executors.
- Fallback: empty string if setting missing.
- Agent persona prompt (e.g. NOVA personality) lives in `foundation:basePrompt` on the agent individual; concatenated after base prompt.

## Project structure
- Frontend: Svelte + TypeScript (`src/`).
- Backend: Rust + Tauri (`src-tauri/`).
- Ontology: `core-ontology/ontology.sql` — auto-generated by `scripts/dump-ontology`, embedded via `include_str!()` in `src-tauri/src/eavto/connection.rs`.
- NEVER edit `ontology.sql` manually — mutate ontology via MCP on live DB; release captures the dump.

## Backend layer architecture (STRICT — violations are blockers in code review)
Layers top-to-bottom: `Frontend → Commands → Core-Ontology → OWL → EAVTO → SQLite`
- **EAVTO** (`src-tauri/src/eavto/`): generic triple storage only — subject/predicate/object. NEVER hardcode Foundation or Anthropic IRIs here. Query functions must be parametric; domain-specific predicate names are passed by the caller.
- **OWL** (`src-tauri/src/owl/`): generic ontology primitives — Class, Individual, Property, cardinality, inheritance. NEVER reference `foundation:*` or `anthropic:*` classes/properties here. Functions must be parametric; callers (Core-Ontology) supply the domain IRIs.
- **Core-Ontology** (`src-tauri/src/core_ontology/`): Foundation-specific use of OWL — manages Status, Search, Conversation patterns and other domain classes. ONLY imports from `owl/`. NEVER imports from `eavto/` directly.
- **Commands** (`src-tauri/src/commands/`): Tauri commands and business logic. Imports from `core_ontology/` and `owl/`. NEVER imports from `eavto/` directly.
- ALWAYS enforce: each layer imports ONLY from the layer directly below it. Skipping layers is a blocker.

## Dev commands
- NEVER run `npm run tauri dev` / `npm run build` — user manages those.
- ALWAYS rely on automatic rebuild — the user's running `tauri dev` recompiles on every Rust file change; NEVER ask the user to rebuild or restart after a backend edit.
- NEVER kill Tauri processes (pkill, killall, taskkill).
- Validate Rust with `cargo check` or `cargo build --manifest-path src-tauri/Cargo.toml`.
- ALWAYS validate with `cargo test` (or `cargo check --tests`) when touching a file that has a `#[cfg(test)]` module — plain `cargo check` skips test code and misses test-only compile errors.
- Logs: `npm run logs [N]`.

## Cargo cache rules
- `cargo check` and `cargo build` do NOT share codegen cache. Check → `.rmeta` only; build → `.rlib`/`.o`.
- ALWAYS use `cargo build` (not `check`) before user runs `tauri dev` if you touched `Cargo.toml`/profile/features/deps — otherwise codegen runs twice.
- Use `cargo check` only when user is NOT about to run `tauri dev`.
- ALWAYS warn user before profile/feature changes — invalidates 100% of the cache (~10–15 min full rebuild on this project).

## Real-time event system (pub/sub)
- Model: the frontend declares which entity IRIs it shows; the backend emits `entity-updated`/`entity-referenced`/`entity-deleted` ONLY for subscribed entities. Closing a widget drops its IRIs and stops its events.
- Registry: `crate::realtime::SubscriptionRegistry` (managed state). Frontend pushes the full displayed set via `events__set_subscriptions` (IRIs+patterns) or `events__set_subscriptions_v2` (IRIs+patterns+creation_queries) — wholesale replace, so a webview reload self-heals.
- Creation-queries: the frontend registers `(classIri, predicate, objectValue)` tuples via `events__set_subscriptions_v2`. The backend matches new triples against registered queries IN MEMORY (no DB query per emit) and emits `entity-joined-set` with the new entityId, classIri, predicate, objectValue and tx. IRIs of domain classes come from the frontend; the realtime layer stays generic.
- Cursor / replay: `entity-updated`/`entity-referenced`/`entity-joined-set` events carry a `tx` field. At snapshot time, the frontend calls `chat__get_conversation_snapshot_tx` (or equivalent) to get max(tx)=T, sets `setSinceTx(T)` on the subscription handle, and calls `replayMissed()` after the subscription is active — invoking `events__replay_since` to recover events in the assinar-depois-do-evento window. Ring in memory (1024 entries); falls back to triples table for older cursors.
- Backend: ALWAYS emit entity events via `crate::realtime::emit_entity_updated_with_tx` / `emit_entity_updated_with` / `emit_entity_referenced_with_tx` / `emit_entity_deleted` (batch path: `emit_queued`). They consult the registry. NEVER call `app.emit("entity-updated"/"entity-referenced"/"entity-deleted", …)` directly — it bypasses the subscription filter.
- DbExecutor write path: EVERY `DbExecutor::write()` accumulates `WRITTEN_SUBJECT_PREDICATES`/`WRITTEN_IRI_OBJECTS`/`WRITTEN_TRIPLES` (type `WrittenTriple`) → `notify_tx` → receiver (`setup.rs`) emits via the helpers above and matches creation-queries. No manual emit after a write.
- ALWAYS keep the search reindex in the notify receiver UNCONDITIONAL — never gate it on subscriptions.
- Backend readevopsrs (task execution/recurrence/scheduler) MUST listen to `entity-changed-internal` — a NON-gated signal the notify receiver emits via `crate::realtime::emit_entity_changed_internal` for EVERY written subject — NOT `entity-updated`/`entity-created` (subscription-gated, would skip entities the UI is not showing). The frontend NEVER listens to `entity-changed-internal`. Server-side automation runs regardless of what the UI displays.
- Frontend: ALWAYS consume entity events via `createEntitySubscription` (`$lib/realtime/subscriptions`); call `setIris` (exact IRIs), `setPatterns` (collection views), `setCreationQueries` and `setSinceTx`/`replayMissed` — typically from a `$effect` — and `destroy()` in `onDestroy`. NEVER attach a raw `listen('entity-updated'/…)` in a component.
- Streaming/execution events (`chat-ai-delta`, `ai-status`, `ai-error`, `automation-execution-*`) are NOT entity events — they stay direct emits/listens, scoped by payload (`conversationId`/`executionIri`).

## Releases
- ALWAYS use `/release-create` skill.

## Blackboard / Lousa
- `add_widget_to_blackboard` takes `blackboard_iri` (a `foundation:Blackboard`), NOT a conversation IRI. Omit → DefaultBlackboard.
- ALWAYS resolve conversation → board: a conversation's board IRI is `foundation:Blackboard_for_<conversationIri with ':' → '_'>` (e.g. `foundation:Blackboard_for_foundation_Conversation_<id>`).
- ALWAYS show user-facing widgets on the ACTIVE conversation's board, never DefaultBlackboard — EXCEPT when chat is off (no active conversation), then omit → DefaultBlackboard (app setting).
- MAIN conversation = `foundation:Conversation_1779893196496`; board = `foundation:Blackboard_for_foundation_Conversation_1779893196496`.
- `InProgress` status does NOT uniquely mark the active conversation (many conversations share it).

## TODO doc filenames
- Format: `YYYYMMDD-HHMMSS-description.md` (e.g. `20260228-192519-layer-violations-fix.md`).

## Gotchas & learnings
- `run_automation(dry_run:true)` mocks ONLY script-level writes (the Rhai `mcp()`/`owl_set_property` bridge). The engine STILL creates and PERSISTS control instances (and loop iteration instances + `iterationOf` links), and AgentTask STILL calls the AI. NOT side-effect-free — clean orphan control instances after a dry run; expect AI cost/latency.
- Plain-JS `.svelte` (no `lang="ts"`) is NOT type-checked by svelte-check / `npm run check` — orphan refs to removed vars pass 0/0 but throw `ReferenceError` at runtime, visible ONLY in the webview F12 console (NOT `npm run logs`, which is backend only). ALWAYS validate plain-JS components by RUNNING; sweep manually for orphan refs.
- Automation diagram nodes: characteristics / metadata / extra info render as a PILL (rounded chip, `border-radius:999px`), NEVER loose text — consistent with the progress badge. Static characteristic = neutral/subtle pill; dynamic state = colored pill.
- QA validates by RUNNING via MCP (`run_automation` + inspect the entity created AFTER the fix — recompute triggers are NOT retroactive), NEVER static review alone; `cargo check`/review has let runtime bugs through.
- Agent CONFIGURES/queries via ontology (sources, mappings, MCP tools); the MOTOR (workers/engine) EXECUTES; widgets are ontology views for end-user personas. Model accordingly.
- `hasStatus` is DERIVED from properties (startedAt/result/scheduledAt), NEVER a control gate — recurrence/scheduler/engine mechanisms must not depend on hasStatus to decide whether to run.
- Schema > Prompt: to force a field in a tool call, put it in the tool `input_schema` `required`, NOT "you MUST include X" in the system prompt — small models ignore the prompt but respect the schema.
- IMAP: use `BODY.PEEK[]` on fetch and `uid_store(+FLAGS \Seen)` ONLY after the email is persisted (commit); a persistence failure must leave the email unread.
- MCP Foundation unavailable / transient drop mid-flow: wait 60–180s and retry automatically — NEVER curl/SQLite, NEVER ask the user to reconnect.

## Code style
- ALWAYS write scripts in Rust — never Node, Python, or shell.
- Comments: WHY only. NEVER write WHAT comments, commented-out code, or TODO/FIXME markers.
- NEVER suppress warnings or errors.
- NEVER add redundant wrapper functions — if existing functions cover it, don't wrap.
- NEVER leave dead code — unused imports, unreachable map keys/branches, orphan components or files, code only reachable by a path that never executes. If a fix exposes dead code, DELETE it in the same change.
