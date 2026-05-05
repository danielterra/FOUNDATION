---
name: widget-remove
description: Use when permanently removing a Foundation widget — retracts the WidgetType individual, deletes the Svelte component file, and removes the WidgetManager dispatch branch.
disable-model-invocation: false
---

# Widget Remove

## Steps
1. Find the WidgetType individual: `search(class_iri: "foundation:WidgetType", filters: [{detail: "rdfs:label", value: "<name>"}])`. Capture the IRI and the `widget_type` ID.
2. Retract the WidgetType: `retract_individual({ iri: "<iri>" })`.
3. Delete `src/lib/components/widgets/<PascalCaseName>Widget.svelte`.
4. Edit `src/lib/components/widgets/WidgetManager.svelte`:
   - Remove the `import` line for the widget.
   - Remove the `{:else if widget.widget_type === '<type_id>'}` branch.
5. Run `cargo check --manifest-path src-tauri/Cargo.toml` to confirm no regressions.

## Rules
- ALWAYS confirm with user before removing — destructive across ontology + filesystem + dispatch.
- NEVER skip the WidgetManager edit — leaving a dead import or branch breaks the build or leaves an unreachable widget type.
- Existing widget instances (`foundation:Widget_*` individuals) are NOT auto-cleaned. Mention this to the user; they may want to retract them via `/ontology-remove`.
- ALWAYS resolve the WidgetType IRI via `search(...)` first. NEVER hardcode IRIs.

## When NOT to use this skill
- Updating widget metadata or component → `/widget-change`
- Creating a new widget → `/widget-create`
