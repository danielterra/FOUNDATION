# Changelog

All notable changes to FOUNDATION will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.18.0] - 2026-05-03

### Added

- **Distribuição via MCPB para Claude Desktop**: foundation.mcpb embarcado como Tauri resource (manifest.json + bridge.js Node que faz ponte stdio→HTTP); script `scripts/build-mcpb` em Rust empacota o bundle e sincroniza versão do manifest com `package.json`; novo botão "Conectar ao Claude Desktop" no SettingsPanel copia o `.mcpb` para Downloads e abre o Explorer (não há deep link nem associação `.mcpb` no SO, principalmente na versão MSIX no Windows)
- **Comando `ai__delete_api_key`**: retrata a credencial e desfaz o link `apiKey` no service AI

### Fixed

- **Filtros `exists`/`not_exists` e numéricos no MCP `search`**: parser descartava silenciosamente filtros sem campo `value` (caso exists/not_exists) e filtros com `value` JSON Number; agora extrai `operator` antes e converte `Number`/`Bool` via `to_string()`. Comparações `>=`/`<=`/`>`/`<` em `xsd:decimal` usavam comparação lexicográfica de string; agora `build_value_condition_fragment` detecta numéricos via `f64::parse`, usa `CAST(... AS REAL)` em `object_value` e inclui `object_number` e `object_integer` no OR
- **Importação IMAP captura múltiplos anexos**: `part_is_attachment` exigia estritamente `Content-Disposition: attachment`, ignorando anexos com `inline; filename=...` ou apenas `name=` no Content-Type (caso comum em e-mails de bancos como Nubank); agora detecta por qualquer indicador de filename e `extract_attachment_filename` cai em fallback de Content-Type quando Content-Disposition está ausente
- **`queryConfig` respeita `orderBy` e `limit`**: campos eram silenciosamente descartados pelo serde (struct `QueryConfig` não os declarava); agora `evaluate_query` gera `ORDER BY (subquery) ASC|DESC` e `LIMIT N`. Resolve o caso `previousMonthBudget` retornar todos os candidatos em vez de só o mais recente
- **Modelo de imutabilidade no query worker**: `update_query_property_triples` lia `current` da view `triples_current` (que retornava valores históricos) e usava `retract` para "atualizar" valores multi-valorados; agora lê via `WHERE tx = (SELECT MAX(tx) ...)` e apenas insere novo TX (retract permanece só para limpeza). View `triples_current` corrigida para filtrar por MAX(tx) na definição inicial — alinha com a migração que já fazia DROP+recriação
- **Cascade de fórmulas com domain==range**: `create_reverse_aggregation_recalc_jobs` decidia direção comparando `source_prop.domain` com tipo da instância alterada — para `previousMonthBudget` (domain == range == MonthlyBudget) caía sempre no branch errado e nunca recomputava o `openingBalance` do mês seguinte. Agora usa o domain do dono da fórmula (`agg_owner_class`) para detectar direção
- **Substituição de valores negativos em fórmulas**: `evaluate_formula_for_instance_raw` produzia expressões como `"a + -3"` que `eval_expr` não aceita (unary minus infixo não suportado); o erro era engolido em `formula_instance_errors` e o triple do resultado nunca era escrito. Agora valores substituídos sempre são envoltos em parênteses
- **Inspector separa forward de backlink**: `PropertyList` agrupava por IRI da propriedade — forward e backlink (que compartilham o mesmo IRI mas têm semânticas opostas) eram fundidos no mesmo grupo. Agora a chave de agrupamento usa sufixo `__backlink` quando `groupTotal != null`, separando visualmente as duas direções

### Refactored

