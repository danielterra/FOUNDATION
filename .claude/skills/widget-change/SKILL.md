---
name: widget-change
description: Use when modifying an existing Foundation widget — updating its WidgetType ontology metadata (icon, default size, supported class, usage note) or its Svelte component logic. NOT for creating new widgets (use widget-create) or removing them (use widget-remove).
disable-model-invocation: false
---

# Widget Change

## Common change targets
- WidgetType metadata (icon, default size, supported class, usage note) → ontology change.
- Component visual/behavior → edit `src/lib/components/widgets/<PascalCaseName>Widget.svelte`.
- `widget_type` ID rename → ontology + dispatch update (rare; avoid).

## Steps for ontology metadata change
1. Find the WidgetType individual: `search(class_iri: "foundation:WidgetType", filters: [{detail: "rdfs:label", value: "<name>"}])`.
2. Use `replace_property_values` on the IRI to update fields like:
   - `foundation:hasIcon`
   - `foundation:widgetDefaultWidth` / `foundation:widgetDefaultHeight`
   - `foundation:widgetSupportedClass`
   - `foundation:widgetUsageNote`
3. Changes take effect immediately — no recompile needed.

## Steps for component change
- Edit the existing `.svelte` file in `src/lib/components/widgets/`. Two contracts must remain intact:
  - `widget-header` class child must exist (drag listener target).
  - Close button must call `invoke('widget_blackboard__remove_widget', { widgetId })`.
- ALWAYS run `cargo check --manifest-path src-tauri/Cargo.toml` after if anything touched Tauri commands.

## Rules
- NEVER rename `widget_type` ID without also updating the dispatch branch in `WidgetManager.svelte`.
- NEVER edit `WidgetManager.svelte` for property changes — only for type ID changes or new widgets.
- ALWAYS keep `widgetUsageNote` leading with WHY (purpose) before HOW (creation/usage).
- ALWAYS resolve the WidgetType IRI via `search(...)` first. NEVER hardcode IRIs.

## When NOT to use this skill
- Creating a new widget → `/widget-create`
- Removing a widget → `/widget-remove`
