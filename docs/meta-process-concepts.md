# MetaProcess Concepts

This document describes the ontology concepts used to model `foundation:MetaProcess` — a general-purpose process modelling layer for describing any kind of process: software systems, business workflows, operational procedures, or anything that can be expressed as a structured sequence of events, decisions, and actions.

A MetaProcess describes **what should happen**, not how it is implemented. It is the specification; the running `foundation:Process` is the execution.

---

## Process Architecture

A MetaProcess can model any scope — an entire application lifecycle, a single business workflow, an operational runbook, or a sub-process within a larger process. For Foundation itself, the root process is `AppStart`, which describes the full application lifecycle from launch to shutdown.

Every `MetaSubProcess` referenced within the main process is defined as its own separate `MetaProcess`. This keeps each process focused and independently understandable.

**Sub-process communication is always via `foundation:MetaIntermediateEvent` carrying a payload of `Array<IRI>`:**

- **Input** — the parent raises a `foundation:MetaIntermediateEvent` (optionally with IRIs) to invoke the sub-process
- **Output** — the sub-process raises a `foundation:MetaIntermediateEvent` (optionally with IRIs) back to the parent when done

The payload `Array<IRI>` may be empty when the event itself is the only meaningful signal (e.g. `cancelled`, `closed`). The parent process uses the returned event and its payload to decide how to continue.

---

## Task vs SubProcess Design Rule

**A `MetaSystemTask` or `MetaUserTask` must produce the same `MetaEventConcept` on every execution.** If a node's success path can produce different output concepts, it must be modelled as a `MetaSubProcess` instead.

- **Same output concept always** → Task (`MetaSystemTask` or `MetaUserTask`)
- **Output concept varies by branch** → SubProcess (`MetaSubProcess`)
- **Errors / exceptions** → `MetaBoundaryEvent` attached to the task (never an internal branch)

Tasks take input and produce output without branching. All branching is external — handled by the gateway that follows the task.

---

## Data Contracts: `inputConcept` and `outputConcept`

Every `MetaFlowNode` has typed data contracts defined via:

- **`foundation:inputConcept`** — the ontology class this node expects to receive
- **`foundation:outputConcept`** — the ontology class this node produces

These point directly to OWL class IRIs (e.g. `foundation:MCPRequest`, `foundation:MetaProcess`) — no wrapper instance needed. These properties apply to all node types, including gateways and events, because every node receives something and passes something on. This is the design-level specification of the data flowing through the graph.

---

## Verifiable Postconditions: `should`

`foundation:should` is a required property on `foundation:MetaAbstractTask` (inherited by all task types). It points to a `foundation:MetaShould` instance — a structured behavioral contract that captures both dimensions:

- **`outcome`** — what the task produces, as a verifiable postcondition (the *what*)
- **`benefit`** — why the task exists, the value it delivers (the *why*)

There are no internal conditions inside a task; a task takes input and must produce output that conforms to the `outcome`. The `benefit` gives future readers (and AI) the motivation behind the task, not just its mechanics.

**Examples:**
- outcome: `"SQLite database file exists and all schema tables are structurally valid"` / benefit: `"Provides the persistent storage layer that all Foundation data depends on"`
- outcome: `"MCP server is running and all tools are discoverable"` / benefit: `"Exposes Foundation's knowledge tools to AI assistants"`

---

## User Task Instructions: `instructions`

`foundation:instructions` is a required property on `foundation:MetaUserTask` only.

It provides **step-by-step guidance** for a person (or AI agent) on how to reproduce the expected outcomes. System tasks don't need instructions because the system executes them deterministically.

---

## Class Hierarchy

```
MetaFlowNode
├── MetaAbstractEvent
│   ├── MetaStartEvent
│   ├── MetaEndEvent
│   └── MetaIntermediateEvent
├── MetaAbstractTask
│   ├── MetaSystemTask
│   ├── MetaUserTask
│   └── MetaSubProcess
├── MetaAbstractGateway          ← uses gatewayCondition → MetaGatewayCondition
│   ├── MetaExclusiveGateway
│   ├── MetaEventBasedGateway
│   └── MetaInclusiveGateway
├── MetaParallelGateway          ← uses nextNode directly (no conditions)
├── MetaGatewayCondition         ← condition node between a gateway and its target
├── MetaBoundaryEvent            ← exception event node attached to a task (event type only)
└── MetaBoundaryCondition        ← routing logic for a boundary catch (mirrors MetaGatewayCondition)
```