- **Detecção de "is_local" de AI services**: agora baseada na ausência de `foundation:apiBaseUrl` em vez de `foundation:apiKey` — providers locais podem ter chave (auth em LM Studio remoto), mas providers remotos sempre têm baseUrl
- **SettingsPanel consolidado**: provider + chave + modelo numa única seção "AI" com dirty/save unificado; remove modal MCP do home (toda configuração migrada para o SettingsPanel)
- **Scripts de release otimizados (276s → 26s, 10.6× mais rápido)**: `dump-ontology` agora gera multi-row INSERTs (batch de 500) — arquivo SQL caiu de 22MB para 9.5MB e o load do dump ficou ~170× mais rápido (de 257s para 1s); `verify-ontology` paraleliza a comparação de 4627 subjects via Rayon (8 threads), drop+recreate de índices durante bulk load, PRAGMAs de durabilidade desabilitados em DB temporário, fallback para `Property::get` quando subject não é Class/Individual, e `triples_current` em vez de `triples WHERE retracted=0` para alinhamento com o estado atual da ontologia

## [0.17.1] - 2026-05-01

### Added

- **Pasta de dados configurável**: primeiro acesso exibe wizard para o usuário selecionar a pasta de dados; configuração persistida em `config.json`; comandos Tauri `settings__is_folder_configured`, `settings__get_foundation_dir`, `settings__save_foundation_dir` e `settings__set_foundation_dir`
- **Servidor MCP dual (HTTP + HTTPS)**: porta 47177 HTTPS com certificado TLS auto-gerado e persistido; porta 47178 HTTP plain para clientes locais como Claude Code; README e docs atualizados com instruções para Claude Desktop Windows/macOS
- **Wizard de onboarding redesenhado**: fluxo em 4 passos em pt-BR — nome, e-mail IMAP (iCloud, Outlook, Yahoo, custom), escolha de IA (chat interno ou via MCP), confirmação; teste de conexão IMAP inline; `setup__reset` reabre o wizard
- **Analytics com consentimento do usuário**: integração PostHog com `opt_out_capturing_by_default`; checkbox de consentimento no último passo do wizard; toggle de privacidade nas Configurações; variáveis públicas injetadas via CI
- **Modal de conexão MCP**: botão "Conectar via MCP" na barra superior; instruções para Claude Desktop com bloco de configuração copiável; paths por OS; hint para configurar API key quando chat está desabilitado
- **Chat condicional por API key**: botão do chat e overlay só aparecem quando há API key configurada; verificação refeita ao fechar Configurações

### Fixed

- **Resiliência de execuções de automação**: watchdog periódico (10 min) retoma execuções travadas sem necessidade de restart; `ActiveExecutions` evita recuperação dupla; `resume_workflow_execution` agora emite `automation-execution-finished` para o frontend
- **WorkflowExecutionWidget**: listeners sempre registrados independente do status inicial; listener `entity-updated` como fallback de reload; crash ao renderizar mensagens corrigido (prop `unit` vs `message`); conversor `messagesToUnits` transforma blocos brutos no formato esperado pelo `ChatMessageBubble` com suporte a markdown
- **Inicialização do banco por presença de ontologia**: testa existência de `foundation:Person` em vez de existência do arquivo; `PRAGMA foreign_keys = OFF` durante import bulk evita falhas de ordem de inserção; `TAURI_ENV_DEBUG` removido da lista de flags de skip
- **Build mode**: encerra imediatamente quando Tauri CLI executa o binário para coleta de schemas; fallback para diretório de dados quando `Documents` não existe
- **CI/CD**: macOS deployment target via `tauri.conf.json` e `GITHUB_ENV`; `permissions: contents: write` para criação de releases; cache npm e `CARGO_INCREMENTAL=0`

### Refactored

- **Sync IMAP**: remoção da extração agentic de entidades por e-mail (`ai_extraction.rs`) — simplifica o worker e elimina dependências de contexto de IA no loop de sincronização

## [0.17.0] - 2026-04-26

### Added

