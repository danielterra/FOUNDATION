<script>
  import PropertyDetailItem from './PropertyDetailItem.svelte';
  import OntologyEntityIdentity from './OntologyEntityIdentity.svelte';
  import BigNumberCard from './BigNumberCard.svelte';
  import { Button } from '$lib/components/ui/button';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { onMount } from 'svelte';

  let {
    properties, requiredFields = [], isClass = false,
    classLabel = null, classIcon = null,
    entityId = null,
    openEntityInspector, onSave, onSaveReference,
    editMode = false, drafts = {}, fieldErrors = {}, onDraftChange = null,
    onClearProperty = null,
    onRemoveProperty = null,
    onSaveCardinality = null,
    onSaveQueryConfig = null,
    onLoadMoreBacklinks = null,
    onLoadMoreProperty = null,
    propertyOrder = [], propertyOrderDraft = [], orderingProperties = false,
    canOrganizeProperties = false, onBeginPropertyOrdering = null,
    onMovePropertyOrder = null, onDropPropertyOrder = null,
  } = $props();

  let now = $state(Date.now());
  let tickTimer;
  let draggedPropertyIri = $state(null);
  let insertionIndex = $state(null);

  function cancelPropertyDrag() {
    draggedPropertyIri = null;
    insertionIndex = null;
  }

  function getInsertionIndex(event) {
    const list = event.currentTarget.closest('.property-order-list');
    const remaining = propertyOrderDraft.filter(iri => iri !== draggedPropertyIri);
    if (!list || remaining.length === 0) return 0;

    const rows = [...list.querySelectorAll('[data-property-order-iri]')]
      .filter(row => row.dataset.propertyOrderIri !== draggedPropertyIri);
    for (const row of rows) {
      const rect = row.getBoundingClientRect();
      if (event.clientY < rect.top + rect.height / 2) {
        return remaining.indexOf(row.dataset.propertyOrderIri);
      }
    }
    return remaining.length;
  }

  function beginPropertyDrag(event, propertyIri) {
    if (event.button !== 0 || event.target.closest('button:not(.property-order-grip-button)')) return;
    event.preventDefault();
    event.stopPropagation();
    draggedPropertyIri = propertyIri;
    insertionIndex = propertyOrderDraft.indexOf(propertyIri);
    event.currentTarget.setPointerCapture(event.pointerId);
  }

  function updatePropertyDrag(event) {
    if (!draggedPropertyIri) return;
    const list = event.currentTarget.closest('.property-order-list');
    const pointerTarget = document.elementFromPoint(event.clientX, event.clientY);
    if (!list?.contains(pointerTarget)) {
      insertionIndex = null;
      return;
    }
    insertionIndex = getInsertionIndex(event);
  }

  function finishPropertyDrag(event) {
    if (!draggedPropertyIri) return;
    const list = event.currentTarget.closest('.property-order-list');
    const pointerTarget = document.elementFromPoint(event.clientX, event.clientY);
    if (!list?.contains(pointerTarget)) {
      cancelPropertyDrag();
      return;
    }
    const propertyIri = draggedPropertyIri;
    const targetIndex = getInsertionIndex(event);
    cancelPropertyDrag();
    if (propertyIri && targetIndex !== propertyOrderDraft.indexOf(propertyIri)) {
      onDropPropertyOrder?.({ detail: { propertyIri, targetIndex } });
    }
  }

  function computeTickInterval() {
    const MS_MINUTE = 60_000;
    const MS_HOUR = 3_600_000;
    const MS_DAY = 86_400_000;
    let interval = MS_DAY;
    for (const prop of properties) {
      if (prop.datatype !== 'xsd:dateTime' || !prop.value) continue;
      const ts = isNaN(Number(prop.value))
        ? new Date(prop.value).getTime()
        : Number(prop.value);
      if (isNaN(ts)) continue;
      const diff = Math.abs(now - ts);
      if (diff < MS_MINUTE) return 1_000;
      if (diff < MS_HOUR) interval = Math.min(interval, MS_MINUTE);
      else if (diff < MS_DAY) interval = Math.min(interval, MS_HOUR);
    }
    return interval;
  }

  function scheduleTick() {
    tickTimer = setTimeout(() => {
      now = Date.now();
      scheduleTick();
    }, computeTickInterval());
  }

  onMount(() => {
    scheduleTick();
    return () => clearTimeout(tickTimer);
  });

  function isNumericType(datatype) {
    return datatype === 'xsd:decimal' || datatype === 'xsd:integer' ||
           datatype === 'xsd:int' || datatype === 'xsd:long' ||
           datatype === 'xsd:float' || datatype === 'xsd:double' ||
           datatype === 'xsd:nonNegativeInteger' || datatype === 'xsd:positiveInteger';
  }

  const groupedDetails = $derived(
    (properties ?? []).reduce((acc, prop) => {
      // Forward and backlink entries can share the same property IRI but mean different
      // things (X has Y as previousMonthBudget vs. Y has X as previousMonthBudget).
      // Discriminator: backlink = sourceClass != null. The backlinkNextCursor is eventual —
      // the backend attaches it only to the first item of the group (group_cursor.remove()),
      // so it cannot be part of the discriminator or the group key.
      const isBacklink = prop.sourceClass != null;
      const key = isBacklink ? `${prop.property}__backlink` : prop.property;
      if (!acc[key]) {
        acc[key] = {
          _groupKey: key,
          property: prop.property,
          propertyLabel: prop.propertyLabel,
          propertyComment: prop.propertyComment,
          isObjectProperty: prop.isObjectProperty,
          sourceClassLabel: prop.sourceClassLabel,
          sourceClassIcon: prop.sourceClassIcon,
          sourceClassIri: prop.sourceClass ?? null,
          datatype: prop.datatype,
          presentation: prop.presentation ?? null,
          language: prop.language ?? null,
          unit: prop.unit ?? null,
          rangeClassIri: prop.rangeClassIri,
          rangeClassLabel: prop.rangeClassLabel,
          rangeClassIcon: prop.rangeClassIcon,
          isCalculated: prop.isCalculated ?? false,
          isQueryProperty: prop.isQueryProperty ?? false,
          isPropertyPicker: prop.isPropertyPicker ?? false,
          dependsOnProperty: prop.dependsOnProperty ?? null,
          queryConfig: prop.queryConfig ?? null,
          isEmpty: prop.isEmpty ?? false,
          minCount: prop.minCount ?? null,
          maxCount: prop.maxCount ?? null,
          groupTotal: null,
          backlinkNextCursor: null,
          propertyNextCursor: null,
          values: []
        };
      }
      if (!prop.isEmpty) {
        acc[key].isEmpty = false;
        acc[key].values.push({
          value: prop.value,
          valueLabel: prop.valueLabel,
          valueIcon: prop.valueIcon,
          valueEntityKind: prop.valueEntityKind ?? null,
          unitLabel: prop.unitLabel,
          datatype: prop.datatype,
          presentation: prop.presentation ?? null,
          language: prop.language ?? null,
          valueStatus: prop.valueStatus,
          formulaError: prop.formulaError ?? null,
          fileInfo: prop.fileInfo ?? null
        });
        if (prop.groupTotal != null && acc[key].groupTotal == null) {
          acc[key].groupTotal = prop.groupTotal;
        }
        if (isBacklink) {
          if (acc[key].sourceClassIri == null && prop.sourceClass != null) {
            acc[key].sourceClassIri = prop.sourceClass;
          }
          if (acc[key].backlinkNextCursor == null && prop.backlinkNextCursor != null) {
            acc[key].backlinkNextCursor = prop.backlinkNextCursor;
          }
        } else if (acc[key].propertyNextCursor == null && prop.propertyNextCursor != null) {
          acc[key].propertyNextCursor = prop.propertyNextCursor;
        }
      }
      return acc;
    }, {})
  );

  const sections = $derived.by(() => {
    const all = Object.values(groupedDetails);

    function groupBySource(items) {
      const buckets = new Map();
      for (const item of items) {
        const key = item.sourceClassLabel ?? null;
        if (!buckets.has(key)) buckets.set(key, []);
        buckets.get(key).push(item);
      }
      const result = [];
      if (buckets.has(null)) result.push({ sourceClassLabel: null, items: buckets.get(null) });
      for (const [key, items] of buckets) {
        if (key !== null) result.push({ sourceClassLabel: key, items });
      }
      return result;
    }

    if (isClass) {
      const byProperty = new Map(all.map(item => [item.property, item]));
      const ordered = (propertyOrder ?? []).map(iri => byProperty.get(iri)).filter(Boolean);
      for (const item of all) {
        if (!ordered.includes(item)) ordered.push(item);
      }
      return {
        mode: 'class',
        all: groupBySource(all),
        ordered,
        allCount: all.length,
      };
    } else {
      const visible = all.filter(g => g.isCalculated
        ? !g.isEmpty
        : !g.isEmpty || (editMode && !g.isQueryProperty));

      // A dependent field exists only once its parent has a value, and it belongs
      // directly below that parent — not at the bottom with the other empty fields.
      // A dependent whose parent is unset (or whose parent was itself dropped, in a
      // chain) never enters the list at all.
      const byIri = new Map(visible.map(g => [g.property, g]));
      const dependentsOf = new Map();
      const independent = [];
      for (const group of visible) {
        const parentIri = group.dependsOnProperty;
        if (!parentIri) { independent.push(group); continue; }
        const parent = byIri.get(parentIri);
        if (!parent || parent.isEmpty) continue;
        if (!dependentsOf.has(parentIri)) dependentsOf.set(parentIri, []);
        dependentsOf.get(parentIri).push(group);
      }
      const ordered = [];
      for (const group of independent) {
        ordered.push(group, ...(dependentsOf.get(group.property) ?? []));
      }
      return {
        mode: 'instance',
        ordered,
        allCount: ordered.length,
      };
    }
  });
