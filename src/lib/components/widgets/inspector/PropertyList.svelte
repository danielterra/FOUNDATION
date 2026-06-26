<script>
  import PropertyDetailItem from './PropertyDetailItem.svelte';
  import BigNumberCard from './BigNumberCard.svelte';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { sticky } from '$lib/utils/actions';
  import { onMount } from 'svelte';

  let {
    properties, requiredFields = [], isClass = false,
    entityId = null,
    openEntityInspector, onSave, onSaveReference,
    onClearProperty = null,
    onRemoveProperty = null,
    onSaveCardinality = null,
    onSaveQueryConfig = null,
    onLoadMoreBacklinks = null,
    onLoadMoreProperty = null,
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
          unit: prop.unit ?? null,
          rangeClassIri: prop.rangeClassIri,
          rangeClassLabel: prop.rangeClassLabel,
          rangeClassIcon: prop.rangeClassIcon,
          isCalculated: prop.isCalculated ?? false,
          isQueryProperty: prop.isQueryProperty ?? false,
          isPropertyPicker: prop.isPropertyPicker ?? false,
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
          unitLabel: prop.unitLabel,
          datatype: prop.datatype,
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
      return {
        mode: 'class',
        all: groupBySource(all),
        allCount: all.length,
      };
    } else {
      const calculated = all.filter(g => g.isCalculated);
      const nonCalculated = all.filter(g => !g.isCalculated);
      const calculatedNumeric = calculated.filter(g => !g.isEmpty && isNumericType(g.datatype));
      const calculatedOther = calculated.filter(g => g.isEmpty || !isNumericType(g.datatype));
      const filled = nonCalculated.filter(g => !g.isEmpty);
      const empty = nonCalculated.filter(g => g.isEmpty);
      const regularItems = [...filled, ...empty];
      return {
        mode: 'instance',
        calculatedNumeric,
        calculatedOther,
        calculatedCount: calculated.length,
        all: [{ sourceClassLabel: null, items: regularItems }],
        allCount: regularItems.length,
      };
    }
  });
</script>

{#snippet sourceGroups(groups, sepTop = 0)}
  {#each groups as sourceGroup}
    {#if sourceGroup.sourceClassLabel}
      <div class="source-separator" use:sticky={{ top: sepTop }}>{sourceGroup.sourceClassLabel}</div>
    {/if}
    {#each sourceGroup.items as detailGroup (detailGroup._groupKey)}
      <PropertyDetailItem
        {detailGroup}
        {isClass}
        {now}
        {entityId}
        {openEntityInspector}
        {onSave}
        {onSaveReference}
        {onClearProperty}
        {onRemoveProperty}
        {onSaveCardinality}
        {onSaveQueryConfig}
        {onLoadMoreBacklinks}
        {onLoadMoreProperty}
      />
    {/each}
  {/each}
{/snippet}

<Tooltip.Provider delayDuration={200}>
  {#if properties?.length > 0}
    <div class="details-list">

      {#if sections.mode === 'class'}

        {#if sections.allCount > 0}
          <div class="section-body">
            {@render sourceGroups(sections.all)}
          </div>
        {/if}

      {:else}

        {#if sections.calculatedCount > 0}
          <div class="section">
            <div class="section-header" use:sticky={{ top: 0 }}>
              <span class="material-symbols-outlined section-calc-icon">calculate</span>
              <span class="section-divider">Calculado</span>
              <span class="section-count">{sections.calculatedCount}</span>
            </div>
            <div class="section-body">
              {#if sections.calculatedNumeric.length > 0}
                <div class="big-numbers-grid">
                  {#each sections.calculatedNumeric as detailGroup (detailGroup._groupKey)}
                    <BigNumberCard {detailGroup} />
                  {/each}
                </div>
              {/if}
              {#each sections.calculatedOther as detailGroup (detailGroup._groupKey)}
                <PropertyDetailItem
                  {detailGroup}
                  {isClass}
                  {now}
                  {entityId}
                  {openEntityInspector}
                  {onSave}
                  {onSaveReference}
                  {onRemoveProperty}
                  {onSaveCardinality}
                  {onSaveQueryConfig}
                />
              {/each}
            </div>
          </div>
        {/if}

        {#if sections.allCount > 0}
          <div class="section-body">
            {@render sourceGroups(sections.all, 0)}
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
    background: var(--color-surface-1);
  }

  .section-divider {
    flex: 1;
    font-family: var(--font-body);
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: color-mix(in srgb, var(--color-neutral) 60%, transparent);
  }

  .section-count {
    font-size: 10px;
    font-weight: 600;
    color: color-mix(in srgb, var(--color-neutral) 70%, transparent);
    padding: 1px 6px;
    background: color-mix(in srgb, var(--color-neutral) 15%, transparent);
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
    margin-top: 4px;
    z-index: 1;
    background: var(--color-surface-1);
  }

  .source-separator:first-child {
    margin-top: 0;
    padding-top: 2px;
  }

  .section-calc-icon {
    font-size: 14px;
    color: var(--color-accent);
    opacity: 0.8;
    flex-shrink: 0;
  }

  .big-numbers-grid {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
</style>