- **Identidade colaborativa, deduplicação de extração e design system**: identidade do usuário centralizada, dedup automática de entidades extraídas e atomização do design system
- **Integração IMAP**: sincronização IMAP com extração agentic de emails para entidades da ontologia, e fix de backlinks
- **Ferramentas MCP de lousa**: novas ferramentas MCP para manipular widgets da lousa, widget padrão por classe, e suporte a modelo local
- **Local AI harness**: runtime `llama-cpp` embarcado, `AgentTrace` para registrar execuções, scheduler com catch-up de jobs perdidos e seletor de modelo no header do chat
- **IRIs clicáveis nas bolhas de chat**: IRIs viram pílulas com navegação direta para o inspetor
- **`restore_*` MCP tools**: `restore_individual`, `restore_class` e `restore_property` para reverter retractions do modelo append-only
- **SSE streaming + `ask_question`**: respostas do agente em streaming, UI de perguntas dedicada, refator do engine e agent picker
- **Toast system + thinking toggle**: notificações in-app, toggle do extended thinking, recuperação silenciosa de erros
- **Cascade fixes**: correções em deleção em cascata para preservar consistência

### Changed

- **Modelo append-only Datomic-style**: store imutável com retração explícita; valores históricos preservados, queries usam tx mais recente como fonte da verdade
- **Pesquisa consolidada via Tantivy**: substituição completa de buscas SQL `LIKE` por índice BM25 do Tantivy
- **Tool loop e streaming nas bolhas de chat**: refator da execução de ferramentas e renderização incremental
- **Licença AGPL-3.0**: troca de MIT para `GNU AGPL-3.0-only` para impedir fork proprietário/SaaS extrativo
- **README e Development Guide**: features alinhadas ao registro `foundation:SoftwareFeature`, prerequisites explícitos (Rust + Tauri 2 system deps), Quick Start corrigido

### Fixed

- **Campos calculados — datatype e precisão IEEE 754**: resultados de fórmula/agregação eram gravados como `xsd:string` com ruído tipo `-20389.580000000024`; agora gravam `xsd:decimal`/`xsd:integer` (conforme `rdfs:range`) com arredondamento de 10 casas que suprime o ruído. 3 sites: `formula_worker.rs`, `formula.rs`, `aggregation.rs`
- **NumberFlow renderizando dígitos crus em release builds**: CSP `style-src nonce-*` injetada pelo Tauri bloqueava o `<style>` do Shadow DOM; `BigNumberCard.svelte` agora repassa o nonce
- **Propriedades numéricas vazias = zero**: cálculos antes falhavam quando uma referência estava ausente; agora considera `0` (e o inspetor permite gravar `0` explicitamente)
- **Texto preservado em mensagens `speak`**: blocos de texto deixavam de ser renderizados quando havia answer associada
- **Build limpo + DMG**: `tauri:build` gera `app,dmg`; `build:release` agora limpa o bundle dir antes de buildar (evita falha por DMG residual); versão centralizada em `package.json` e lida pelo `tauri.conf.json`; feature `devtools` habilitada em release; `LICENSE` (AGPL-3.0) criado

### Refactored

- **Code review compliance**: violações de camada corrigidas, hierarquia de classes ajustada, labels normalizadas para pt-BR
- **`scripts/release-local`**: script Rust agora limpa o bundle dir, deps órfãs removidas; `scripts/build-release.sh` (caminho de target obsoleto) deletado

## [0.16.0] - 2026-03-31

### Added

- **Ontology-driven widget registry**: Widget types are now defined as `foundation:WidgetType` individuals in the ontology — no Rust code changes required to register new types
- **speak.iris**: `speak` tool accepts an `iris` parameter to auto-show widgets for referenced entities when communicating with the user
- **Automation run toast**: Inspector shows a live toast notification when an automation is triggered, updating in real time as steps progress and resolving on completion or failure
- **Numeric formula operators**: Formula properties support numeric operators with validation, recalculation, and NumberFlow animated display
- **Adaptive thinking**: AI uses extended thinking budget dynamically based on task complexity
- **Relative timestamps**: Chat messages show relative time (e.g. "2 min ago") instead of absolute timestamps
- **speak tool**: Dedicated AI output tool for communicating with the user, with 144-character limit and optional entity widget display

### Fixed