**Supporting concepts (not flow nodes):**
- `MetaShould` — behavioral contract attached to a task via `should` (outcome + benefit)
- `MetaGatewayConditionOperator` — controlled vocabulary of condition operators

---

## `foundation:MetaStartEvent`

The entry point of a process. Has a `triggerType` (see `MetaEventTrigger`) describing what initiates it.

**Main application process** — has a named `foundation:MetaStartEvent` representing the real-world trigger that begins the entire lifecycle:
```
AppStart([MetaStartEvent: App Start])  triggerType: App Launch
```

**Sub-process** — has a generic `Start` event, triggered by the parent process calling it. The parent passes a `foundation:MetaIntermediateEvent` as input, and the sub-process begins from that:
```
Start([MetaStartEvent: Start])
```

---

## `foundation:MetaEndEvent`

The terminal point of a process. Marks where the process concludes. A process may have multiple end events representing different outcomes. For example, `AppStop` is reached when the user closes the application.

```
AppStop([MetaEndEvent: App Stop])
```

---

## `foundation:MetaIntermediateEvent`

An event that occurs **during** the execution of a process. Has a `triggerType` (see `MetaEventTrigger`) describing what fires it.

There are two modelling patterns:

**Independent event** — fires at any time after a milestone is reached. Connected to its context node with a dashed arrow (`-.->`) to signal it is not on the fixed sequence. The following task reacts to it when it fires.

**Catching event** — when a task leads *to* a `MetaIntermediateEvent`, the flow waits at that node until the trigger fires before continuing. This models "listen for X" behaviour.

```
Start MCP Server → [MCP Request Received] (triggerType: HTTP Request) → EventBasedGateway
```

A catching `MetaIntermediateEvent` does **not** need a loop-back edge from the tasks downstream of it — it fires independently on each new trigger. The execute tasks are terminal; the event re-fires on its own for the next request.

**Properties:**
- `triggerType` — `foundation:MetaEventTrigger` instance describing what fires this event

**Examples from AppStart:**
- `MCP Request Received` — fires when an HTTP request arrives at the MCP server (triggerType: HTTP Request)
- `Formula Recalculation` — triggered when the background worker processes pending jobs (triggerType: System Signal)
- `Entity Created` — raised when a new entity is persisted (triggerType: System Signal)
- `Entity Updated` — raised when an existing entity changes (triggerType: System Signal)
- `AI Processing Status` — raised during AI inference (triggerType: System Signal)

Each intermediate event is followed by a node that handles the reaction (e.g. a `MetaSystemTask` or `MetaEventBasedGateway`).

---

## `foundation:MetaEventTrigger`

A controlled vocabulary of what can initiate a `MetaStartEvent` or `MetaIntermediateEvent`.

| Instance | When to use |
|---|---|
| `App Launch` | The OS launches the application |
| `HTTP Request` | An inbound HTTP request arrives (e.g. MCP server) |
| `CRON Schedule` | A scheduled timer fires |
| `System Signal` | An internal system signal (background worker, IPC, etc.) |
| `User Action` | Direct user interaction |

---

## Data flow types

`inputConcept` and `outputConcept` point directly to OWL class IRIs from the ontology — no wrapper needed. Use any existing concept that represents the data flowing through that node.

**Examples:**
- `foundation:MCPRequest` — the payload arriving at the MCP event-based gateway
- `foundation:MCPResponse` — the result produced by each MCP tool task
- `foundation:MetaProcess` — an entity IRI returned from a process lookup

---

## `foundation:MetaSystemTask`

A task performed entirely by the system, without user interaction. It may read data, write to the database, start background services, or render a UIComponent.

When a `foundation:MetaSystemTask` renders something visible to the user, that is described in the task's description — there is no separate concept for rendering.

