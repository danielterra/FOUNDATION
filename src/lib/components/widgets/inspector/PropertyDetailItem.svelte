<script>
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import FileGrid from './FileGrid.svelte';
  import PropertyEditForm from './PropertyEditForm.svelte';
  import PropertyValuesGroup from './PropertyValuesGroup.svelte';
  import ReferenceSelect from './ReferenceSelect.svelte';
  import ReferenceSingleSelect from './ReferenceSingleSelect.svelte';
  import RecurrenceEditor from './RecurrenceEditor.svelte';
  import QueryConfigEditor from './QueryConfigEditor.svelte';
  import { focus } from '$lib/utils/actions';

  let {
    detailGroup,
    isClass = false,
    now,
    openEntityInspector,
    onSave = null,
    onSaveReference = null,
    onClearProperty = null,
    onRemoveProperty = null,
    onSaveCardinality = null,
    onSaveQueryConfig = null,
    onLoadMoreBacklinks = null,
    onShowHint,
    onHideHint,
  } = $props();

  let editingKey = $state(null);
  let draftValue = $state('');
  let editingDatatype = $state(null);
  let saving = $state(false);
  let editingRefKey = $state(null);
  let savingRef = $state(false);
  let editingCardinalityKey = $state(null);
  let extraValues = $state([]);
  let loadingMore = $state(false);

  $effect(() => {
    void detailGroup.values;
    extraValues = [];
  });

  const hasMore = $derived(
    detailGroup.groupTotal != null &&
    (detailGroup.values.length + extraValues.length) < detailGroup.groupTotal
  );

  async function loadMore() {
    if (!onLoadMoreBacklinks || loadingMore) return;
    loadingMore = true;
    try {
      const offset = detailGroup.values.length + extraValues.length;
      const items = await onLoadMoreBacklinks(
        detailGroup.property,
        detailGroup.sourceClassIri,
        offset
      );
      extraValues = [...extraValues, ...items];
    } finally {
      loadingMore = false;
    }
  }
  let cardinalityDraftMin = $state('');
  let cardinalityDraftMax = $state('');
  let savingCardinality = $state(false);

  let editingQueryConfigKey = $state(null);
  let savingQueryConfig = $state(false);

  async function saveQueryConfigEdit(propertyIri, json) {
    if (!onSaveQueryConfig || savingQueryConfig) return;
    savingQueryConfig = true;
    try {
      await onSaveQueryConfig(propertyIri, json);
    } finally {
      savingQueryConfig = false;
      editingQueryConfigKey = null;
    }
  }

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

  function isRruleType(datatype) {
    return datatype === 'foundation:rrule';
  }

  function isQueryConfigProperty(propertyIri) {
    return propertyIri === 'foundation:queryConfig';
  }

  function iriShort(iri) {
    if (!iri) return '';
    const i = iri.lastIndexOf(':');
    return i >= 0 ? iri.slice(i + 1) : iri;
  }

  function queryConfigSummary(json) {
    if (!json) return null;
    try {
      const c = JSON.parse(json);
      return { targetClass: c.targetClass || '', filters: c.filters || [] };
    } catch {
      return null;
    }
  }

  function filterLabel(f) {
    const prop = iriShort(f.propertyIri);
    const ops = { eq: '=', neq: '≠', gt: '>', lt: '<', gte: '≥', lte: '≤' };
    if (f.operator === 'exists') return `${prop} existe`;
    if (f.operator === 'not_exists') return `${prop} não existe`;
    if (f.operator === 'between') return `${prop} entre ${f.valueFrom ?? '?'} e ${f.valueTo ?? '?'}`;
    return `${prop} ${ops[f.operator] ?? f.operator} ${f.value ?? ''}`;
  }

  function rruleLabel(rrule) {
    if (!rrule) return 'Nunca';
    if (rrule.includes('FREQ=HOURLY')) return 'A Cada Hora';
    if (rrule.includes('BYDAY=MO,TU,WE,TH,FR')) return 'Dias de Semana';
    if (rrule.includes('BYDAY=SA,SU')) return 'Fins de Semana';
    if (rrule.includes('FREQ=DAILY')) return 'Diariamente';
    if (rrule.includes('FREQ=WEEKLY') && rrule.includes('INTERVAL=2')) return 'Quinzenalmente';
    if (rrule.includes('FREQ=WEEKLY')) return 'Semanalmente';
    if (rrule.includes('FREQ=MONTHLY') && rrule.includes('INTERVAL=6')) return 'A Cada 6 Meses';
    if (rrule.includes('FREQ=MONTHLY') && rrule.includes('INTERVAL=3')) return 'A Cada 3 Meses';
    if (rrule.includes('FREQ=MONTHLY')) return 'Mensalmente';
    if (rrule.includes('FREQ=YEARLY')) return 'Anualmente';
    return 'Personalizada';
  }

  function isStringType(datatype) {
    return !datatype || datatype === 'xsd:string' || datatype === 'rdf:langString';
  }

  function isDateType(datatype) {
    return datatype === 'xsd:date' || datatype === 'xsd:dateTime';
  }

  function isNumericType(datatype) {
    return datatype === 'xsd:decimal' || datatype === 'xsd:integer' ||
           datatype === 'xsd:int' || datatype === 'xsd:long' ||
           datatype === 'xsd:float' || datatype === 'xsd:double' ||
           datatype === 'xsd:nonNegativeInteger' || datatype === 'xsd:positiveInteger';
  }

  function numericStep(datatype) {
    return (datatype === 'xsd:integer' || datatype === 'xsd:int' ||
            datatype === 'xsd:long' || datatype === 'xsd:nonNegativeInteger' ||
            datatype === 'xsd:positiveInteger') ? '1' : 'any';
  }

  function toInputValue(value, datatype) {
    if (datatype === 'xsd:date') return value || '';
    if (datatype === 'xsd:dateTime') {
      const d = value
        ? new Date(isNaN(Number(value)) ? value : Number(value))
        : new Date();
      if (isNaN(d.getTime())) return '';
      const pad = n => String(n).padStart(2, '0');
      return `${d.getFullYear()}-${pad(d.getMonth()+1)}-${pad(d.getDate())}` +
             `T${pad(d.getHours())}:${pad(d.getMinutes())}`;
    }
    return value;
  }

  function fromInputValue(value, datatype) {
    if (value === '' || value === null || value === undefined) return '';
    if (datatype === 'xsd:dateTime') {
      return new Date(value).toISOString();
    }
    return String(value);
  }

  function formatDatatype(datatype) {
    if (!datatype || datatype === 'xsd:string' || datatype === 'rdf:langString') return 'String';
    if (datatype === 'foundation:rrule') return 'Recorrência';
    const parts = datatype.split(':');
    const typeName = parts.length > 1 ? parts[1] : datatype;
    return typeName.charAt(0).toUpperCase() + typeName.slice(1);
  }

  function datePart(v) { return v?.split('T')[0] ?? ''; }
  function timePart(v) { return v?.split('T')[1] ?? ''; }
  function combineDatetime(d, t) {
    if (!d) return '';
    return t ? `${d}T${t}` : `${d}T00:00`;
  }
