# FOUNDATION — Codex Agent Rules

## Meta
- ALWAYS update this file immediately when user corrects you or states a preference.
- NEVER use project memory files (`memory/`, `MEMORY.md`). ALL project learnings, feedback and preferences live in THIS file (`AGENTS.md`) as the repository source of truth.
- ALWAYS respond to the user in Portuguese.
- ALWAYS keep progress updates short and direct.
- ALWAYS ask one validation item at a time, NEVER several at once. Use structured input when available for optional choices; when an answer is required to proceed, ask one concise plain-text question.
- ALWAYS create a new branch when starting a new work item, naming it exactly for the target release version (`MAJOR.MINOR.PATCH`); default to the next minor version (`MAJOR.(MINOR+1).0`).
- NEVER create a patch or major release branch unless the user explicitly requests that release type.
- NEVER stash, reset, commit, or push without an explicit order; read-only git (status/diff/log) is free.
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
- ALWAYS require a successful email account test during initial setup; the email address is the person's canonical identity.
- ALWAYS implement ONE canonical behavior; when the model changes, migrate existing data/individuals to the new model instead of branching to keep the old one working.
- ALWAYS surface any design tension that tempts a fallback and ask the user before adding one.
- NEVER add a fallback/default branch in render dispatch maps either (e.g. a `bpmn_FlowNode → NodeTask` catch-all in a frontend node-type map) — fix the real cause so every concrete type maps to its own component; a missing mapping is a bug to fix, not to mask.

## Visual language
- ALWAYS preserve the reviewed Oblivion canvas texture: a black field with a subtle, evenly spaced cyan dot grid behind widgets.
- ALWAYS keep the floating widgets menu collapsed to its smallest useful affordance while the pointer is outside; expand it on hover and collapse it when the pointer leaves.
- ALWAYS animate floating widgets menu expansion and collapse smoothly; align the compact icon and count; constrain the panel to a maximum width and wrap long widget names within it.

## Product communication
- ALWAYS ground positioning in sovereignty/privacy/security, efficiency, and visual collaboration; start with the founding purpose.
- ALWAYS explain ontology as structured, connected knowledge; explain reactive automation as avoiding repeated agent work and Subconscious as surfacing relevant context.
- ALWAYS present the blackboard as a shared visual workspace for people and agents, reducing text overload.
- NEVER publish absolute offline, security, performance, or token-saving claims without verifying their scope and evidence.

## Foundation vocabulary — domain entities
- `foundation:Project` / `foundation:SoftwareFeature` / `foundation:UserStory` / `foundation:ArchitectureDecisionRecord` (ADR) — work planning.
- `foundation:SoftwareAgent` — product-side agent personas only: NOVA (`foundation:LocalAIAssistant`), Automator, etc. **Dev-process personas (architect, support, ux, developer-backend, developer-frontend, qa, devops) live in [.codex/agents/](.codex/agents/) — NEVER in the ontology**. Process roles are tooling, not product.
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