**Required:** `foundation:should` postconditions describing the verifiable output.

**Examples:**
- `Initialize System` — starts logging, plugins, and services
- `Create Database` — initializes the SQLite schema on first run
- `Queue Pending Calculations` — enqueues interrupted formula jobs from the previous session
- `Show Inspector Widget` — renders the Inspector UIComponent in response to an entity creation event

---

## `foundation:MetaUserTask`

A task where the **user performs an action**. The system may present a UIComponent to support that action, but the task is only complete when the user provides input or makes a decision.

**Required:** `foundation:should` postconditions + `foundation:instructions` describing how to achieve the expected outcomes.

**Examples:**
- `User completes Setup Wizard` — first-run onboarding, user fills in their name, email, and AI preferences

---

## `foundation:MetaSubProcess`

A node that delegates execution to a separate `foundation:MetaProcess` (referenced via `foundation:invokesProcess`). A `foundation:MetaSubProcess` encapsulates its own internal flow and is completely opaque to the caller.

Use a `MetaSubProcess` instead of a task when the success path can produce **different output concepts** depending on what happens inside.

Communication with the parent happens exclusively through `foundation:MetaIntermediateEvent` with an `Array<IRI>` payload:
- The **parent invokes** the sub-process by raising an input event (payload may be empty)
- The **sub-process responds** by raising an output event back to the parent (payload may be empty or carry entity IRIs)

**Examples from AppStart:**
- `Home` — invoked with no payload; returns events like `search:shortcut`, `widget:open`, `chat:open`, `app:close`
- `Search` — invoked with no payload; returns `search:selected` with an array of entity IRIs, or `search:cancelled` with empty payload
- `Chat with AI` — invoked with no payload; returns `chat:closed` with empty payload when the user dismisses the panel

---

## `foundation:MetaExclusiveGateway`

A decision point where **exactly one** outgoing path is taken based on a condition. Each outgoing branch is represented by a `foundation:MetaGatewayCondition` node.

**Examples:**
- `Database exists?` — routes to the new database setup path or directly to normal startup
- `First run?` — routes to the Setup Wizard or directly to Home

---

## `foundation:MetaEventBasedGateway`

A gateway that **waits for one of several events** to occur and routes to the branch corresponding to the first event received. Each outgoing branch is represented by a `foundation:MetaGatewayCondition` node whose `conditionValue` identifies the triggering event.

Unlike the Exclusive Gateway, the routing decision is not based on a data condition evaluated at runtime — it is based on whichever external event arrives first.

**Example from AppStart:**
- `MCP Tool Events` — waits for one of the MCP tool calls (`learn_things`, `remember`, `get_things`, etc.) and routes to the corresponding handler task.

---

## `foundation:MetaInclusiveGateway`

A gateway that **activates all outgoing branches whose conditions evaluate to true**. Unlike the Exclusive Gateway (exactly one), one or more branches may fire. Each outgoing branch is represented by a `foundation:MetaGatewayCondition` node.

Used when multiple parallel outcomes are possible based on overlapping conditions.

---

## `foundation:MetaParallelGateway`

A gateway that **splits the flow into multiple concurrent paths**, all of which run simultaneously without any conditions. Uses `foundation:nextNode` directly — no `MetaGatewayCondition` nodes are involved.

A matching parallel gateway can be used as a join to wait for all paths to complete before continuing.

Rendered as a circle with a `+` symbol to visually distinguish it from conditional gateways.

**Example from AppStart:**
- At `AppStart`, the flow splits into `Start MCP Server` (runs independently for the full application lifetime) and `Initialize System` (the main startup sequence). These two paths never rejoin — the MCP server runs until `AppStop`.

---

## `foundation:MetaGatewayCondition`

A condition node that sits **between a conditional gateway and its target**. Each condition node represents one outgoing branch of an `ExclusiveGateway`, `EventBasedGateway`, or `InclusiveGateway`.

Properties:
- `conditionOperator` — reference to a `foundation:MetaGatewayConditionOperator` instance (controlled vocabulary, not a free string)
- `conditionValue` — the value being compared against or the event name being matched
- `nextNode` — the target flow node to activate when this condition is met

