<script>
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import MarkdownValue from './MarkdownValue.svelte';
  import FileGrid from './FileGrid.svelte';
  import PropertyEditForm from './PropertyEditForm.svelte';
  import ReferenceSelect from './ReferenceSelect.svelte';
  import { sticky, focus } from '$lib/utils/actions';
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

  function bodyPortal(node) {
    document.body.appendChild(node);
    return {
      destroy() {
        node.remove();
      }
    };
  }

  let editingKey = $state(null);
  let draftValue = $state('');
  let editingDatatype = $state(null);
  let saving = $state(false);
  let editingRefKey = $state(null);
  let savingRef = $state(false);


  function editKey(propertyIri, valueIdx) {
    return `${propertyIri}::${valueIdx}`;
  }

  function startEdit(propertyIri, currentValue, valueIdx, datatype = null) {
    editingKey = editKey(propertyIri, valueIdx);
    editingDatatype = datatype;
    draftValue = toInputValue(currentValue ?? '', datatype);
  }

  function cancelEdit() {
    editingKey = null;
    draftValue = '';
    editingDatatype = null;
  }

  async function saveEdit(propertyIri) {
    if (!onSave || saving) return;
    saving = true;
    try {
      await onSave(propertyIri, fromInputValue(draftValue, editingDatatype), editingDatatype);
    } finally {
      saving = false;
      editingKey = null;
      draftValue = '';
      editingDatatype = null;
    }
  }

  function startRefEdit(propertyIri) {
    editingRefKey = propertyIri;
  }

  function cancelRefEdit() {
    editingRefKey = null;
  }

  async function saveRefEdit(propertyIri, iris) {
    if (!onSaveReference || savingRef) return;
    savingRef = true;
    try {
      await onSaveReference(propertyIri, iris);
    } finally {
      savingRef = false;
      editingRefKey = null;
    }
  }


  let editingCardinalityKey = $state(null);
  let cardinalityDraftMin = $state('');
  let cardinalityDraftMax = $state('');
  let savingCardinality = $state(false);

  function startCardinalityEdit(propertyIri, currentMin, currentMax) {
    editingCardinalityKey = propertyIri;
    cardinalityDraftMin = currentMin !== null && currentMin !== undefined ? String(currentMin) : '';
    cardinalityDraftMax = currentMax !== null && currentMax !== undefined ? String(currentMax) : '';
  }

  function cancelCardinalityEdit() {
    editingCardinalityKey = null;
    cardinalityDraftMin = '';
    cardinalityDraftMax = '';
  }

  async function saveCardinalityEdit(propertyIri) {
    if (!onSaveCardinality || savingCardinality) return;
    savingCardinality = true;
    try {
      const min = cardinalityDraftMin === '' ? null : parseInt(cardinalityDraftMin, 10);
      const max = cardinalityDraftMax === '' ? null : parseInt(cardinalityDraftMax, 10);
      await onSaveCardinality(propertyIri, min, max);
    } finally {
      savingCardinality = false;
      editingCardinalityKey = null;
      cardinalityDraftMin = '';
      cardinalityDraftMax = '';
    }
  }

  function formatCardinality(min, max) {
    const minStr = min !== null && min !== undefined ? String(min) : '0';
    const maxStr = max !== null && max !== undefined ? String(max) : '*';
    return `${minStr}..${maxStr}`;
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

  function isUrl(datatype) {
    return datatype === 'xsd:anyURI';
  }

  function isStringType(datatype) {
    return !datatype || datatype === 'xsd:string' || datatype === 'rdf:langString';
  }

  function isDateType(datatype) {
    return datatype === 'xsd:date' || datatype === 'xsd:dateTime';
  }

  function toInputValue(value, datatype) {
    if (!value) return '';
    if (datatype === 'xsd:date') return value;
    if (datatype === 'xsd:dateTime') {
      const d = new Date(isNaN(Number(value)) ? value : Number(value));
      if (isNaN(d.getTime())) return '';
      const pad = n => String(n).padStart(2, '0');
      return `${d.getFullYear()}-${pad(d.getMonth()+1)}-${pad(d.getDate())}` +
             `T${pad(d.getHours())}:${pad(d.getMinutes())}`;
    }
    return value;
  }

  function fromInputValue(value, datatype) {
    if (!value) return '';
    if (datatype === 'xsd:dateTime') {
      return new Date(value).toISOString();
    }
    return value;
  }

  async function openUrl_(url) {
    try {
      await openUrl(url);
    } catch (err) {
      console.error('Failed to open URL:', err);
    }
  }

  function formatDate(timestamp) {
    const currentNow = now; // read $state to make this reactive
    const ts = typeof timestamp === 'string' ? parseInt(timestamp) : timestamp;
    const date = new Date(ts);

    if (isNaN(date.getTime())) return timestamp;

    const nowDate = new Date(currentNow);
    const today = new Date(nowDate.getFullYear(), nowDate.getMonth(), nowDate.getDate());
    const yesterday = new Date(today);
    yesterday.setDate(yesterday.getDate() - 1);

    const dateDay = new Date(date.getFullYear(), date.getMonth(), date.getDate());
    const diffMs = currentNow - date.getTime();
    const diffMins = Math.floor(diffMs / 60000);
    const diffHours = Math.floor(diffMs / 3600000);
    const diffDays = Math.floor(diffMs / 86400000);

    if (diffMins < 1) return 'just now';
    if (diffMins < 60) return `${diffMins} ${diffMins === 1 ? 'minute' : 'minutes'} ago`;

    if (dateDay.getTime() === today.getTime()) {
      if (diffHours < 2) return '1 hour ago';
      return `${diffHours} hours ago`;
    }

    if (dateDay.getTime() === yesterday.getTime()) {
      const timeStr = date.toLocaleTimeString(
        'en-US', { hour: 'numeric', minute: '2-digit', hour12: true });
      return `Yesterday at ${timeStr}`;
    }

    if (diffDays < 7) {
      const dayName = date.toLocaleDateString('en-US', { weekday: 'long' });
      const timeStr = date.toLocaleTimeString(
        'en-US', { hour: 'numeric', minute: '2-digit', hour12: true });
      return `${dayName} at ${timeStr}`;
    }

    if (date.getFullYear() === nowDate.getFullYear()) {
      return date.toLocaleDateString('en-US', {
        month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit', hour12: true
      });
    }

    return date.toLocaleDateString('en-US', {
      year: 'numeric', month: 'short', day: 'numeric',
      hour: 'numeric', minute: '2-digit', hour12: true
    });
  }

  function formatDatatype(datatype) {
    if (!datatype || datatype === 'xsd:string' || datatype === 'rdf:langString') return 'String';
    const parts = datatype.split(':');
    const typeName = parts.length > 1 ? parts[1] : datatype;
    return typeName.charAt(0).toUpperCase() + typeName.slice(1);
  }

  function isIconUrl(icon) {
    if (!icon) return false;
    return icon.startsWith('http://') || icon.startsWith('https://') ||
           icon.startsWith('data:') || icon.startsWith('file://') || icon.startsWith('/');
  }

  function getIconUrl(icon) {
    if (!icon) return '';
    if (icon.startsWith('http://') || icon.startsWith('https://') ||
        icon.startsWith('data:')) return icon;
    if (icon.startsWith('file://')) return convertFileSrc(icon.replace(/^file:\/\//, ''));
    if (icon.startsWith('/')) return convertFileSrc(icon);
    return icon;
  }
</script>

{#snippet dateEditor(propertyIri, inputType)}
  <div class="date-edit-container">
    <input
      class="date-input"
      type={inputType}
      bind:value={draftValue}
      onkeydown={(e) => {
        if (e.key === 'Escape') cancelEdit();
        else if (e.key === 'Enter') saveEdit(propertyIri);
      }}
      use:focus
    />
    <div class="edit-actions">
      <button
        class="edit-save-btn"
        onmousedown={(e) => e.preventDefault()}
        onclick={() => saveEdit(propertyIri)}
        disabled={saving}
      >
        {#if saving}
          <span class="material-symbols-outlined spinning-small">progress_activity</span>
        {:else}
          <span class="material-symbols-outlined">check</span>
        {/if}
        Save
      </button>
      <button
        class="edit-cancel-btn"
        onmousedown={(e) => e.preventDefault()}
        onclick={cancelEdit}
      >
        <span class="material-symbols-outlined">close</span>
        Cancel
      </button>
    </div>
  </div>
{/snippet}

{#snippet detailItem(detailGroup)}
  <div class="detail-item" transition:slide={{ duration: 300, easing: cubicOut }}>
    <div class="detail-header">
      <div class="detail-name">
        {detailGroup.propertyLabel}
        {#if detailGroup.propertyComment || detailGroup.sourceClassLabel}
          <span
            class="material-symbols-outlined prop-info"
            role="img"
            aria-label="Property info"
            onmouseenter={(e) => showHint(e, detailGroup.propertyComment, detailGroup.sourceClassLabel, detailGroup.sourceClassIcon)}
            onmouseleave={hideHint}
          >info</span>
        {/if}
        {#if detailGroup.isObjectProperty}
          <span class="detail-type detail-type-object">
            {#if detailGroup.rangeClassIcon}
              <span class="material-symbols-outlined detail-type-icon">{detailGroup.rangeClassIcon}</span>
            {/if}
            {detailGroup.rangeClassLabel ?? 'Object'}
          </span>
        {:else}
          <span class="detail-type">{formatDatatype(detailGroup.datatype)}</span>
        {/if}
        {#if detailGroup.isCalculated}
          <span class="calculated-badge" title="Calculated field">ƒ</span>
        {/if}
        {#if detailGroup.values.length > 1}
          <span class="detail-count">{detailGroup.values.length}</span>
        {/if}
        {#if isClass && (detailGroup.minCount !== null || detailGroup.maxCount !== null)}
          <span class="cardinality-badge">{formatCardinality(detailGroup.minCount, detailGroup.maxCount)}</span>
        {/if}
      </div>
      {#if onSave && !detailGroup.isObjectProperty && !detailGroup.isCalculated && (isStringType(detailGroup.datatype) || isDateType(detailGroup.datatype)) && (detailGroup.isEmpty || detailGroup.values.length <= 1)}
        <button
          class="edit-btn"
          title="Edit"
          onclick={() => startEdit(detailGroup.property, detailGroup.values[0]?.value ?? '', 0, detailGroup.datatype ?? null)}
        >
          <span class="material-symbols-outlined">edit</span>
        </button>
      {:else if onSaveReference && detailGroup.isObjectProperty && !detailGroup.isCalculated}
        <button
          class="edit-btn"
          title="Edit"
          onclick={() => startRefEdit(detailGroup.property)}
        >
          <span class="material-symbols-outlined">edit</span>
        </button>
      {:else if isClass}
        <div class="class-prop-actions">
          {#if onSaveCardinality && !detailGroup.sourceClassLabel}
            <button
              class="edit-btn"
              class:active={editingCardinalityKey === detailGroup.property}
              title="Edit cardinality"
              onclick={() => {
                if (editingCardinalityKey === detailGroup.property) {
                  cancelCardinalityEdit();
                } else {
                  startCardinalityEdit(detailGroup.property, detailGroup.minCount, detailGroup.maxCount);
                }
              }}
            >
              <span class="material-symbols-outlined">rule</span>
            </button>
          {/if}
          <button
            class="edit-btn"
            title="Inspect property"
            onclick={() => openEntityInspector(detailGroup.property)}
          >
            <span class="material-symbols-outlined">open_in_new</span>
          </button>
          {#if onRemoveProperty && !detailGroup.sourceClassLabel}
            <button
              class="edit-btn remove-btn"
              title="Remove property"
              onclick={() => onRemoveProperty(detailGroup.property, detailGroup.propertyLabel)}
            >
              <span class="material-symbols-outlined">delete</span>
            </button>
          {/if}
        </div>
      {/if}
    </div>

    {#if editingCardinalityKey === detailGroup.property}
      <div class="cardinality-edit-form">
        <div class="cardinality-edit-row">
          <label class="cardinality-edit-label">Min</label>
          <input
            class="cardinality-input"
            type="number"
            min="0"
            bind:value={cardinalityDraftMin}
            placeholder="0"
            onkeydown={(e) => {
              if (e.key === 'Escape') cancelCardinalityEdit();
              else if (e.key === 'Enter') saveCardinalityEdit(detailGroup.property);
            }}
          />
          <label class="cardinality-edit-label">Max</label>
          <input
            class="cardinality-input"
            type="number"
            min="0"
            bind:value={cardinalityDraftMax}
            placeholder="∞"
            onkeydown={(e) => {
              if (e.key === 'Escape') cancelCardinalityEdit();
              else if (e.key === 'Enter') saveCardinalityEdit(detailGroup.property);
            }}
          />
        </div>
        <div class="edit-actions">
          <button
            class="edit-save-btn"
            onmousedown={(e) => e.preventDefault()}
            onclick={() => saveCardinalityEdit(detailGroup.property)}
            disabled={savingCardinality}
          >
            {#if savingCardinality}
              <span class="material-symbols-outlined spinning-small">progress_activity</span>
            {:else}
              <span class="material-symbols-outlined">check</span>
            {/if}
            Save
          </button>
          <button
            class="edit-cancel-btn"
            onmousedown={(e) => e.preventDefault()}
            onclick={cancelCardinalityEdit}
          >
            <span class="material-symbols-outlined">close</span>
            Cancel
          </button>
        </div>
      </div>
    {/if}

    {#if editingRefKey === detailGroup.property}
      <ReferenceSelect
        propertyIri={detailGroup.property}
        rangeClassIri={detailGroup.rangeClassIri}
        rangeClassLabel={detailGroup.rangeClassLabel}
        currentValues={detailGroup.values.map(v => ({
          iri: v.value,
          label: v.valueLabel ?? v.value,
          icon: v.valueIcon ?? null,
        }))}
        minCount={detailGroup.minCount}
        maxCount={detailGroup.maxCount}
        saving={savingRef}
        onsave={saveRefEdit}
        oncancel={cancelRefEdit}
      />
    {:else if detailGroup.isEmpty && editingKey !== editKey(detailGroup.property, 0)}
      <div class="empty-value">—</div>
    {:else if detailGroup.isEmpty && editingKey === editKey(detailGroup.property, 0)}
      <div class="detail-value">
        {#if isDateType(editingDatatype)}
          {@render dateEditor(detailGroup.property, editingDatatype === 'xsd:dateTime' ? 'datetime-local' : 'date')}
        {:else}
          <PropertyEditForm
            propertyIri={detailGroup.property}
            bind:draftValue
            {saving}
            onsave={saveEdit}
            oncancel={cancelEdit}
          />
        {/if}
      </div>
    {:else if detailGroup.rangeClassIri === 'foundation:File'}
      <FileGrid values={detailGroup.values} {openEntityInspector} />
    {:else}
      <div class="detail-values-group">
        {#each detailGroup.values as val, idx (detailGroup.property + '_' + val.value + '_' + idx)}
          {#if detailGroup.isObjectProperty}
            <div
              class="detail-value clickable"
              class:calculated={detailGroup.isCalculated}
              role="button"
              tabindex="0"
              onclick={() => openEntityInspector(val.value)}
              onkeydown={(e) => e.key === 'Enter' && openEntityInspector(val.value)}
            >
              {#if val.valueIcon}
                {#if isIconUrl(val.valueIcon)}
                  <img src={getIconUrl(val.valueIcon)} alt="" class="value-icon-image" />
                {:else}
                  <span class="material-symbols-outlined value-icon">{val.valueIcon}</span>
                {/if}
              {/if}
              {#if val.unitLabel}
                <span class="unit">{val.unitLabel}</span>
              {/if}
              <span class="value-text">{val.valueLabel || val.value}</span>
              {#if val.valueStatus}
                <span
                  class="inline-status"
                  style="--status-color: {val.valueStatus.color || 'var(--color-neutral)'}"
                  title={val.valueStatus.iri}
                >
                  <span class="material-symbols-outlined inline-status-icon">radio_button_checked</span>
                  <span class="inline-status-label">{val.valueStatus.label}</span>
                </span>
              {/if}
              {#if val.formulaError}
                <span class="formula-error" title={val.formulaError}>
                  <span class="material-symbols-outlined formula-error-icon">warning</span>
                  <span class="formula-error-text">{val.formulaError}</span>
                </span>
              {/if}
            </div>
          {:else}
            <div class="detail-value" class:calculated={detailGroup.isCalculated}>
              {#if val.datatype === 'xsd:dateTime'}
                {#if editingKey === editKey(detailGroup.property, idx)}
                  {@render dateEditor(detailGroup.property, 'datetime-local')}
                {:else}
                  {@const date = new Date(val.value)}
                  <div class="timestamp-display">
                    <span class="value-text">
                      {date.toLocaleString('en-US', {
                        year: 'numeric', month: 'short', day: 'numeric',
                        hour: 'numeric', minute: '2-digit', second: '2-digit', hour12: true
                      })}
                    </span>
                    <span class="timestamp-relative">{formatDate(date.getTime())}</span>
                  </div>
                {/if}
              {:else if val.datatype === 'xsd:date'}
                {#if editingKey === editKey(detailGroup.property, idx)}
                  {@render dateEditor(detailGroup.property, 'date')}
                {:else}
                  {@const [y, m, d] = val.value.split('-').map(Number)}
                  {@const date = new Date(y, m - 1, d)}
                  <span class="value-text">
                    {date.toLocaleDateString('en-US', { year: 'numeric', month: 'short', day: 'numeric' })}
                  </span>
                {/if}
              {:else if isUrl(val.datatype)}
                <button class="url-value" onclick={() => openUrl_(val.value)} title={val.value}>
                  <span class="value-text">{val.valueLabel || val.value}</span>
                  <span class="material-symbols-outlined url-open-icon">open_in_new</span>
                </button>
              {:else if isStringType(val.datatype)}
                {#if editingKey === editKey(detailGroup.property, idx)}
                  <PropertyEditForm
                    propertyIri={detailGroup.property}
                    bind:draftValue
                    {saving}
                    onsave={saveEdit}
                    oncancel={cancelEdit}
                  />
                {:else if (val.value ?? '').length > 50_000}
                  <div class="value-large">
                    <pre class="value-pre">{val.value}</pre>
                    <button class="copy-btn" onclick={() => navigator.clipboard.writeText(val.value)} title="Copy value">
                      <span class="material-symbols-outlined">content_copy</span>
                    </button>
                  </div>
                {:else}
                  <MarkdownValue value={val.value} />
                {/if}
              {:else}
                {#if val.unitLabel}
                  <span class="unit">{val.unitLabel}</span>
                {/if}
                <span class="value-text">{val.valueLabel || val.value}</span>
              {/if}
              {#if val.valueStatus}
                <span
                  class="inline-status"
                  style="--status-color: {val.valueStatus.color || 'var(--color-neutral)'}"
                  title={val.valueStatus.iri}
                >
                  <span class="material-symbols-outlined inline-status-icon">radio_button_checked</span>
                  <span class="inline-status-label">{val.valueStatus.label}</span>
                </span>
              {/if}
              {#if val.formulaError}
                <span class="formula-error" title={val.formulaError}>
                  <span class="material-symbols-outlined formula-error-icon">warning</span>
                  <span class="formula-error-text">{val.formulaError}</span>
                </span>
              {/if}
            </div>
          {/if}
        {/each}
      </div>
    {/if}
  </div>
{/snippet}

{#snippet sourceGroups(groups, sepTop = 0)}
  {#each groups as sourceGroup}
    {#if sourceGroup.sourceClassLabel}
      <div class="source-separator" use:sticky={{ top: sepTop }}>{sourceGroup.sourceClassLabel}</div>
    {/if}
    {#each sourceGroup.items as detailGroup (detailGroup.property)}
      {@render detailItem(detailGroup)}
    {/each}
  {/each}
{/snippet}

<div
  use:bodyPortal
  class="prop-hint-portal"
  class:prop-hint-visible={hintVisible}
  style:left="{hintX}px"
  style:top="{hintY}px"
>
  {#if hintDesc}
    <p class="prop-hint-desc">{hintDesc}</p>
  {/if}
  {#if hintSourceLabel}
    <div class="prop-hint-source" class:with-sep={hintDesc}>
      <span class="prop-hint-chip">
        {#if hintSourceIcon}
          <span class="material-symbols-outlined prop-hint-chip-icon">{hintSourceIcon}</span>
        {/if}
        {hintSourceLabel}
      </span>
    </div>
  {/if}
</div>

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

  .detail-item {
    padding: 10px 12px;
    background: color-mix(in srgb, var(--color-white) 3%, transparent);
    border-radius: 8px;
    border-left: 3px solid color-mix(in srgb, var(--color-neutral) 30%, transparent);
  }

  .detail-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 6px;
    gap: 4px;
  }

  .detail-name {
    font-family: var(--font-title);
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--color-neutral-active);
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .prop-info {
    font-size: 13px;
    color: var(--color-neutral);
    opacity: 0.45;
    cursor: default;
    user-select: none;
    flex-shrink: 0;
  }

  .prop-info:hover {
    opacity: 0.8;
  }

  .detail-type {
    font-size: 10px;
    padding: 2px 6px;
    background: color-mix(in srgb, var(--color-neutral) 20%, transparent);
    color: var(--color-neutral);
    border-radius: 4px;
    font-weight: 600;
  }

  .detail-type-object {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }

  .detail-type-icon {
    font-size: 11px;
  }

  .detail-count {
    font-size: 10px;
    padding: 2px 6px;
    background: color-mix(in srgb, var(--color-accent) 20%, transparent);
    color: var(--color-accent);
    border-radius: 4px;
    font-weight: 600;
  }

  .cardinality-badge {
    font-size: 10px;
    padding: 2px 6px;
    background: color-mix(in srgb, var(--color-interactive) 20%, transparent);
    color: var(--color-interactive);
    border-radius: 4px;
    font-weight: 600;
    font-family: var(--font-mono, monospace);
  }

  .cardinality-edit-form {
    padding: 8px;
    background: color-mix(in srgb, var(--color-white) 4%, transparent);
    border-radius: 6px;
    margin-bottom: 8px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .cardinality-edit-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .cardinality-edit-label {
    font-size: 11px;
    font-weight: 700;
    color: var(--color-neutral);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    min-width: 26px;
  }

  .cardinality-input {
    width: 70px;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    border: 1px solid var(--color-interactive);
    border-radius: 6px;
    padding: 4px 8px;
    font-size: 13px;
    color: var(--color-neutral-active);
    outline: none;
    text-align: center;
  }

  .cardinality-input::-webkit-inner-spin-button,
  .cardinality-input::-webkit-outer-spin-button {
    opacity: 0.4;
  }

  .edit-btn.active {
    color: var(--color-interactive);
    background: color-mix(in srgb, var(--color-interactive) 15%, transparent);
  }

  .empty-value {
    font-family: var(--font-body);
    font-size: 13px;
    color: var(--color-neutral);
    opacity: 0.35;
    padding: 2px 0;
  }

  .detail-values-group {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .detail-value {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px;
    background: color-mix(in srgb, var(--color-black) 30%, transparent);
    border-radius: 6px;
  }

  .value-large {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  .value-pre {
    font-family: monospace;
    font-size: 11px;
    color: var(--color-neutral-active);
    background: color-mix(in srgb, var(--color-black) 40%, transparent);
    border-radius: 4px;
    padding: 8px;
    max-height: 200px;
    overflow-y: auto;
    white-space: pre-wrap;
    word-break: break-all;
    margin: 0;
  }

  .copy-btn {
    align-self: flex-end;
    background: none;
    border: none;
    cursor: pointer;
    color: var(--color-neutral);
    padding: 2px;
    display: flex;
    align-items: center;
    border-radius: 4px;
    transition: color 0.15s;
  }

  .copy-btn:hover {
    color: var(--color-neutral-active);
  }

  .copy-btn .material-symbols-outlined {
    font-size: 16px;
  }

  .clickable {
    cursor: pointer;
    user-select: none;
  }

  .clickable:hover {
    background: color-mix(in srgb, var(--color-interactive) 20%, transparent) !important;
    transform: translateX(2px);
  }

  .clickable:active {
    transform: translateX(1px);
  }

  .value-icon {
    font-size: 18px;
    color: var(--color-interactive);
  }

  .value-icon-image {
    width: 24px;
    height: 24px;
    border-radius: 4px;
    object-fit: cover;
  }

  .value-text {
    font-family: var(--font-body);
    font-size: 13px;
    color: var(--color-neutral-active);
    flex: 1;
  }

  .unit {
    font-size: 11px;
    color: var(--color-neutral);
    padding: 2px 6px;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    border-radius: 4px;
  }

  .timestamp-display {
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1;
  }

  .timestamp-relative {
    font-size: 11px;
    color: var(--color-neutral);
    opacity: 0.6;
    font-family: var(--font-body);
    font-style: italic;
  }

  .url-value {
    display: flex;
    align-items: center;
    gap: 4px;
    flex: 1;
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    color: var(--color-interactive);
    text-align: left;
  }

  .url-value:hover .value-text {
    text-decoration: underline;
  }

  .url-open-icon {
    font-size: 14px;
    opacity: 0.6;
    flex-shrink: 0;
  }

  .inline-status {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 2px 7px 2px 4px;
    background: color-mix(in srgb, var(--status-color) 18%, transparent);
    border: 1px solid color-mix(in srgb, var(--status-color) 40%, transparent);
    border-radius: 20px;
    flex-shrink: 0;
    margin-left: auto;
  }

  .inline-status-icon {
    font-size: 12px;
    color: var(--status-color);
  }

  .inline-status-label {
    font-family: var(--font-body);
    font-size: 10px;
    font-weight: 600;
    color: var(--status-color);
    white-space: nowrap;
  }

  .calculated-badge {
    font-size: 11px;
    font-weight: 700;
    font-style: italic;
    padding: 2px 6px;
    background: color-mix(in srgb, var(--color-accent) 15%, transparent);
    color: var(--color-accent);
    border-radius: 4px;
    line-height: 1;
    cursor: default;
  }

  .detail-value.calculated {
    font-style: italic;
    color: var(--color-neutral);
    background: color-mix(in srgb, var(--color-black) 20%, transparent);
    border-left: 2px solid color-mix(in srgb, var(--color-accent) 40%, transparent);
  }

  .detail-value.calculated .value-text {
    color: var(--color-neutral);
  }

  .formula-error {
    display: flex;
    align-items: flex-start;
    gap: 4px;
    padding: 4px 6px;
    background: color-mix(in srgb, var(--color-error, #ef4444) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-error, #ef4444) 30%, transparent);
    border-radius: 4px;
    width: 100%;
    margin-top: 4px;
    box-sizing: border-box;
  }

  .formula-error-icon {
    font-size: 14px;
    color: var(--color-error, #ef4444);
    flex-shrink: 0;
    line-height: 1.4;
  }

  .formula-error-text {
    font-family: var(--font-body);
    font-size: 11px;
    color: var(--color-error, #ef4444);
    line-height: 1.4;
  }

  .edit-btn {
    background: none;
    border: none;
    cursor: pointer;
    color: var(--color-neutral);
    opacity: 0;
    padding: 2px;
    display: flex;
    align-items: center;
    border-radius: 4px;
    transition: color 0.15s, opacity 0.15s;
    flex-shrink: 0;
  }

  .edit-btn .material-symbols-outlined {
    font-size: 14px;
  }

  .edit-btn:hover {
    color: var(--color-neutral-active);
    background: color-mix(in srgb, var(--color-white) 8%, transparent);
  }

  .detail-item:hover .edit-btn {
    opacity: 1;
  }

  .class-prop-actions {
    display: flex;
    gap: 2px;
    flex-shrink: 0;
  }

  .remove-btn:hover {
    color: var(--color-error, #ef4444) !important;
  }

  .date-edit-container {
    display: flex;
    flex-direction: column;
    gap: 6px;
    flex: 1;
    min-width: 0;
  }

  .date-input {
    background: color-mix(in srgb, var(--color-black) 50%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-interactive) 50%, transparent);
    border-radius: 6px;
    color: var(--color-neutral-active);
    font-family: var(--font-body);
    font-size: 13px;
    padding: 6px 8px;
    outline: none;
    transition: border-color 0.15s;
    color-scheme: dark;
  }

  .date-input:focus {
    border-color: var(--color-interactive);
  }

  .edit-actions {
    display: flex;
    gap: 6px;
  }

  .edit-save-btn,
  .edit-cancel-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-family: var(--font-body);
    font-size: 12px;
    font-weight: 600;
    transition: background 0.15s;
  }

  .edit-save-btn {
    background: color-mix(in srgb, var(--color-interactive) 25%, transparent);
    color: var(--color-interactive);
  }

  .edit-save-btn:hover:not(:disabled) {
    background: color-mix(in srgb, var(--color-interactive) 40%, transparent);
  }

  .edit-save-btn:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .edit-cancel-btn {
    background: color-mix(in srgb, var(--color-neutral) 12%, transparent);
    color: var(--color-neutral);
  }

  .edit-cancel-btn:hover {
    background: color-mix(in srgb, var(--color-neutral) 22%, transparent);
  }

  .edit-save-btn .material-symbols-outlined,
  .edit-cancel-btn .material-symbols-outlined {
    font-size: 14px;
  }

  .spinning-small {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  :global(.prop-hint-portal) {
    position: fixed;
    z-index: 2147483647;
    transform: translate(-50%, -100%);
    background: color-mix(in srgb, var(--color-black) 88%, var(--color-white) 12%);
    border: 1px solid color-mix(in srgb, var(--color-neutral) 22%, transparent);
    border-radius: 6px;
    padding: 8px 10px;
    min-width: 160px;
    max-width: 260px;
    pointer-events: none;
    opacity: 0;
    visibility: hidden;
    transition: opacity 0.12s, visibility 0.12s;
    box-shadow: 0 4px 16px color-mix(in srgb, var(--color-black) 60%, transparent);
    text-transform: none;
    letter-spacing: normal;
    font-family: var(--font-body);
  }

  :global(.prop-hint-portal.prop-hint-visible) {
    opacity: 1;
    visibility: visible;
  }

  :global(.prop-hint-desc) {
    font-size: 11px;
    color: var(--color-neutral-active);
    line-height: 1.5;
    margin: 0;
  }

  :global(.prop-hint-source) {
    display: flex;
    align-items: center;
  }

  :global(.prop-hint-source.with-sep) {
    margin-top: 6px;
    padding-top: 6px;
    border-top: 1px solid color-mix(in srgb, var(--color-neutral) 15%, transparent);
  }

  :global(.prop-hint-chip) {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 7px 2px 5px;
    background: color-mix(in srgb, var(--color-neutral) 14%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-neutral) 20%, transparent);
    border-radius: 20px;
    font-size: 10px;
    font-weight: 600;
    color: var(--color-neutral);
  }

  :global(.prop-hint-chip-icon) {
    font-size: 12px;
    opacity: 0.8;
  }
</style>