- **Widget cascade positioning**: Widgets opened via `speak.iris` now cascade from top-left instead of appearing far off-screen
- **Automation button in Inspector**: Inspector correctly shows automation buttons for classes with linked automations after fixing widget type registry lookup
- **Recovery loop infinite cycling**: Concurrent recovery calls on the same conversation no longer create orphaned tool pairs that loop indefinitely
- **Inspector header border-bottom**: Hidden when inspector is minimized to remove visual artifact
- **rdf:Property type upgrade**: Improved handling of RDF property type assertions

### Refactored

- **Consolidated individual/class AI functions**: Merged redundant AI function handlers with retention policy
- **actions-bar fix**: Corrected actions bar rendering and interaction

## [0.15.0] - 2026-03-22

### Added

- **Inline cardinality editor**: Min/max cardinality controls on property rows in the class inspector; cardinality badge shows current constraint (e.g. `1..*`)
- **System entity locking**: OWL-layer guard prevents mutations to system-locked entities; inspector UI shows lock state with toggle for system entities; migration binary to apply locks to existing ontology
- **Search overhaul**: Improved search relevance, automation tool rename, inspector and chat enhancements
- **Automation IO handles**: Replaced MetaProcess with Automation IO handles and NOVAMessageTask
- **Multi-concept handles**: Class-compatible edge routing with multi-concept handles on automation nodes
- **Automation graph visual overhaul**: Side-tab nodes and subprocess IO visualisation
- **Full property editing in inspector**: References, dates, and string properties fully editable inline
- **Meta-process node enrichment**: Inspector meta edit, icon consolidation, trigger types and `renders_component`

### Fixed

- **Recovery loop**: Break recovery loop when conversation ends with assistant message after sanitization — prevents infinite 400 error loop with Claude 4
- **Inspector class property editing**: SQL safety fixes and a11y warnings resolved
- **Subconscious context, entity chips**: DB lock fixes and camera fix

## [0.14.0] - 2026-03-18

### Added

- **AgentTask context injection**: Input IRIs are fetched as full `Individual` records and embedded in the agent's prompt under `## Input Data`, eliminating hallucination of entity properties
- **`task_complete` tool**: AgentTask can now call `task_complete(success, output_iris[], message)` to explicitly signal completion with typed outcome; tool is isolated to AgentTask and not exposed globally
- **`foundation:allowedTools`**: New property on `automation_AgentTask` to restrict which MCP tools the agent may invoke; empty means all tools available
- **`foundation:triggeredBy`**: New property on `WorkflowExecution` linking back to the input instance that triggered the run — enables backlinks in the inspector
- **Automation runner commands**: `automation__run(processIri, inputIri?)` and `automation__find_for_types(typeIris)` Tauri commands for triggering automations programmatically
- **Inspector actions bar**: Fixed bottom bar in InspectorWidget with full-width buttons for running automations; shows "Run" for automations without `inputClass`, or per-automation buttons matched to entity type
- **`applicableAutomations` in MCP**: `describe_individual` and `describe_class` now return automations applicable to the entity/class so the AI knows what actions are available
- **Subconscious context**: Entity context injected into AI responses, entity chips in chat
- **AutomationWidget**: Flow diagram widget for visualising and inspecting process automations

### Changed

- **Process executor**: `run_process` now accepts `input_iri: Option<String>` seeded into `ExecutionContext` as `inputIRIs`
- **Toast animations**: Running automation toast uses pulsing purple (`--color-transition`) background instead of broken spinner; semantic color variables applied throughout

### Fixed

- **DB lock contention**: Fixed multiple database lock issues
- **Camera file summaries**: Camera capture now generates file summaries correctly
- **Inspector auto-refresh**: Inspector reloads entity data when related events are emitted

## [0.13.0] - 2026-03-17

### Added

- **`verify-code-iris` script**: Scans all Rust source files for hardcoded `foundation:*` IRIs and validates each against the live database before the ontology dump — catches missing core entities at release time
- **9 missing OWL properties**: Created `allowedStatus`, `calledElement`, `credentialUsername`, `email`, `endpointPath`, `eventKey`, `hasCredential`, `messageEventOf`, `requestTimeout` as proper ontology properties

### Changed