## Product status integrity
- ALWAYS read the current Feature → UserStory → AcceptanceCriterion graph through Foundation MCP before changing a status.
- ALWAYS enumerate the complete linked AcceptanceCriterion set before QA; when an MCP relation view is capped, use paginated or reverse relation queries and never treat a truncated list as complete.
- ALWAYS keep status separate from implementation evidence; source code or a changelog alone NEVER proves an AcceptanceCriterion passed.
- ALWAYS record concrete QA evidence for every AcceptanceCriterion moved to `foundation:Completed`.
- ALWAYS update each AcceptanceCriterion's status immediately when QA confirms its result; update the UserStory to `foundation:Completed` in that same QA pass once every in-scope criterion is Completed and required checks pass.
- ALWAYS persist and read back each AcceptanceCriterion's status and evidence as soon as its individual QA result is known; never defer AC status updates until the end of the test run.
- ALWAYS verify the QA changelog entry after writing it; when `describe_individual` or `read_property_page` truncates a multi-valued property, confirm the entry through a property-value search before concluding it is absent. Keep the parent Blocked only when all supported readback paths fail to find it.
- ALWAYS finish the parent status in the same QA pass: `foundation:Completed` when all in-scope ACs pass and required checks are green; `foundation:Status_1773581282341` when an AC fails and a fix is required; `foundation:Blocked` when validation cannot continue without an external action.
- ALWAYS reserve `foundation:Status_1772600993751` for QA that is actively testing; do not leave a story in QA after the pass ends with an unresolved external blocker.
- ALWAYS let QA set a UserStory to `foundation:Completed` only after every current in-scope AcceptanceCriterion is Completed, none is Pending or Rejected, and required automated and visual checks pass.
- ALWAYS return a Completed or QA UserStory to `foundation:Status_1773581282341` before planning when an AcceptanceCriterion is added or changed after validation.
- ALWAYS return a Feature to `foundation:Status_1773581282341` when a verified change contradicts its delivered `successCriteria`; record the code and criterion evidence in its changelog.
- ALWAYS confirm the explicitly delivered scope before setting a Feature to `foundation:Completed`; membership alone does not prove every linked story belongs to that scope.
- ALWAYS run `$product-integrity-audit` before release work and after `$feature-code-sync`; resolve all blockers before release.
- NEVER infer or roll up a Feature or UserStory status from child statuses without verifying scope and evidence.
- NEVER move a UserStory to Completed outside the QA gate.
- NEVER let the read-only product integrity audit mutate entities or statuses.

