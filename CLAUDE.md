# FOUNDATION — AI Assistant Rules

## Meta
- ALWAYS update this file immediately when user corrects you or states a preference.
- NEVER use the memory system (`memory/` files, MEMORY.md). ALL learnings, feedback and preferences live in THIS file (CLAUDE.md) — it is the single source of truth.
- ALWAYS respond to the user in Portuguese.
- ALWAYS ask one validation item at a time, NEVER several at once;  use AskUserQuestion (the UI component), NEVER plain-text question lists.
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
- ALWAYS write a UserStory from the END-USER's perspective delivering user-facing value — NEVER a technical/internal concern. Non-functional/technical requirements (resilience, retry, backoff, performance, observability, error handling) are `foundation:AcceptanceCriterion` of a user-facing US (e.g. "email triage completes reliably"), NEVER a US on their own. If a candidate US reads as implementation/internal mechanics, it is an AC of an existing user-value US — find that US and attach it.
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
- ALWAYS delegate to the specialist agent — the main loop (PO) ONLY orchestrates (dispatches agents, relays to the user, decides between options). NEVER do a specialist's work yourself when an agent exists for it: bug investigation/code reading-for-diagnosis → `support`; create/refine `SoftwareProduct`/`SoftwareFeature`/`UserStory`/`AcceptanceCriterion` (and discovery: searching/describing product entities) → `product-owner`; architecture/codebase mapping & planning → `architect`; UX spec → `ux`; code → `developer-backend`/`developer-frontend`; validation → `qa`; review/merge/release → `devops`. The PO does NOT grep/read code to investigate, NOT write ontology entities, NOT plan, NOT implement.
- ALWAYS route every planning / implementation / bug request through `architect`. NEVER let the PO invoke `developer-backend`, `developer-frontend`, `ux` or `support` directly **for a fix** (even a one-line/micro change — route through `architect`).
- `architect` has three modes: **Planejamento** (designs architecture, writes `foundation:implementationPlan`, moves US to Planejado), **Execução** (delegates the plan's "Fatia de execução" to specialists in parallel, costura builds + `## Como testar` + changelog, moves US to Em Validação (QA)), and **Triagem de Bug** (reads `support` dossier, picks the dev, delegates, costura, moves Bug to Em Validação (QA)).
- `architect` NEVER writes code, NEVER investigates bugs (that's `support`), NEVER closes items. Specialists never move status or persist plan/dossier — `architect` costura everything.
- `support` is the bug entry point. Investigates (logs → messages → DB → code), produces dossier in `foundation:Bug`, moves to Pronto para Dev. NEVER edits code, NEVER picks the dev, NEVER closes the bug.
- ALWAYS relate a reported problem to the specific `foundation:UserStory` of its feature BEFORE classifying it. Read the US + its `foundation:AcceptanceCriterion`: if behavior deviates from a defined AC → it is a **Bug** (defect); if it is behavior the US never specified → it is a **new UserStory or AcceptanceCriterion** (capability/enhancement), route to `product-owner`, NEVER to `support`/`/bug-fix`. Adding resilience/retry/tolerance the spec never required is a US, not a Bug.
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
- If a Foundation MCP tool is unavailable, ALWAYS first verify the dev server is alive (`pm2 describe foundation-dev` + `Get-Process FOUNDATION`) — Claude OWNS the lifecycle, so if it is down START it via `/server-start` instead of waiting. ONLY wait+retry (1 min, auto, never asking) when the server IS running (true transient drop). NEVER fall back to curl/SQLite and NEVER ask the user to reconnect.

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
- Ontology: lives in the SEPARATE `foundation-core` repo at `assets/ontology.sql` (embedded via `include_str!()` in the crate's `src/eavto/connection.rs`); the app consumes it via git-dep. See "## foundation-core crate (separate repo)".
- NEVER edit `ontology.sql` manually — mutate ontology via MCP on live DB; release captures the dump.

## foundation-core crate (separate repo)
- The ontology engine (EAVTO + OWL + base ontology + `foundation:*` vocabulary) lives in the SEPARATE public repo `foundation-core` (github.com/danielterra/foundation-core, AGPL-3.0) — NOT in this app repo. It is domain-agnostic-of-infrastructure and installable in other projects.
- The app consumes it as a git-dependency in `src-tauri/Cargo.toml` (`foundation-core = { git = "https://github.com/danielterra/foundation-core" }`).
- ALWAYS keep the COMMITTED `src-tauri/Cargo.toml` WITHOUT a `[patch]` block — CI/fresh clones fetch the core from GitHub (pinned by rev in `Cargo.lock`). The local `[patch]` (uncommitted) overrides it to the sibling clone for fluid dev.
- ALWAYS clone the core as a SIBLING at `../foundation-core` (so the `[patch]` path `../../foundation-core` and the ontology scripts resolve). Edit core code there; the app rebuilds against it via the patch.
- `dump-ontology`/`verify-ontology` (in this app repo) read the live DB and write/verify `../foundation-core/assets/ontology.sql`. Regenerate → commit in the core repo → bump the git rev in the app's `Cargo.lock` to pick it up.
- Storage identity is injected at app boot via `foundation_core::paths::configure("Foundation","FOUNDATION.db","org.w3id.foundation")` (defaults Foundation) — `src-tauri/src/lib.rs` `.setup()`.

## Backend layer architecture (STRICT — violations are blockers in code review)
Layers top-to-bottom: `Frontend → Commands → Core-Ontology → OWL → EAVTO → SQLite`
- **EAVTO** (`src/eavto/` in the separate `foundation-core` repo — consumed via git-dep): generic triple storage only — subject/predicate/object. NEVER hardcode Foundation or Anthropic IRIs here. Query functions must be parametric; domain-specific predicate names are passed by the caller.
- **OWL** (`src/owl/` in the separate `foundation-core` repo — consumed via git-dep; the app keeps only a thin façade in `src-tauri/src/owl/mod.rs` re-exporting it plus the Tauri-coupled `formula_worker`/`query_worker`): generic ontology primitives — Class, Individual, Property, cardinality, inheritance. NEVER reference `foundation:*` or `anthropic:*` classes/properties here. Functions must be parametric; callers (Core-Ontology) supply the domain IRIs.
- **Core-Ontology** (`src-tauri/src/core_ontology/`): Foundation-specific use of OWL — manages Status, Search, Conversation patterns and other domain classes. ONLY imports from `owl/`. NEVER imports from `eavto/` directly.
- **Commands** (`src-tauri/src/commands/`): Tauri commands and business logic. Imports from `core_ontology/` and `owl/`. NEVER imports from `eavto/` directly.
- ALWAYS enforce: each layer imports ONLY from the layer directly below it. Skipping layers is a blocker.

## Dev server lifecycle — Claude's responsibility
- Claude OWNS the dev server lifecycle via PM2 (process `foundation-dev`, defined in `ecosystem.config.cjs`). The user no longer manages it.
- ALWAYS use the skills: `/server-start`, `/server-stop`, `/server-restart` — NEVER spawn `npm run tauri dev` directly and NEVER `pm2 restart` (skips orphan sweep).
- ALWAYS kill processes ONLY through the skills' managed paths (PM2 + targeted orphan sweep of `FOUNDATION.exe`); NEVER kill unrelated node/cargo processes.
- If the app is down and Claude needs it (MCP, QA, validation), start it via `/server-start` — do not ask the user.
- Vite SSR zombie (`transport invoke timed out ... vite:invoke fetchModule`, or boot stalled at `MCP server listening` with no `[STARTUP]`/`[FRONTEND]` lines): run `/server-restart` with cache clean (`node_modules/.vite` + `.svelte-kit`) — recurring issue.

## Dev commands
- ALWAYS rely on automatic rebuild — the running `tauri dev` recompiles on every Rust file change; no restart needed after a backend edit.
- NEVER run `npm run build` — production builds only via release flow.
- Validate Rust with `cargo check` or `cargo build --manifest-path src-tauri/Cargo.toml`.
- ALWAYS validate with `cargo test` (or `cargo check --tests`) when touching a file that has a `#[cfg(test)]` module — plain `cargo check` skips test code and misses test-only compile errors.
- Logs: `npm run logs [N]` (backend); `pm2 logs foundation-dev --lines N --nostream` (vite/tauri stdout).

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
- MCP Foundation unavailable: it is NOT always a transient drop — FIRST check the server is up (`pm2 describe foundation-dev` + `Get-Process FOUNDATION`); if down, Claude owns the lifecycle → START it via `/server-start`, do NOT just wait. Only when the server IS running treat it as a transient drop: wait 60–180s and retry automatically. NEVER curl/SQLite, NEVER ask the user to reconnect. The `support`/`qa`/`developer-backend`/`product-owner` sub-agents DO receive the Foundation MCP tools; the `architect` sub-agent does NOT (its definition lists them but they are not injected — it fails with `No such tool available`). So architect costura that requires ontology WRITES (status moves, changelog, plan/dossier persistence) must be delegated by the PO to an MCP-capable agent (`support`/`qa`/`developer-backend`) — do NOT re-dispatch the architect for the write. The PO (main loop) has no direct MCP tools either.

## Code style
- ALWAYS paginate EVERY method/command/MCP tool that returns a collection with STABLE pagination = `snapshot_tx` + `ORDER BY` domain-key + offset/keyset INSIDE the snapshot. (1) Pin `snapshot_tx = MAX(tx)` on the FIRST page and echo it on every later page. (2) Read every page AS-OF snapshot_tx: BOTH membership as-of (the membership triple's `MAX(tx) <= snapshot_tx` and not retracted) AND the ordering-key value as-of (key's value at `MAX(tx) <= snapshot_tx`) — freezing only one re-introduces mid-scroll reordering. (3) `ORDER BY` the list's natural DOMAIN key (`foundation:lastUpdatedAt` for activity, `foundation:createdAt`, `rdfs:label`, `foundation:receivedAt`…), NEVER `tx`. (4) With the dataset frozen, offset is stable again (offset only broke because a new row entered the top — the snapshot removes that). Return `{ items, next_cursor, has_more, snapshot_tx }`. The frontend pins snapshot_tx on page 1 and passes it back. NEVER load-all-then-slice, NEVER return an unbounded list. Rationale + revision recorded in the pagination ADR (`foundation:ArchitectureDecisionRecord_1781556688201`).
- `tx` is the ordering key ONLY for create-immutable entities whose natural order IS creation-recency (`RawDataRecord`, `AIAPICall`, IMAP sync log): there `ORDER BY creation_tx DESC` is the domain key and keyset-by-creation_tx is correct. NEVER use `tx` as the ordering key for lists ordered by a mutable/domain key (AI models by default/alpha, conversations by activity).
- The pagination `snapshot_tx` and the realtime-replay `setSinceTx` share the value `MAX(tx)` but are DISTINCT in purpose (frozen past vs live future) — complementary, they converge in the UI. Do NOT conflate them.
- Bounded low-cardinality single-page lists (AI services/models pickers, agents, IMAP accounts, applicable properties) MAY freeze membership-as-of only and accept the current ordering key — there is no page 2 where instability shows. If such a list ever grows to real pagination, promote it to true snapshot. Apply the bound in SQL (`LIMIT`), NEVER load-all-then-slice.
- EXCEPTIONS (still NOT snapshot-paginated; a NEW one needs PO sign-off + an ADR note): (1) relevance-ranked search (`search` / `owl__search_entities` / `describe_property` query mode) — bounded top-N, re-ranked per query, NO cursor; bound applied in SQL. (2) graph/BFS traversals (`class_graph`) — bounded by `max_depth`, no linear cursor. (3) `chat__get_recent_messages` scroll-back stays offset — realtime replay-since-tx already covers incremental advance.
- For MCP tools, declare `snapshot_tx` (echoed) + the page param in the code `ToolTemplate` (`ai/functions/definitions.rs`); the AI MUST echo the returned `snapshot_tx` (and `next_cursor` where keyset) to keep the dataset frozen across pages. The MCP-exposed `inputSchema` is built from the code `ToolTemplate` (`mcp/mod.rs::to_mcp_tool`), NOT from the `foundation:MCPTool` individual (whose `foundation:inputSchema` is documentation-only and is NEVER read at runtime); after changing a tool's params the MCP CLIENT must re-list tools (reconnect) before sending the new arg.
- ALWAYS write scripts in Rust — never Node, Python, or shell.
- Comments: WHY only. NEVER write WHAT comments, commented-out code, or TODO/FIXME markers.
- NEVER suppress warnings or errors.
- NEVER add redundant wrapper functions — if existing functions cover it, don't wrap.
- NEVER leave dead code — unused imports, unreachable map keys/branches, orphan components or files, code only reachable by a path that never executes. If a fix exposes dead code, DELETE it in the same change.
- ALWAYS prefer shadcn-svelte components (`src/lib/components/ui/*` — Button, Card, etc.) over custom CSS/markup when a primitive exists. When a needed size/variant is missing (e.g. an icon button smaller than `icon-sm`), EXTEND the component's variant set (`buttonVariants`), NEVER a one-off ad-hoc CSS class on the call site.