- **`dump-ontology`**: `bootstrap_registry` now includes entities with `setup` origin (in addition to `foundation:ontology:*` and `core`), and explicitly excludes installation-specific IRIs (`ThisUser`, `ThisFoundationInstance`, etc.) from seeding
- **Release skill**: Updated to 10 numbered steps; Step 4 runs `verify-code-iris` before dumping ontology

## [0.12.0] - 2026-03-17

### Added

- **Generic inspector auto-refresh**: Inspector now automatically reloads when new backlinks or properties are added to the viewed entity — works for any entity, not just chat messages
- **Multi-attachment fix**: All camera frames are now correctly linked via `hasAttachment`; previously only the last frame was persisted due to `add_property` overwrite behavior
- **Camera frame summaries**: AI summaries for camera frames now focus on emotional/body state in 1-3 words; environment description only on the first frame
- **Tantivy concept-filtered search**: BooleanQuery with concept field allows concept-scoped full-text search
- **Usage-based relevance boost**: Tantivy search results boosted by entity access frequency via fast field
- **Per-conversation blackboard**: Widgets scoped to active conversation via blackboard state
- **Concept properties in Inspector/MCP**: `get_concepts` and Inspector now expose concept-level properties
- **Cascade delete on forget_concepts**: Deleting a concept now removes all its instances
- **Formula auto-recalculation**: Formula properties recomputed automatically on `learn_properties` and `learn_things`
- **`get_concept_graph` and `get_process` MCP tools**: Dynamic node status in process graphs
- **Widget resize handle**: Drag-to-resize for blackboard widgets
- **MetaBoundaryCondition, HTTP response events, pan gestures**: Extended process modelling and canvas interaction
- **MetaProcess graph enrichment**: Trigger types, `renders_component`, and expressive node icons
- **InclusiveGateway and BoundaryEvent node types**: Extended BPMN-style flow support
- **MetaProcess widget**: BPMN flow diagram with status badges
- **Widget window state persistence**: Widget position and size persist across sessions

### Fixed

- **Camera frame image preview**: Generic file icon shown instead of thumbnail; fixed by adding filename extension fallback for MIME detection
- **Inspector backlink state**: Backlink groups no longer collapse when inspector reloads
- **Spurious inspector reload**: Opening a backlink in a second inspector no longer reloads the first
- **Inspector auto-refresh on new backlinks**: Dedicated `entity-referenced` event prevents cascade reloads from access counters and unrelated writes
- **Minimized inspector header padding**: Tightened padding and action gap
- **`file://` icons in `learn_things`**: Extended meta-property bypass to allow file-protocol icon URIs

### Refactored

- **`PropertyList.svelte`**: Extracted `FileGrid.svelte` and `PropertyEditForm.svelte` to reduce file size
- **`ChatWindow.svelte`**: Extracted `ChatHeader`, `ChatApiKeySetup`, `ChatErrorBanner`, `ChatMessageList` components
- **MCP tool descriptions**: Rewritten to lead with when-to-use guidance
- **Tantivy BM25 full-text index**: Replaced SQL `LIKE` search

## [0.11.1] - 2026-03-12

### Refactored

- **Widget command namespacing**: Renamed `blackboard__*` and `inspector__update_property` Tauri commands to `widget_blackboard__*` and `widget_inspector__*` following the Rust double-underscore namespace convention
- **Startup component**: Extracted boot screen logic from `src/routes/+page.svelte` into a dedicated `Startup.svelte` component for semantic clarity
- **Dead code removal**: Removed unused `ai__generate` command, `process_automation` module, and Portuguese inline comments; cleaned up `console.log` debug calls from canvas page

## [0.11.0] - 2026-03-10

### Added

- **Required field inheritance**: Concepts now automatically inherit required fields from parent concepts; `get_concepts` and `learn_things` validate against the full inherited + own set
- **Superclass enforcement**: Creating a concept without a superclass is rejected; updating a concept to remove all superclasses is rejected; `owl:Thing` restrictions are always inherited as the implicit root
- **Concept deletion guard**: Deleting a concept that has dependent subclasses is now rejected — subclass references must be removed first