## Development process — PO → Architect → specialists
- Roles: **PO** (Codex principal / user) → **architect** (single coordination point) → **ux** + **developer-backend** + **developer-frontend** → **qa** → **devops**. **Bug flow** adds **support** before `architect`: Bug → **support** (technical dossier) → **architect** (Modo Triagem) → dev → **qa** → Concluído.
- ALWAYS delegate to the specialist agent — the main loop (PO) ONLY orchestrates (dispatches agents, relays to the user, decides between options). NEVER do a specialist's work yourself when an agent exists for it: bug investigation/code reading-for-diagnosis → `support`; create/refine `SoftwareProduct`/`SoftwareFeature`/`UserStory`/`AcceptanceCriterion` (and discovery: searching/describing product entities) → `product-owner`; architecture/codebase mapping & planning → `architect`; UX spec → `ux`; code → `developer-backend`/`developer-frontend`; validation → `qa`; review/merge/release → `devops`. The PO does NOT grep/read code to investigate, NOT write ontology entities, NOT plan, NOT implement.
- ALWAYS route every planning / implementation / bug request through `architect`. NEVER let the PO invoke `developer-backend`, `developer-frontend`, `ux` or `support` directly **for a fix** (even a one-line/micro change — route through `architect`).
- `architect` has three modes: **Planejamento** (designs architecture, writes `foundation:implementationPlan`, moves US to Planejado), **Execução** (delegates the plan's "Fatia de execução" to specialists in parallel, costura builds + `## Como testar` + changelog, moves US to Em Validação (QA)), and **Triagem de Bug** (reads `support` dossier, picks the dev, delegates, costura, moves Bug to Em Validação (QA)).
- `architect` NEVER writes code, NEVER investigates bugs (that's `support`), NEVER closes items. Specialists never move status or persist plan/dossier — `architect` costura everything.
- `support` is the bug entry point. Investigates (logs → messages → DB → code), produces dossier in `foundation:Bug`, moves to Pronto para Dev. NEVER edits code, NEVER picks the dev, NEVER closes the bug.
- ALWAYS relate a reported problem to the specific `foundation:UserStory` of its feature BEFORE classifying it. Read the US + its `foundation:AcceptanceCriterion`: if behavior deviates from a defined AC → it is a **Bug** (defect); if it is behavior the US never specified → it is a **new UserStory or AcceptanceCriterion** (capability/enhancement), route to `product-owner`, NEVER to `support`/`$bug-fix`. Adding resilience/retry/tolerance the spec never required is a US, not a Bug.
- ALWAYS route UX planning through `architect`, who invokes `ux` ONLY in Planejamento — by heuristic (US touches `src/**` or mentions interface/widget/screen/form). `ux` writes the `## UX/UI` section of the plan (including a "Validação pelo usuário" checklist that QA merges into `## Como testar`). `ux` does NOT enter Execução or validate the rendered screen; visual validation happens in the running app via visible Computer Use by the frontend developer and QA. `ux` NEVER edits code.
- `developer-backend` owns `src-tauri/**` (Rust/Tauri, MCP, ontology mutations via MCP). NEVER touches `src/**`.
- `developer-frontend` owns `src/**` (Svelte 5, TS, widgets, realtime subscriptions). NEVER touches `src-tauri/**`.
- **`qa` is the SINGLE gate before Concluído — for User Stories AND Bugs.** NEVER move a US or Bug from Em Progresso / Em Validação (QA) directly to Concluído without `qa` validation. `qa` enters when `architect` finishes Execução / Triagem; `devops` enters after for code review / security / release. NEITHER edits feature code.
- ALWAYS have the implementation workflow invoke QA immediately after the architect hands off a story to Em Validação; QA owns per-AC evidence and status writes, then reads back the complete status set before reporting.
- Skills `$userstory-plan`, `$userstory-implement`, `$bug-fix` are thin proxies: they invoke `architect` (and `support` for bugs). NEVER duplicate the protocol inside the skills.
- Skills `$feature-plan` and `$feature-implement` orchestrate per-US via the userstory skills, so they inherit the hierarchy automatically.
- ALWAYS use `.agents/skills/product-integrity-audit/SKILL.md` as the read-only status audit procedure; release and code-sync workflows invoke it.

## Debugging order
- ALWAYS investigate in this order: logs → message history → DB → code.
- Logs: `npm run logs [N]` or `~/Library/Application Support/org.w3id.foundation/application.log`.
- DB: `~/Documents/Foundation/FOUNDATION.db`.
- Chat messages: class `foundation:AIConversationMessage`. `foundation:role` / `foundation:content` in `object_value`. `foundation:sentAt` is Unix ms in `object_datetime` (NOT `object_value`). `foundation:partOfConversation` / `foundation:sender` / `foundation:receiver` are IRIs in `object`.

## Database
- NEVER delete the database.
- ALWAYS create a consistent backup before user-authorized local diagnostic mutations when the user accepts test-data risk; mark test data uniquely and clean up only those test-created individuals via MCP after evidence is recorded.
- NEVER run INSERT/UPDATE/DELETE/DROP/TRUNCATE without explicit user confirmation — SELECT only.
- ALWAYS use MCP tools for ALL DB access (reads and writes) — never raw SQL.
- If app is not running, report findings and wait for user to start it.
- If a Foundation MCP tool is unavailable, ALWAYS first verify the dev server is alive (`pm2 describe foundation-dev` + `pgrep -fl FOUNDATION` on macOS/Linux or `Get-Process FOUNDATION` on Windows) — Codex OWNS the lifecycle, so if it is down START it via `$server-start` instead of waiting. ONLY wait+retry (1 min, auto, never asking) when the server IS running (true transient drop). NEVER fall back to curl/SQLite and NEVER ask the user to reconnect.

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
- ALWAYS keep the COMMITTED `src-tauri/Cargo.toml` AND `Cargo.lock` clean (git-dep + rev pin, NO `[patch]`) — CI/fresh clones fetch the core from GitHub. NEVER add a `[patch]` block to the committed Cargo.toml for local dev — once the file is tracked the local edit cannot be gitignored and clutters the working tree forever (`.gitignore` only affects untracked files).
- ALWAYS do local fluid dev via a Cargo **paths override** placed OUTSIDE any repo, in `C:\GIT\.cargo\config.toml`: `paths = ["C:/GIT/foundation-core"]`. Cargo reads it walking up the tree from `src-tauri`; it is applied at build time and NOT recorded in `Cargo.lock`, so the working tree stays clean (verify with `cargo metadata --locked` → exit 0 and foundation-core `source = null`).
- ALWAYS clone the core as a SIBLING at `../foundation-core` (= `C:\GIT\foundation-core`, what the paths override points at) so the override and the ontology scripts resolve. Edit core code there; the app rebuilds against it via the override.
- `dump-ontology`/`verify-ontology` (in this app repo) read the live DB and write/verify `../foundation-core/assets/ontology.sql`. Regenerate → commit in the core repo → bump the git rev in the app's `Cargo.lock` to pick it up.
- Storage identity is injected at app boot via `foundation_core::paths::configure("Foundation","FOUNDATION.db","org.w3id.foundation")` (defaults Foundation) — `src-tauri/src/lib.rs` `.setup()`.

## Backend layer architecture (STRICT — violations are blockers in code review)
Layers top-to-bottom: `Frontend → Commands → Core-Ontology → OWL → EAVTO → SQLite`
- **EAVTO** (`src/eavto/` in the separate `foundation-core` repo — consumed via git-dep): generic triple storage only — subject/predicate/object. NEVER hardcode Foundation or Anthropic IRIs here. Query functions must be parametric; domain-specific predicate names are passed by the caller.
- **OWL** (`src/owl/` in the separate `foundation-core` repo — consumed via git-dep; the app keeps only a thin façade in `src-tauri/src/owl/mod.rs` re-exporting it plus the Tauri-coupled `formula_worker`/`query_worker`): generic ontology primitives — Class, Individual, Property, cardinality, inheritance. NEVER reference `foundation:*` or `anthropic:*` classes/properties here. Functions must be parametric; callers (Core-Ontology) supply the domain IRIs.
- **Core-Ontology** (`src-tauri/src/core_ontology/`): Foundation-specific use of OWL — manages Status, Search, Conversation patterns and other domain classes. ONLY imports from `owl/`. NEVER imports from `eavto/` directly.
- **Commands** (`src-tauri/src/commands/`): Tauri commands and business logic. Imports from `core_ontology/` and `owl/`. NEVER imports from `eavto/` directly.
- ALWAYS enforce: each layer imports ONLY from the layer directly below it. Skipping layers is a blocker.

## Dev server lifecycle — Codex responsibility
- Codex OWNS the dev server lifecycle via PM2 (process `foundation-dev`, defined in `ecosystem.config.cjs`). The user no longer manages it.
- ALWAYS visually validate UI changes in the running Foundation app through visible, cursor-driven Computer Use; the frontend developer performs a smoke check and QA independently validates the acceptance criteria.
- ALWAYS prefer the browser bridge at `http://localhost:1420/` when native Foundation cannot be bound by Computer Use. Keep the Tauri dev app running under PM2: its debug-only `tauri-plugin-dev-invoke` relays browser `invoke` calls to the local Rust backend.
- ALWAYS perform browser-bridge visual validation in the Codex integrated browser; NEVER use Safari or another standalone browser unless the user explicitly requests it.
- ALWAYS require the implementing developer to validate UI changes visually in the integrated browser before reporting implementation complete; if validation fails, the developer fixes the issue and repeats the validation.
- ALWAYS close the Inspector widget for an entity after its deletion succeeds, remove that widget from the active blackboard, and keep unrelated widgets open.
- ALWAYS confirm browser validation with a visible interaction and the resulting rendered state; NEVER claim success from screenshots or accessibility-tree inspection alone.
- ALWAYS keep the dev bridge bound to loopback and the exact development origin, and configure `.serve_assets(false)`; NEVER expose it in release builds or enable its asset-serving endpoint.
- ALWAYS use `$server-start` when the dev app is down and `$server-restart` when a clean restart is needed; NEVER substitute invisible input when the browser bridge or Computer Use is unavailable — report the concrete blocker.
- ALWAYS use the skills: `$server-start`, `$server-stop`, `$server-restart` — NEVER spawn `npm run tauri dev` directly and NEVER `pm2 restart` (skips orphan sweep).
- ALWAYS kill processes ONLY through the skills' managed paths (PM2 + targeted orphan sweep of `FOUNDATION.exe`); NEVER kill unrelated node/cargo processes.
- If the app is down and Codex needs it (MCP, QA, validation), start it via `$server-start` — do not ask the user.
- Vite SSR zombie (`transport invoke timed out ... vite:invoke fetchModule`, or boot stalled at `MCP server listening` with no `[STARTUP]`/`[FRONTEND]` lines): run `$server-restart` with cache clean (`node_modules/.vite` + `.svelte-kit`) — recurring issue.

## Application window
- ALWAYS start the desktop app with its window maximized to the available display bounds on every launch.

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
- ALWAYS use `$release-create` skill.
- ALWAYS implement new work on its release-version branch; the target release branch is the workspace for that release's planned work.
- ALWAYS continue work on branch `0.22.0` until the user publishes it; NEVER create a new release branch before publication.

## Blackboard / Lousa
- ALWAYS when a user clicks an entity reference in an Inspector, open its widget immediately at the top of the active blackboard stack; if a widget for that entity already exists, promote it instead of creating a duplicate.
- ALWAYS resolve the blackboard from the conversation currently open in the Foundation app UI; NEVER assume a hardcoded or main conversation is the active one.
- ALWAYS verify newly created Foundation stories and artifacts are visibly present on that conversation's blackboard before reporting completion.
- `add_widget_to_blackboard` takes `blackboard_iri` (a `foundation:Blackboard`), NOT a conversation IRI. Omit → DefaultBlackboard.
- ALWAYS resolve conversation → board: a conversation's board IRI is `foundation:Blackboard_for_<conversationIri with ':' → '_'>` (e.g. `foundation:Blackboard_for_foundation_Conversation_<id>`).
- ALWAYS show user-facing widgets on the ACTIVE conversation's board, never DefaultBlackboard — EXCEPT when chat is off (no active conversation), then omit → DefaultBlackboard (app setting).
- NEVER assume a fixed `MAIN conversation` IRI; resolve the conversation currently open in the Foundation UI and its board immediately before each board operation.
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
- MCP Foundation unavailable: FIRST check whether the dev server is alive (`pm2 describe foundation-dev` plus `pgrep -fl FOUNDATION` on macOS/Linux or `Get-Process FOUNDATION` on Windows). If it is down, Codex owns the lifecycle and starts it via `$server-start`. Only when the server is running treat the failure as transient: wait 60–180s and retry automatically. NEVER fall back to curl/SQLite and NEVER ask the user to reconnect. MCP availability is session-specific; if a subagent lacks a required Foundation tool, it returns the pending ontology operation to the primary agent or another MCP-capable role instead of inventing a fallback.

## Code style
- ALWAYS use `json-render.dev` with `@json-render/core` and `@json-render/svelte` for AI-generated custom widgets; maintain one canonical catalog of all eligible reusable UI components and allowed props/actions.
- ALWAYS allow closures until the planned closures-to-named-functions refactor is complete; only reintroduce the named-functions-only rule after that refactor.
- ALWAYS paginate EVERY method/command/MCP tool that returns a collection with STABLE pagination = `snapshot_tx` + `ORDER BY` domain-key + offset/keyset INSIDE the snapshot. (1) Pin `snapshot_tx = MAX(tx)` on the FIRST page and echo it on every later page. (2) Read every page AS-OF snapshot_tx: BOTH membership as-of (the membership triple's `MAX(tx) <= snapshot_tx` and not retracted) AND the ordering-key value as-of (key's value at `MAX(tx) <= snapshot_tx`) — freezing only one re-introduces mid-scroll reordering. (3) `ORDER BY` the list's natural DOMAIN key (`foundation:lastUpdatedAt` for activity, `foundation:createdAt`, `rdfs:label`, `foundation:receivedAt`…), NEVER `tx`. (4) With the dataset frozen, offset is stable again (offset only broke because a new row entered the top — the snapshot removes that). Return `{ items, next_cursor, has_more, snapshot_tx }`. The frontend pins snapshot_tx on page 1 and passes it back. NEVER load-all-then-slice, NEVER return an unbounded list. Rationale + revision recorded in the pagination ADR (`foundation:ArchitectureDecisionRecord_1781556688201`).
- ALWAYS make pagination explicit to the user: show that more results exist, how many are available when known, and a clear next/load-more control or page indicator; APIs must expose `has_more` and a continuation cursor. NEVER silently truncate a list/property/relation or present a partial page as complete.
- `tx` is the ordering key ONLY for create-immutable entities whose natural order IS creation-recency (`RawDataRecord`, `AIAPICall`, IMAP sync log): there `ORDER BY creation_tx DESC` is the domain key and keyset-by-creation_tx is correct. NEVER use `tx` as the ordering key for lists ordered by a mutable/domain key (AI models by default/alpha, conversations by activity).
- The pagination `snapshot_tx` and the realtime-replay `setSinceTx` share the value `MAX(tx)` but are DISTINCT in purpose (frozen past vs live future) — complementary, they converge in the UI. Do NOT conflate them.
- Bounded low-cardinality single-page lists (AI services/models pickers, agents, IMAP accounts, applicable properties) MAY freeze membership-as-of only and accept the current ordering key — there is no page 2 where instability shows. If such a list ever grows to real pagination, promote it to true snapshot. Apply the bound in SQL (`LIMIT`), NEVER load-all-then-slice.
- EXCEPTIONS (still NOT snapshot-paginated; a NEW one needs PO sign-off + an ADR note): (1) relevance-ranked search (`search` / `owl__search_entities` / `describe_property` query mode) — bounded top-N, re-ranked per query, NO cursor; bound applied in SQL. (2) graph/BFS traversals (`class_graph`) — bounded by `max_depth`, no linear cursor. (3) `chat__get_recent_messages` scroll-back stays offset — realtime replay-since-tx already covers incremental advance.
- For MCP tools, declare `snapshot_tx` (echoed) + the page param in the code `ToolTemplate` (`ai/functions/definitions.rs`); the AI MUST echo the returned `snapshot_tx` (and `next_cursor` where keyset) to keep the dataset frozen across pages. The MCP-exposed `inputSchema` is built from the code `ToolTemplate` (`mcp/mod.rs::to_mcp_tool`), NOT from the `foundation:MCPTool` individual (whose `foundation:inputSchema` is documentation-only and is NEVER read at runtime); after changing a tool's params the MCP CLIENT must re-list tools (reconnect) before sending the new arg.
- ALWAYS write scripts in Rust — never Node, Python, or shell.

## Visual style
- ALWAYS keep the blackboard canvas black and high-contrast; confine Oblivion-inspired cyan/ice colors to projected widgets and active UI elements.
- ALWAYS preserve the reviewed black canvas texture: an evenly spaced, subtle cyan dot grid across the full canvas behind widgets.
- NEVER tint all application surfaces cyan or soften foreground items into a washed-out glass look.
- ALWAYS keep the open-widgets panel floating over the blackboard, anchored to the bottom edge, content-height by default, with the chat's subtle translucent background.
- ALWAYS keep widget surfaces dark enough to preserve readable text over bright content; balance translucency and blur against contrast instead of letting light background widgets wash through.
- ALWAYS style Oblivion-inspired widget frames with restrained cyan corner marks and thin separators, preserving the black canvas and dark widget bodies.
- NEVER retain a continuous card outline or broad outer glow on projected widgets; corner marks define their frame.
- ALWAYS scale widgets progressively by stacking depth: the rearmost open widget is 30%, the frontmost is 100%, and intermediate widgets are evenly spaced; selecting one animates it to full scale with a subtle lift.
- ALWAYS place inspector property labels above their values to preserve horizontal space; align each value's left edge with its label and keep consistent vertical spacing between them.
- ALWAYS keep the chat panel and open-widgets menu visually consistent with widget frames: dark square surfaces and restrained cyan corner accents.
- ALWAYS define color and typography-size tokens only in `src/app.css`; components consume semantic tokens and never declare ad hoc colors or font sizes.
- ALWAYS ensure syntax-highlighting token colors are vivid and clearly distinguishable in the rendered inspector; if users cannot distinguish them, strengthen semantic design-system tokens in `src/app.css` and validate in the integrated browser.
- ALWAYS use a recognizable language-brand icon as the visible identifier when one is available; keep its name in a tooltip and accessible label instead of repeating it as visible text.
- ALWAYS define spacing tokens semantically in `src/app.css`; components consume those tokens for control internals, related items, field groups, and section separation instead of ad hoc spacing values.
- ALWAYS keep a field label visually close to its value/content; use minimal spacing within a related label-value pair and reserve larger spacing for separate fields or sections.
- ALWAYS keep widget-control glyphs aligned to the sidebar icon size and give touch-operated icon buttons at least a 44px target.
- ALWAYS right-align widget window controls and submenu actions with the same semantic gap and 44px touch targets for icon actions.
- ALWAYS show the complete editable entity title in the Inspector header; NEVER duplicate it as a field in the body.
- ALWAYS keep the inspector's editable entity title and status visible; missing either is a regression.
- ALWAYS style the inspector's entity class in the header as readable secondary context: smaller and less prominent than the entity title, but not faint or tiny.
- ALWAYS allow long inspector class names up to two compact lines, truncate overflow with an ellipsis, and expose the complete name in a hover tooltip.
- NEVER draw borders around text values in inspector fields; use spacing and typography to define the content instead.
- ALWAYS use restrained cyan line-and-point framing around widgets, with generous outer spacing, dark translucent surfaces, and uppercase for concise semantic labels; keep titles and body text in sentence case for readability.
- ALWAYS make the Oblivion frame language read as thin connected guide lines, sparse registration points, and measured spacing; NEVER rely on bright isolated corner brackets as the primary frame, broad colored borders, or near-opaque widget fills.
- ALWAYS extend widget guide lines to the frame edges and let registration points meet those edges; omit separate L-shaped corner strokes.
- ALWAYS keep conversation and open-widget panels free of corner brackets; use edge-spanning horizontal rules and endpoint marks instead of a boxed outline.
- ALWAYS place HUD point markers only at guide-line endpoints; NEVER add intermediate points along a frame edge.
- ALWAYS style native browser scrollbars through semantic design-system tokens: dark tracks, fine cyan indicators, square geometry, and orange active feedback.
- ALWAYS keep chat message bubbles on restrained near-black neutral surfaces; use cyan only for a fine streaming/status rule and reserve orange for selection or focus.
- ALWAYS place activity toasts in the upper-right corner and style them as compact, high-contrast dark HUD panels with readable body text and restrained status accents.
- ALWAYS give the application startup screen the same black high-contrast canvas, restrained cyan HUD framing, semantic status hierarchy, and orange only for focused actions.
- ALWAYS use the semantic Expanse-inspired orange selection accent to show the currently selected item and the user's keyboard focus; keep cyan for non-selected interactive accents.
- ALWAYS prioritize smooth, legible text on dark widget surfaces, including widgets scaled back in the depth stack.
- NEVER use bold font weights or italic font styles while the visual system is being established; communicate hierarchy through semantic size and color tokens.
- Comments: WHY only. NEVER write WHAT comments, commented-out code, or TODO/FIXME markers.
- NEVER suppress warnings or errors.
- NEVER add redundant wrapper functions — if existing functions cover it, don't wrap.
- NEVER leave dead code — unused imports, unreachable map keys/branches, orphan components or files, code only reachable by a path that never executes. If a fix exposes dead code, DELETE it in the same change.
- ALWAYS prefer shadcn-svelte components (`src/lib/components/ui/*` — Button, Card, etc.) over custom CSS/markup when a primitive exists. When a needed size/variant is missing (e.g. an icon button smaller than `icon-sm`), EXTEND the component's variant set (`buttonVariants`), NEVER a one-off ad-hoc CSS class on the call site.