</script>

{#snippet propertyItem(detailGroup)}
  <PropertyDetailItem
    {detailGroup}
    {isClass}
    {now}
    {entityId}
    {openEntityInspector}
    {onSave}
    {editMode}
    {drafts}
    {fieldErrors}
    {onDraftChange}
    {onSaveReference}
    {onClearProperty}
    {onRemoveProperty}
    {onSaveCardinality}
    {onSaveQueryConfig}
    {onLoadMoreBacklinks}
    {onLoadMoreProperty}
  />
{/snippet}

<Tooltip.Provider delayDuration={200}>
  {#if properties?.length > 0}
    <div class="details-list">

      {#if sections.mode === 'class'}

        {#if canOrganizeProperties && !orderingProperties}
          <div class="property-order-heading">
            <Button variant="ghost" class="organize-action" onclick={onBeginPropertyOrdering}>
              <span class="material-symbols-outlined" aria-hidden="true">swap_vert</span>
              Organizar
            </Button>
          </div>
        {/if}

        {#if sections.allCount > 0}
          <div class="section-body">
            {#if orderingProperties}
              {@const sourceIndex = propertyOrderDraft.indexOf(draggedPropertyIri)}
              {@const insertionSlot = insertionIndex + (sourceIndex <= insertionIndex ? 1 : 0)}
              <ol class="property-order-list" aria-label="Ordem das propriedades da classe">
                {#each propertyOrderDraft as propertyIri, index (propertyIri)}
                  {#if draggedPropertyIri && insertionIndex !== null && insertionSlot === index}
                    <li class="property-order-insertion" aria-hidden="true"></li>
                  {/if}
                  {@const detailGroup = sections.ordered.find(item => item.property === propertyIri)}
                  {#if detailGroup}
                    <li
                      class="property-order-item"
                      data-property-order-iri={propertyIri}
                      class:property-order-dragging={draggedPropertyIri === propertyIri}
                    >
                      <span class="property-order-number" aria-label={`Posição ${index + 1}`}>{index + 1}</span>
                      <button
                        type="button"
                        class="property-order-grip-button"
                        aria-label={`Arrastar ${detailGroup.propertyLabel ?? detailGroup.property} para reordenar`}
                        title="Arraste para reordenar"
                        onpointerdown={(event) => beginPropertyDrag(event, propertyIri)}
                        onpointermove={updatePropertyDrag}
                        onpointerup={finishPropertyDrag}
                        onpointercancel={cancelPropertyDrag}
                        onlostpointercapture={cancelPropertyDrag}
                      >
                        <span class="material-symbols-outlined property-order-grip" aria-hidden="true">drag_indicator</span>
                      </button>
                      <div class="property-order-label">
                        <span>{detailGroup.propertyLabel ?? detailGroup.property}</span>
                        <OntologyEntityIdentity
                          kind="class"
                          label={detailGroup.sourceClassLabel ?? classLabel}
                          icon={detailGroup.sourceClassLabel ? detailGroup.sourceClassIcon : classIcon}
                          compact
                        />
                      </div>
                      <div class="property-order-controls">
                        <Button variant="ghost" class="order-control" aria-label={`Mover ${detailGroup.propertyLabel ?? detailGroup.property} para cima`} disabled={index === 0} onclick={() => onMovePropertyOrder?.({ detail: { propertyIri, targetIndex: index - 1 } })}>
                          <span class="material-symbols-outlined" aria-hidden="true">arrow_upward</span>
                        </Button>
                        <Button variant="ghost" class="order-control" aria-label={`Mover ${detailGroup.propertyLabel ?? detailGroup.property} para baixo`} disabled={index === propertyOrderDraft.length - 1} onclick={() => onMovePropertyOrder?.({ detail: { propertyIri, targetIndex: index + 1 } })}>
                          <span class="material-symbols-outlined" aria-hidden="true">arrow_downward</span>
                        </Button>
                      </div>
                    </li>
                  {/if}
                {/each}
                {#if draggedPropertyIri && insertionIndex !== null && insertionSlot === propertyOrderDraft.length}
                  <li class="property-order-insertion" aria-hidden="true"></li>
                {/if}
              </ol>
            {:else}
              {#each sections.ordered as detailGroup (detailGroup._groupKey)}
                {@render propertyItem(detailGroup)}
              {/each}
            {/if}
          </div>
        {/if}

      {:else}
        {#if sections.allCount > 0}
          <div class="section-body">
            {#each sections.ordered as detailGroup (detailGroup._groupKey)}
              {#if detailGroup.isCalculated && isNumericType(detailGroup.datatype)}
                <BigNumberCard {detailGroup} />
              {:else}
                {@render propertyItem(detailGroup)}
              {/if}
            {/each}
          </div>
        {/if}

      {/if}

    </div>
  {/if}
</Tooltip.Provider>

<style>
  .details-list {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-section);
    margin-bottom: var(--spacing-section);
  }

  .property-order-heading { display: flex; align-items: center; justify-content: space-between; gap: var(--spacing-field-group); margin-bottom: var(--spacing-field-group); color: var(--color-neutral); }
  :global(.organize-action) { color: var(--color-interactive); gap: var(--spacing-inline-tight); }
  :global(.organize-action .material-symbols-outlined) { font-size: var(--type-icon-small); }
  .property-order-list { display: flex; flex-direction: column; gap: var(--spacing-field-group); padding: 0; margin: 0; list-style: none; }
  .property-order-item { display: flex; align-items: center; gap: var(--spacing-control-internal); padding: var(--spacing-control-internal); background: var(--color-surface-2); }
  .property-order-number { min-width: 2ch; color: var(--color-neutral); text-align: center; font-family: var(--font-mono); }
  .property-order-grip-button { display: grid; place-items: center; flex: 0 0 auto; width: 2.5rem; height: 2.5rem; padding: 0; border: 0; background: transparent; color: var(--color-interactive); cursor: grab; touch-action: none; }
  .property-order-grip-button:active { cursor: grabbing; }
  .property-order-grip-button:focus-visible { outline: 2px solid var(--color-interactive); outline-offset: 2px; }
  .property-order-grip { color: inherit; pointer-events: none; }
  .property-order-dragging { opacity: 0.35; }
  .property-order-insertion { min-height: 3.5rem; border: 1px dashed var(--color-interactive); background: color-mix(in srgb, var(--color-interactive) 12%, transparent); }
  .property-order-label { display: flex; flex: 1; min-width: 0; flex-direction: column; gap: var(--spacing-inline-tight); overflow-wrap: anywhere; }
  .property-order-controls { display: flex; align-items: center; gap: var(--spacing-inline-tight); }
  :global(.order-control) { width: var(--size-touch-target); height: var(--size-touch-target); padding: 0; color: var(--color-interactive); }
  :global(.order-control .material-symbols-outlined) { font-size: var(--type-icon-small); }

  .section-body {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-inspector-property);
  }
</style>
