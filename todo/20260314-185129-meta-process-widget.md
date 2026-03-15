# MetaProcess Widget — Implementation Plan

Render a `foundation:MetaProcess` as an interactive flow diagram on the blackboard.

**Stack:** `@xyflow/svelte` (rendering) + `@dagrejs/dagre` (auto-layout) + Rust backend (BFS graph traversal)

---

## Overview

The widget is bound to a `foundation:MetaProcess` entity. The backend traverses its node graph
(via `metaStartNode` → `nextNode` BFS with cycle detection) and returns a `{ nodes, edges }` JSON
payload. The frontend computes layout with dagre and renders with xyflow using custom node
components per concept type.

---

## Step 1 — Install frontend dependencies

```bash
npm install @xyflow/svelte @dagrejs/dagre
```

---

## Step 2 — Rust: backend graph traversal command

**File:** `src-tauri/src/commands/meta_process.rs`

Create command `meta_process__get_graph(process_iri: String) -> Result<String, String>`

### Logic

1. Load the `MetaProcess` entity, read `foundation:metaStartNode` IRI
2. BFS from `metaStartNode`, following `foundation:nextNode` (multi-valued — use `get_all_iri_properties`)
3. Track visited IRIs to handle cycles (e.g. Home loops back to itself)
4. For each visited node, read:
   - `rdf:type` → concept type (used by frontend to pick node component)
   - `rdfs:label` → display label
   - `foundation:invokesProcess` → (MetaSubProcess only) IRI of child MetaProcess
5. Collect edges: for each node, each `nextNode` value = one edge
6. Return JSON:

```json
{
  "processLabel": "AppStart",
  "nodes": [
    { "id": "foundation:MetaStartEvent_123", "type": "MetaStartEvent", "label": "App Start" },
    { "id": "foundation:MetaSystemTask_456", "type": "MetaSystemTask", "label": "Initialize System" },
    { "id": "foundation:MetaSubProcess_789", "type": "MetaSubProcess", "label": "Home", "invokesProcess": "foundation:MetaProcess_111" }
  ],
  "edges": [
    { "id": "e1", "source": "foundation:MetaStartEvent_123", "target": "foundation:MetaParallelGateway_xxx" },
    { "id": "e2", "source": "foundation:MetaParallelGateway_xxx", "target": "foundation:MetaSystemTask_456", "label": "" }
  ]
}
```

### Register in lib.rs

Add `meta_process__get_graph` to the `invoke_handler` in `src-tauri/src/lib.rs`.

---

## Step 3 — Rust: register widget type

**File:** `src-tauri/src/commands/widget.rs`

Add to `blackboard__list_widget_types()`:

```rust
WidgetType {
    id: "meta_process".to_string(),
    name: "MetaProcess".to_string(),
    description: "Interactive flow diagram of a MetaProcess".to_string(),
    supports_entity: true,
},
```

---

## Step 4 — Frontend: custom node components

**Directory:** `src/lib/components/widgets/meta-process/`

Create one Svelte component per concept type. Each receives `{ data: { label } }` from xyflow.

| File | Concept | Color |
|------|---------|-------|
| `NodeStartEvent.svelte` | MetaStartEvent / MetaEndEvent | `#43A047` green |
| `NodeSystemTask.svelte` | MetaSystemTask | `#1E88E5` blue |
| `NodeUserTask.svelte` | MetaUserTask | `#FB8C00` orange |
| `NodeSubProcess.svelte` | MetaSubProcess | `#00ACC1` cyan |
| `NodeGatewayExclusive.svelte` | MetaExclusiveGateway | `#8E24AA` purple, diamond shape |
| `NodeGatewayParallel.svelte` | MetaParallelGateway | `#8E24AA` purple, hexagon shape |
| `NodeIntermediateEvent.svelte` | MetaIntermediateEvent | `#FDD835` yellow, dashed border |

Each node shows the concept type in small text above the label (matching the diagram style).

Clicking a node dispatches an event to open an Inspector widget for that node's IRI.
Clicking a MetaSubProcess node also offers "Open process" to open its child MetaProcess widget.

---

## Step 5 — Frontend: dagre layout utility

**File:** `src/lib/components/widgets/meta-process/layout.js`