Rendered as a yellow pill node showing `{operator} {value}` (e.g. `equals true`, `equals learn_things`).

**Required fields:** `hasStatus`, `conditionOperator`, `conditionValue`, `nextNode`

---

## `foundation:MetaGatewayConditionOperator`

A controlled vocabulary for condition operators used in `MetaGatewayCondition`. Replaces free-string operator values for robustness and consistency.

| Instance | Plain-text label | Meaning |
|---|---|---|
| `Equals` | `equals` | Exact equality |
| `NotEquals` | `not equals` | Inequality |
| `GreaterThan` | `greater than` | Numeric/ordinal comparison |
| `LessThan` | `less than` | Numeric/ordinal comparison |
| `Matches` | `matches` | Pattern or event name match |
| `Arrow` | `→` | Directional / unconditional route |

The Rust backend resolves the operator IRI to its `rdfs:label` before sending it to the frontend (same pattern as `hasStatus`).

---

## `foundation:MetaBoundaryEvent`

An exception event node **attached to a task** via `foundation:boundaryCondition` (as a `MetaBoundaryCondition`). Responsible only for describing the *type* of exception — the routing logic lives in `MetaBoundaryCondition`.

Properties:
- `eventType` — the kind of exception: `error`, `timer`, `signal`, or `message`

Rendered as a dashed-border pill node. The icon changes by event type:
- `error` → error icon (red)
- `timer` → schedule/clock icon
- `signal` → cell tower icon
- `message` → mail icon

---

## `foundation:MetaBoundaryCondition`

A condition node that sits **between a task and its boundary handler**, attached to the task via `foundation:boundaryCondition`. Mirrors `MetaGatewayCondition` — holds the condition that triggers the catch and routes to the handler.

Properties:
- `conditionOperator` — a `MetaGatewayConditionOperator` instance (e.g. `throws`)
- `conditionValue` — the error or signal being caught (e.g. `entity_not_found`)
- `nextNode` — the handler task or end event to route to when this condition fires

Rendered as a red pill node showing `{operator} {value}`.

**Example:** A `MetaBoundaryCondition` on `Execute blackboard_update` with `throws entity_not_found` routes to `Return MCP Error Response`.

---

## Relationship to Runtime Concepts

| Ontology IRI | Runtime equivalent |
|---|---|
| `foundation:MetaProcess` | `foundation:Process` |
| `foundation:MetaSubProcess` | A `foundation:Process` invoked by another via `foundation:invokesProcess` |
| `foundation:MetaSystemTask` | A step executed by the Rust backend or Svelte frontend |
| `foundation:MetaUserTask` | A UI form or wizard step awaiting user input |
| `foundation:MetaIntermediateEvent` | A Tauri event (`entity-created`, `ai-status`, etc.) |
| `foundation:MetaStartEvent` | The trigger that instantiates a Process |
| `foundation:MetaEndEvent` | The terminal state of a Process instance |
| `foundation:MetaExclusiveGateway` | A conditional branch — exactly one path taken |
| `foundation:MetaInclusiveGateway` | A conditional branch — one or more paths taken |
| `foundation:MetaParallelGateway` | A `tokio::spawn` or concurrent execution split |
| `foundation:MetaEventBasedGateway` | A listener waiting for the first matching event |
| `foundation:MetaGatewayCondition` | A named branch condition with operator and value |
| `foundation:MetaBoundaryEvent` | A `catch` block or timeout handler on a task (event type only) |
| `foundation:MetaBoundaryCondition` | The routing logic for a boundary catch (condition + target) |
| `foundation:MetaShould` | The behavioral contract of a task (outcome + benefit) |
| `foundation:MetaEventTrigger` | The kind of real-world trigger initiating a start/intermediate event |
| `foundation:inputConcept` | The OWL class a node consumes as input |
| `foundation:outputConcept` | The OWL class a node produces as output |
| `foundation:invokesProcess` | The `foundation:MetaProcess` a sub-process node delegates to |