### Changed

- **Property type inference**: AI tools now infer property types from the ontology when not explicitly specified
- **Cancel AI support**: AI operations can now be cancelled mid-execution

## [0.10.0] - 2026-03-09

### Added

- **Process automation**: BPMN 2.0 workflow engine with connector commands, widgets, AI agent tasks, timers, and parallel execution
- **Properties management tools**: New `learn_properties`, `remember_properties`, and `forget_properties` AI tools for fine-grained OWL property management independent of concept definitions
- **Inline markdown editing**: String properties in the Inspector now support inline markdown editing

### Fixed

- **Scheduler**: IRI navigation, startup timing, auto-reload, and Paused status now behave correctly
- **Date/datetime conversion**: ISO date and datetime values from AI are now converted to Unix milliseconds before writing to OWL
- **Mermaid lines in WebKit**: Explicit SVG dimensions fix broken connector lines in Safari/WebKit
- **Property operation ordering**: `remove_properties` is now executed before `upsert_properties` in `learn_things` to avoid conflicts

### Refactored

- Renamed `add_details` to `upsert_details` across all AI tool definitions for clarity
- Split `eavto/query.rs` (1502 lines) into `query/mod.rs`, `query/find.rs`, and `query/search.rs`
- Replaced production panics (`.expect()`) with proper `Result` propagation

### Docs

- Added widget system, automation system, and development guides under `docs/`

## [0.9.1] - 2026-03-08

### Fixed

- **rdfs:range class validation**: ObjectProperty writes now enforce that the value IRI is an instance of the declared range class (with subclass support); previously any existing IRI was accepted regardless of type
- **Explicit add/remove API for concept details and thing properties**: Corrected API behavior for adding and removing concept details and thing properties
- **Widget cascade offset removed**: Widgets no longer shift position on each open

## [0.9.0] - 2026-03-07

### Added

- **Per-agent chat configuration**: Each conversation now loads its own agent config (model, API key, system prompt, timeout) from the linked agent entity via `foundation:handledBy`
- **WhatsApp-style chat header**: Agent avatar (clickable → opens Inspector), agent name, and icon-only action buttons replace the old plain title bar
- **Conversation rename**: Inline rename input in the conversation bar with confirm/cancel; renamed labels persist across app reloads
- **`chat__rename_conversation` command**: New Tauri command to rename a conversation by updating its `rdfs:label` triple
- **Empty property stubs in Inspector**: Properties defined on a class now appear in the Inspector even when the individual has no value set, making it easier to fill in missing data

### Changed

- **Conversation bar**: Moved to its own row below the header; displays conversation label instead of IRI; extracted into a dedicated `ConversationBar` Svelte component
- **`chat__create_conversation`**: Default label is now a human-readable date string (e.g. `Conversation Mar 07, 2026 14:30`) instead of `"New Conversation"`

### Fixed

- Fixed `required_fields` in `learn_concepts` failing when referencing properties created in the same call
- Fixed `allowedStatus` validation to verify IRI existence and icon before setting

## [0.8.0] - 2026-03-07

### Added

- **Mermaid diagram widget**: Interactive Mermaid diagram widget on the blackboard that loads content from a linked entity, supports pan/zoom (up to 5000%), and updates live when the entity's `diagramSource` property changes
- **Fit-to-view on widget open**: Diagram automatically fits to the canvas when the modal is opened
- **InspectorWidget dynamic actions**: Inspector header now shows action buttons for compatible widget types, allowing users to open entity-bound widgets directly from the inspector
- **`widget__list_definitions` command**: New Tauri command that exposes available widget definitions from the ontology, optionally filtered by concept IRI
- **Blackboard context in AI prompts**: Current blackboard state (widget type, concept IRI/name, thing IRI/name) is injected as a non-cached system block into every AI API call so the assistant knows what the user is viewing

### Fixed

- Fixed `rowid` vs `id` references in triple queries causing incorrect results
- Fixed Svelte a11y warnings in chat and widget components

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
