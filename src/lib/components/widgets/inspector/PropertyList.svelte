<script>
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import PropertyDetailItem from './PropertyDetailItem.svelte';
  import PropertyHint from './PropertyHint.svelte';
  import { sticky } from '$lib/utils/actions';
  import { onMount } from 'svelte';

  let {
    properties, requiredFields = [], isClass = false,
    openEntityInspector, onSave, onSaveReference,
    onRemoveProperty = null,
    onSaveCardinality = null,
  } = $props();

  let now = $state(Date.now());
  let tickTimer;

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

  let hintVisible = $state(false);
  let hintX = $state(0);
  let hintY = $state(0);
  let hintDesc = $state('');
  let hintSourceLabel = $state('');
  let hintSourceIcon = $state('');

  function showHint(e, desc, sourceLabel, sourceIcon) {
    const rect = e.currentTarget.getBoundingClientRect();
    hintDesc = desc ?? '';
    hintSourceLabel = sourceLabel ?? '';
    hintSourceIcon = sourceIcon ?? '';
    hintX = rect.left + rect.width / 2;
    hintY = rect.top - 8;
    hintVisible = true;
  }

  function hideHint() {
    hintVisible = false;
  }

  let optionalCollapsed = $state(true);

  const groupedDetails = $derived(
    (properties ?? []).reduce((acc, prop) => {
      if (!acc[prop.property]) {
        acc[prop.property] = {
          property: prop.property,
          propertyLabel: prop.propertyLabel,
          propertyComment: prop.propertyComment,
          isObjectProperty: prop.isObjectProperty,
          sourceClassLabel: prop.sourceClassLabel,
          sourceClassIcon: prop.sourceClassIcon,
          datatype: prop.datatype,
          unit: prop.unit ?? null,
          rangeClassIri: prop.rangeClassIri,
          rangeClassLabel: prop.rangeClassLabel,
          rangeClassIcon: prop.rangeClassIcon,
          isCalculated: prop.isCalculated ?? false,
          isEmpty: prop.isEmpty ?? false,
          minCount: prop.minCount ?? null,
          maxCount: prop.maxCount ?? null,
          values: []
        };
      }
      if (!prop.isEmpty) {
        acc[prop.property].values.push({
          value: prop.value,
          valueLabel: prop.valueLabel,
          valueIcon: prop.valueIcon,
          unitLabel: prop.unitLabel,
          datatype: prop.datatype,
          valueStatus: prop.valueStatus,
          formulaError: prop.formulaError ?? null,
          fileInfo: prop.fileInfo ?? null
        });
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
      const required = all.filter(g => requiredFields.includes(g.property));
      const optional = all.filter(g => !requiredFields.includes(g.property));
      return {
        mode: 'class',
        required: groupBySource(required),
        optional: groupBySource(optional),
        requiredCount: required.length,
        optionalCount: optional.length,
      };
    } else {
      const filled = all.filter(g => !g.isEmpty);
      const empty = all.filter(g => g.isEmpty);
      const allItems = [...filled, ...empty];
      return {
        mode: 'instance',
        all: [{ sourceClassLabel: null, items: allItems }],
        allCount: allItems.length,
      };
    }
  });
</script>

<PropertyHint
  visible={hintVisible}
  x={hintX}
  y={hintY}
  desc={hintDesc}
  sourceLabel={hintSourceLabel}
  sourceIcon={hintSourceIcon}
/>

{#snippet sourceGroups(groups, sepTop = 0)}
  {#each groups as sourceGroup}
    {#if sourceGroup.sourceClassLabel}
      <div class="source-separator" use:sticky={{ top: sepTop }}>{sourceGroup.sourceClassLabel}</div>
    {/if}
    {#each sourceGroup.items as detailGroup (detailGroup.property)}
      <PropertyDetailItem
        {detailGroup}
        {isClass}
        {now}
        {openEntityInspector}
        {onSave}
        {onSaveReference}
        {onRemoveProperty}
        {onSaveCardinality}
        onShowHint={showHint}
        onHideHint={hideHint}
      />
    {/each}
  {/each}
{/snippet}

{#if properties?.length > 0}
  <div class="details-list">

    {#if sections.mode === 'class'}

      {#if sections.requiredCount > 0}
        <div class="section">
          <div class="section-header" use:sticky={{ top: 0 }}>
            <span class="section-title">Required</span>
            <span class="section-count">{sections.requiredCount}</span>
          </div>
          <div class="section-body">
            {@render sourceGroups(sections.required, 28)}
          </div>
        </div>
      {/if}

      {#if sections.optionalCount > 0}
        <div class="section">
          <button
            class="section-header collapsible"
            use:sticky={{ top: 0 }}
            onclick={() => optionalCollapsed = !optionalCollapsed}
          >
            <span class="material-symbols-outlined chevron" class:expanded={!optionalCollapsed}>
              chevron_right
            </span>
            <span class="section-title">Optional</span>
            <span class="section-count">{sections.optionalCount}</span>
          </button>
          {#if !optionalCollapsed}
            <div class="section-body" transition:slide={{ duration: 300, easing: cubicOut }}>
              {@render sourceGroups(sections.optional, 28)}
            </div>
          {/if}
        </div>
      {/if}

    {:else}

      {#if sections.allCount > 0}
        <div class="section-body">
          {@render sourceGroups(sections.all, 0)}
        </div>
      {/if}

    {/if}

  </div>
{/if}

<style>
  .details-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 16px;
  }

  .section {
    display: flex;
    flex-direction: column;
  }

  .section-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 2px;
    margin-bottom: 6px;
    z-index: 2;
    background: color-mix(in srgb, var(--color-black) 97%, transparent);
    backdrop-filter: blur(12px);
  }

  .section-header.collapsible {
    background: color-mix(in srgb, var(--color-black) 97%, transparent);
    border: none;
    cursor: pointer;
    width: 100%;
    text-align: left;
    border-radius: 4px;
    transition: background 0.15s;
  }

  .section-header.collapsible:hover {
    background: color-mix(in srgb, var(--color-black) 92%, var(--color-white) 4%);
  }

  .chevron {
    font-size: 16px;
    color: var(--color-neutral);
    opacity: 0.6;
    transition: transform 0.2s;
    flex-shrink: 0;
  }

  .chevron.expanded {
    transform: rotate(90deg);
  }

  .section-title {
    font-family: var(--font-body);
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: color-mix(in srgb, var(--color-neutral) 60%, transparent);
    flex: 1;
  }

  .section-count {
    font-size: 10px;
    font-weight: 600;
    color: color-mix(in srgb, var(--color-neutral) 70%, transparent);
    padding: 1px 6px;
    background: color-mix(in srgb, var(--color-neutral) 15%, transparent);
    border-radius: 10px;
  }

  .section-body {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .source-separator {
    font-family: var(--font-body);
    font-size: 10px;
    font-weight: 600;
    color: color-mix(in srgb, var(--color-neutral) 40%, transparent);
    padding: 6px 2px 2px;
    border-top: 1px solid color-mix(in srgb, var(--color-neutral) 12%, transparent);
    margin-top: 4px;
    z-index: 1;
    background: color-mix(in srgb, var(--color-black) 97%, transparent);
    backdrop-filter: blur(12px);
  }

  .source-separator:first-child {
    margin-top: 0;
    border-top: none;
    padding-top: 2px;
  }
</style>