</script>

{#snippet dateEditor(propertyIri, inputType)}
  <div class="date-edit-container">
    {#if inputType === 'datetime-local'}
      <div class="datetime-pair">
        <input
          class="date-input"
          type="date"
          value={datePart(draftValue)}
          oninput={(e) => draftValue = combineDatetime(e.currentTarget.value, timePart(draftValue))}
          onchange={(e) => draftValue = combineDatetime(e.currentTarget.value, timePart(draftValue))}
          use:focus
        />
        <input
          class="date-input"
          type="time"
          value={timePart(draftValue)}
          oninput={(e) => draftValue = combineDatetime(datePart(draftValue), e.currentTarget.value)}
          onchange={(e) => draftValue = combineDatetime(datePart(draftValue), e.currentTarget.value)}
          onkeydown={(e) => {
            if (e.key === 'Escape') cancelEdit();
            else if (e.key === 'Enter') saveEdit(propertyIri);
          }}
        />
      </div>
    {:else}
      <input
        class="date-input"
        type="date"
        value={draftValue}
        oninput={(e) => draftValue = e.currentTarget.value}
        onchange={(e) => draftValue = e.currentTarget.value}
        onkeydown={(e) => {
          if (e.key === 'Escape') cancelEdit();
          else if (e.key === 'Enter') saveEdit(propertyIri);
        }}
        use:focus
      />
    {/if}
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

{#snippet numericEditor(propertyIri)}
  <div class="date-edit-container">
    <input
      class="date-input"
      type="number"
      step={numericStep(editingDatatype)}
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

<div class="detail-item" transition:slide={{ duration: 300, easing: cubicOut }}>
  <div class="detail-header">
    <div class="detail-name field-label">
      {detailGroup.propertyLabel}
      {#if detailGroup.propertyComment || detailGroup.sourceClassLabel}
        <span
          class="material-symbols-outlined prop-info"
          role="img"
          aria-label="Property info"
          onmouseenter={(e) => onShowHint(e, detailGroup.propertyComment, detailGroup.sourceClassLabel, detailGroup.sourceClassIcon)}
          onmouseleave={onHideHint}
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
      {#if detailGroup.isQueryProperty}
        <span class="calculated-badge query-badge" title="Coleção derivada — valores resolvidos automaticamente por filtros">
          <span class="material-symbols-outlined calculated-icon">manage_search</span>
          Query
        </span>
      {:else if detailGroup.isCalculated}
        <span class="calculated-badge" title="Campo calculado automaticamente">
          <span class="material-symbols-outlined calculated-icon">calculate</span>
          Calculado
        </span>
      {/if}
      {#if (detailGroup.values.length + extraValues.length) > 1}
        <span class="detail-count">{detailGroup.values.length + extraValues.length}{#if detailGroup.groupTotal && (detailGroup.values.length + extraValues.length) < detailGroup.groupTotal}/{detailGroup.groupTotal}{/if}</span>
      {/if}
      {#if isClass && (detailGroup.minCount !== null || detailGroup.maxCount !== null)}
        <span class="cardinality-badge">{formatCardinality(detailGroup.minCount, detailGroup.maxCount)}</span>
      {/if}
    </div>
    {#if isClass}
      <div class="class-prop-actions">
        {#if onSaveQueryConfig && detailGroup.isQueryProperty && !detailGroup.sourceClassLabel}
          <button
            class="edit-btn"
            class:active={editingQueryConfigKey === detailGroup.property}
            title="Editar query config"
            onclick={() => {
              editingQueryConfigKey = editingQueryConfigKey === detailGroup.property ? null : detailGroup.property;
            }}
          >
            <span class="material-symbols-outlined">manage_search</span>
          </button>
        {/if}
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
    {:else}
      <div class="prop-actions">
        {#if onSave && !detailGroup.isObjectProperty && !detailGroup.isCalculated && (isStringType(detailGroup.datatype) || isDateType(detailGroup.datatype) || isNumericType(detailGroup.datatype) || isRruleType(detailGroup.datatype) || isQueryConfigProperty(detailGroup.property)) && (detailGroup.isEmpty || detailGroup.values.length <= 1)}
          <button
            class="edit-btn"
            title="Edit"
            onclick={() => startEdit(detailGroup.property, detailGroup.values[0]?.value ?? '', 0, detailGroup.datatype ?? null)}
          >
            <span class="material-symbols-outlined">edit</span>
          </button>
        {:else if onSaveReference && detailGroup.isObjectProperty && !detailGroup.isCalculated && !detailGroup.sourceClass}
          <button
            class="edit-btn"
            title="Edit"
            onclick={() => startRefEdit(detailGroup.property)}
          >
            <span class="material-symbols-outlined">edit</span>
          </button>
        {/if}
        {#if onClearProperty && !detailGroup.isCalculated && !detailGroup.isEmpty}
          <button
            class="edit-btn clear-btn"
            title="Limpar valor"
            onclick={() => onClearProperty(detailGroup.property)}
          >
            <span class="material-symbols-outlined">backspace</span>
          </button>
        {/if}
      </div>
    {/if}
  </div>

  {#if editingCardinalityKey === detailGroup.property}
    <div class="cardinality-edit-form">
      <div class="cardinality-edit-row">
        <label class="cardinality-edit-label" for="cardinality-min-input">Min</label>
        <input
          id="cardinality-min-input"
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
        <label class="cardinality-edit-label" for="cardinality-max-input">Max</label>
        <input
          id="cardinality-max-input"
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

  {#if editingQueryConfigKey === detailGroup.property}
    <div class="query-config-edit-container">
      <QueryConfigEditor
        value={detailGroup.queryConfig ?? ''}
        onconfirm={(json) => saveQueryConfigEdit(detailGroup.property, json)}
        oncancel={() => { editingQueryConfigKey = null; }}
      />
    </div>
  {/if}

  {#if editingRefKey === detailGroup.property}
    {#if detailGroup.maxCount === 1}
      <ReferenceSingleSelect
        propertyIri={detailGroup.property}
        rangeClassIri={detailGroup.rangeClassIri}
        rangeClassLabel={detailGroup.rangeClassLabel}
        currentValue={detailGroup.values[0] ? {
          iri: detailGroup.values[0].value,
          label: detailGroup.values[0].valueLabel ?? detailGroup.values[0].value,
          icon: detailGroup.values[0].valueIcon ?? null,
        } : null}
        saving={savingRef}
        onsave={saveRefEdit}
        oncancel={cancelRefEdit}
      />
    {:else}
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
    {/if}
  {:else if detailGroup.isEmpty && editingKey !== editKey(detailGroup.property, 0)}
    <div class="empty-value">—</div>
  {:else if detailGroup.isEmpty && editingKey === editKey(detailGroup.property, 0)}
    <div class="detail-value">
      {#if isQueryConfigProperty(detailGroup.property)}
        <QueryConfigEditor
          value={detailGroup.values[0]?.value ?? ''}
          onconfirm={(json) => { draftValue = json; saveEdit(detailGroup.property); }}
          oncancel={cancelEdit}
        />
      {:else if isDateType(editingDatatype)}
        {@render dateEditor(detailGroup.property, editingDatatype === 'xsd:dateTime' ? 'datetime-local' : 'date')}
      {:else if isNumericType(editingDatatype)}
        {@render numericEditor(detailGroup.property)}
      {:else if isRruleType(editingDatatype)}
        <RecurrenceEditor
          value={detailGroup.values[0]?.value ?? ''}
          onconfirm={(rrule) => { draftValue = rrule; saveEdit(detailGroup.property); }}
          oncancel={cancelEdit}
        />
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
  {:else if isQueryConfigProperty(detailGroup.property)}
    {#if editingKey === editKey(detailGroup.property, 0)}
      <div class="detail-value">
        <QueryConfigEditor
          value={detailGroup.values[0]?.value ?? ''}
          onconfirm={(json) => { draftValue = json; saveEdit(detailGroup.property); }}
          oncancel={cancelEdit}
        />
      </div>
    {:else}
      {@const cfg = queryConfigSummary(detailGroup.values[0]?.value ?? '')}
      {#if cfg}
        <div class="qc-display">
          <div class="qc-target">
            <span class="material-symbols-outlined qc-icon">manage_search</span>
            <span class="qc-class">{iriShort(cfg.targetClass)}</span>
          </div>
          {#if cfg.filters.length > 0}
            <div class="qc-filters">
              {#each cfg.filters as f}
                <div class="qc-filter-line">{filterLabel(f)}</div>
              {/each}
            </div>
          {/if}
        </div>
      {:else}
        <div class="empty-value">—</div>
      {/if}
    {/if}
  {:else if isClass && detailGroup.isQueryProperty}
    {@const cfg = queryConfigSummary(detailGroup.queryConfig ?? '')}
    {#if cfg}
      <div class="qc-display">
        <div class="qc-target">
          <span class="material-symbols-outlined qc-icon">manage_search</span>
          <span class="qc-class">{iriShort(cfg.targetClass)}</span>
        </div>
        {#if cfg.filters.length > 0}
          <div class="qc-filters">
            {#each cfg.filters as f}
              <div class="qc-filter-line">{filterLabel(f)}</div>
            {/each}
          </div>
        {/if}
      </div>
    {:else}
      <div class="empty-value">—</div>
    {/if}
  {:else if detailGroup.rangeClassIri === 'foundation:File'}
    <FileGrid values={detailGroup.values} {openEntityInspector} />
  {:else}
    <PropertyValuesGroup
      {detailGroup}
      {extraValues}
      {editingKey}
      {editingDatatype}
      bind:draftValue
      {saving}
      {now}
      onSaveEdit={saveEdit}
      onCancelEdit={cancelEdit}
      {openEntityInspector}
    />
    {#if hasMore && onLoadMoreBacklinks}
      <button class="load-more-btn" onclick={loadMore} disabled={loadingMore}>
        {#if loadingMore}
          <span class="material-symbols-outlined spinning-small">progress_activity</span>
        {:else}
          <span class="material-symbols-outlined">expand_more</span>
          Carregar mais ({detailGroup.groupTotal - detailGroup.values.length - extraValues.length})
        {/if}
      </button>
    {/if}
  {/if}
</div>

<style>
  .detail-item {
    padding: 15px 0px;
  }

  .detail-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 6px;
    gap: 4px;
  }

  /* .detail-name layout-only adjustments; typography comes from global .field-label */
  .detail-name {
    flex: 1;
    min-width: 0;
  }

  .prop-info {
    font-size: 14px;
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
    font-weight: 600;
  }

  .cardinality-badge {
    font-size: 10px;
    padding: 2px 6px;
    background: color-mix(in srgb, var(--color-interactive) 20%, transparent);
    color: var(--color-interactive);
    font-weight: 600;
    font-family: var(--font-mono, monospace);
  }

  .cardinality-edit-form {
    padding: 8px;
    background: color-mix(in srgb, var(--color-white) 4%, transparent);
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
    border: none;
    padding: 4px 8px;
    font-size: 14px;
    color: var(--color-neutral-active);
    outline: none;
    text-align: center;
  }

  .cardinality-input::-webkit-inner-spin-button,
  .cardinality-input::-webkit-outer-spin-button {
    opacity: 0.4;
  }

  .edit-btn {
    background: none;
    border: none;
    cursor: pointer;
    color: var(--color-interactive);
    opacity: 0;
    padding: 2px;
    display: flex;
    align-items: center;
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

  .edit-btn.active {
    color: var(--color-interactive);
    background: color-mix(in srgb, var(--color-interactive) 15%, transparent);
  }

  .prop-actions {
    display: flex;
    gap: 2px;
    flex-shrink: 0;
  }

  .class-prop-actions {
    display: flex;
    gap: 2px;
    flex-shrink: 0;
  }

  .remove-btn:hover {
    color: var(--color-error, #ef4444) !important;
  }

  .empty-value {
    font-family: var(--font-body);
    font-size: 14px;
    color: var(--color-neutral);
    opacity: 0.35;
    padding: 2px 0;
  }

  .detail-value {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px;
  }

  .query-config-edit-container {
    padding: 8px;
    background: color-mix(in srgb, var(--color-white) 4%, transparent);
    margin-bottom: 8px;
  }

  .qc-display {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 2px 0;
  }

  .qc-target {
    display: flex;
    align-items: center;
    gap: 5px;
  }

  .qc-icon {
    font-size: 14px;
    color: var(--color-interactive);
  }

  .qc-class {
    font-family: var(--font-mono, monospace);
    font-size: 13px;
    color: var(--color-neutral-active);
    font-weight: 600;
  }

  .qc-filters {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding-left: 19px;
  }

  .qc-filter-line {
    font-family: var(--font-mono, monospace);
    font-size: 12px;
    color: var(--color-neutral);
  }

  .calculated-badge {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.04em;
    padding: 2px 8px 2px 5px;
    background: color-mix(in srgb, var(--color-accent) 18%, transparent);
    color: var(--color-accent);
    line-height: 1;
    cursor: default;
  }

  .calculated-icon {
    font-size: 13px;
    line-height: 1;
  }

  .query-badge {
    background: color-mix(in srgb, var(--color-interactive) 18%, transparent);
    color: var(--color-interactive);
  }

  .datetime-pair {
    display: flex;
    gap: 6px;
  }

  .datetime-pair .date-input {
    flex: 1;
    min-width: 0;
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
    border: none;
    color: var(--color-neutral-active);
    font-family: var(--font-body);
    font-size: 14px;
    padding: 6px 8px;
    outline: none;
    color-scheme: dark;
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

  .load-more-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    margin-top: 6px;
    padding: 4px 0;
    background: none;
    border: none;
    cursor: pointer;
    font-family: var(--font-body);
    font-size: 12px;
    font-weight: 600;
    color: var(--color-interactive);
    opacity: 0.7;
  }

  .load-more-btn:disabled {
    cursor: default;
    opacity: 0.4;
  }

  .load-more-btn .material-symbols-outlined {
    font-size: 14px;
  }
</style>
