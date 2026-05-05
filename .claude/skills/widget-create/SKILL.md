---
name: widget-create
description: Creates a new Foundation blackboard widget — registers it in the ontology, scaffolds the Svelte component, and wires it into WidgetManager.
disable-model-invocation: false
---

# Create Widget

## WidgetManager component dispatch block
!`grep -n "widget_type\|import.*Widget" src/lib/components/widgets/WidgetManager.svelte | head -40`

---

## Instructions

You are creating a new blackboard widget for Foundation. A widget is a live panel on the visual canvas, bound to one ontology entity, that reads its data from the triple store.

Start by calling `search(class_iri: "foundation:WidgetType")` to see all existing widget types and avoid duplicates.

If the user hasn't specified all details, ask for:
- **Widget name** — display name (e.g. "Invoice Viewer"); the type ID will be inferred as snake_case
- **Supported class** — IRI of the entity class this widget targets, or `owl:Thing` for universal
- **Purpose** — what the widget does and when the AI should use it

---

### Step 1 — Register in the ontology

Use `assert_individual` to create a `foundation:WidgetType` individual:

```
assert_individual(operations: [{
  class_iri: "foundation:WidgetType",
  label: "<Widget Name>",
  comment: "<one-line description — shown as tooltip in Inspector header>",
  icon: "<material-symbols-name>",
  properties: [
    { property_iri: "foundation:hasStatus",          values: ["foundation:Status_1772755611667"] },
    { property_iri: "foundation:widgetTypeId",        values: ["<type_id>"] },
    { property_iri: "foundation:widgetSupportedClass", values: ["<class_iri or owl:Thing>"] },
    { property_iri: "foundation:widgetDefaultWidth",  values: ["480"] },
    { property_iri: "foundation:widgetDefaultHeight", values: ["600"] }
  ]
}])
```

Always add a `widgetUsageNote` explaining when and why the AI should use this widget, followed by how to display it:
```
{ property_iri: "foundation:widgetUsageNote", values: ["<WHY: when to use this widget — then HOW: pass the entity IRI in speak.iris>"] }
```

If the widget requires a dedicated entity to be created first (e.g. a `foundation:MermaidDiagram` individual), the HOW section must describe that creation workflow before the `speak.iris` step.

Confirm the IRI returned before proceeding.

---

### Step 2 — Scaffold the Svelte component

Create `src/lib/components/widgets/<PascalCaseName>Widget.svelte`.

Use this minimal structure:

```svelte
<script>
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';

  let { widgetId, entityId = '' } = $props();

  let entityData = $state(null);
  let loading = $state(true);
  let error = $state(null);

  async function loadData() {
    if (!entityId) return;
    try {
      const result = await invoke('entity__get', { entityId });
      entityData = JSON.parse(result);
    } catch (err) {
      error = err?.toString() ?? 'Failed to load';
    } finally {
      loading = false;
    }
  }

  async function closeWidget() {
    await invoke('widget_blackboard__remove_widget', { widgetId });
  }

  let unlistenEntityUpdated;

  onMount(async () => {
    await loadData();
    unlistenEntityUpdated = await listen('entity-updated', async (event) => {
      if (event.payload.entityId === entityId) await loadData();
    });
  });

  onDestroy(() => {
    unlistenEntityUpdated?.();
  });
</script>

<div class="widget">
  <!-- widget-header is required — WidgetManager attaches drag listeners to it -->
  <div class="widget-header">
    <span class="material-symbols-outlined"><!-- icon --></span>
    <span class="title">{entityData?.label ?? entityId}</span>
    <button class="close-btn" onclick={closeWidget}>
      <span class="material-symbols-outlined">close</span>
    </button>
  </div>

  <div class="widget-content">
    {#if loading}
      <div class="loading">
        <span class="material-symbols-outlined spinning">progress_activity</span>
      </div>
    {:else if error}
      <div class="error">{error}</div>
    {:else}
      <!-- widget body -->
    {/if}
  </div>
</div>
```

Two contracts must be respected:
- A child with class `widget-header` must exist — `WidgetManager` attaches drag listeners to it.
- Call `invoke('widget_blackboard__remove_widget', { widgetId })` to close.

Adapt the body for the widget's purpose using `entityData.properties` for entity data.

---

### Step 3 — Register in WidgetManager

Edit `src/lib/components/widgets/WidgetManager.svelte`:

1. Add the import at the top of `<script>`:
```svelte
import <PascalCaseName>Widget from './<PascalCaseName>Widget.svelte';
```

2. Add a branch inside the `{#each widgets}` dispatch block, after the last `{:else if}`:
```svelte
{:else if widget.widget_type === '<type_id>'}
  <<PascalCaseName>Widget widgetId={widget.id} entityId={widget.entity_id} />
```

---

### Step 4 — Verify

Run `cargo check` to confirm no Rust regressions (the ontology registration requires no Rust changes):

```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

Confirm with the user that the widget appears correctly in the Inspector header when an entity of the supported class is open.

---

## Rules

- **Widget IDs are deterministic** — `foundation:Widget_{type_id}_{entity_id}`. Adding a widget that already exists is a no-op.
- **Never hardcode widget type IDs in Rust** — the registry lives entirely in the ontology.
- **`widgetUsageNote` must lead with WHY** — purpose first, creation steps second. This text is shown directly to the AI in the system prompt.
- **`widget-header` class is mandatory** — without it, drag doesn't work.
- **Always read WidgetManager.svelte before editing** — the dispatch block structure matters.