```js
import dagre from '@dagrejs/dagre'

export function applyDagreLayout(nodes, edges, direction = 'LR') {
  const g = new dagre.graphlib.Graph()
  g.setGraph({ rankdir: direction, ranksep: 80, nodesep: 40 })
  g.setDefaultEdgeLabel(() => ({}))

  for (const node of nodes) {
    g.setNode(node.id, { width: node.width ?? 160, height: node.height ?? 60 })
  }
  for (const edge of edges) {
    g.setEdge(edge.source, edge.target)
  }

  dagre.layout(g)

  return nodes.map(node => {
    const { x, y } = g.node(node.id)
    return { ...node, position: { x: x - (node.width ?? 160) / 2, y: y - (node.height ?? 60) / 2 } }
  })
}
```

---

## Step 6 — Frontend: MetaProcessWidget.svelte

**File:** `src/lib/components/widgets/MetaProcessWidget.svelte`

### Responsibilities

1. On mount: `invoke('meta_process__get_graph', { processIri: entityId })`
2. Map backend nodes to xyflow nodes, assigning the correct custom component per `type`
3. Map backend edges to xyflow edges (with labels where present)
4. Call `applyDagreLayout(nodes, edges)` to compute positions
5. Render `<SvelteFlow>` with `nodeTypes` map and computed nodes/edges
6. Listen to `entity-updated` events for the process IRI → reload graph
7. On node click → `invoke('widget_blackboard__add_widget', { widgetType: 'inspector', entityId: nodeId })`
8. On MetaSubProcess click → offer option to open child process in a new MetaProcess widget

### Node type mapping

```js
const nodeTypes = {
  MetaStartEvent:          NodeStartEvent,
  MetaEndEvent:            NodeStartEvent,   // same component, different label
  MetaSystemTask:          NodeSystemTask,
  MetaUserTask:            NodeUserTask,
  MetaSubProcess:          NodeSubProcess,
  MetaExclusiveGateway:    NodeGatewayExclusive,
  MetaParallelGateway:     NodeGatewayParallel,
  MetaIntermediateEvent:   NodeIntermediateEvent,
}
```

---

## Step 7 — Register in WidgetManager

**File:** `src/lib/components/widgets/WidgetManager.svelte`

```svelte
import MetaProcessWidget from './MetaProcessWidget.svelte';

{:else if widget.widget_type === 'meta_process'}
  <MetaProcessWidget widgetId={widget.id} entityId={widget.entity_id} />
```

---

## Step 8 — Register WidgetDefinition in ontology

Use `learn_things` MCP tool to create a `foundation:WidgetDefinition`:

```json
{
  "concept_iri": "foundation:WidgetDefinition",
  "label": "MetaProcess",
  "properties": [
    { "detail_iri": "foundation:widgetDefId",               "values": ["meta_process"] },
    { "detail_iri": "foundation:widgetDefDescription",      "values": ["Interactive flow diagram of a MetaProcess"] },
    { "detail_iri": "foundation:widgetDefSupportsEntity",   "values": ["true"] },
    { "detail_iri": "foundation:widgetDefSupportedConcepts","values": ["foundation:MetaProcess"] }
  ]
}
```

---

## Step 9 — Validate

- `cargo check` passes
- Open `foundation:MetaProcess_1773524501934` (AppStart) in the widget
- Verify all 25 nodes render with correct colors and labels
- Verify dagre LR layout matches the hand-drawn diagram
- Verify clicking a node opens its Inspector
- Verify clicking a MetaSubProcess offers "Open process"
- Verify `entity-updated` triggers a reload

---

## Node type → concept IRI mapping (reference)

| type string | foundation IRI |
|---|---|
| `MetaStartEvent` | `foundation:MetaStartEvent` |
| `MetaEndEvent` | `foundation:MetaEndEvent` |
| `MetaIntermediateEvent` | `foundation:MetaIntermediateEvent` |
| `MetaSystemTask` | `foundation:MetaSystemTask` |
| `MetaUserTask` | `foundation:MetaUserTask` |
| `MetaSubProcess` | `foundation:MetaSubProcess` |
| `MetaExclusiveGateway` | `foundation:MetaExclusiveGateway` |
| `MetaParallelGateway` | `foundation:MetaParallelGateway` |
