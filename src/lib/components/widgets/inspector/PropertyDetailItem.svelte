<script>
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { Button } from '$lib/components/ui/button';
  import FileGrid from './FileGrid.svelte';
  import PropertyEditForm from './PropertyEditForm.svelte';
  import PropertyValuesGroup from './PropertyValuesGroup.svelte';
  import ReferenceSelect from './ReferenceSelect.svelte';
  import ReferenceSingleSelect from './ReferenceSingleSelect.svelte';
  import PropertySelect from './PropertySelect.svelte';
  import RecurrenceEditor from './RecurrenceEditor.svelte';
  import QueryConfigEditor from './QueryConfigEditor.svelte';
  import { Input } from '$lib/components/ui/input';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import InspectorValueEntry from './InspectorValueEntry.svelte';
  import InspectorReferenceDecision from './InspectorReferenceDecision.svelte';
  import InspectorInformation from './InspectorInformation.svelte';
  import InspectorDetails from './InspectorDetails.svelte';
  import OntologyEntityIdentity from './OntologyEntityIdentity.svelte';

  let {
    detailGroup,
    isClass = false,
    now,
    entityId = null,
    openEntityInspector,
    onSave = null,
    editMode = false,
    drafts = {},
    fieldErrors = {},
    onDraftChange = null,
    onSaveReference = null,
    onClearProperty = null,
    onRemoveProperty = null,
    onSaveCardinality = null,
    onSaveQueryConfig = null,
    onLoadMoreBacklinks = null,
    onLoadMoreProperty = null,
  } = $props();

  const isMultipleLiteralEntry = $derived(detailGroup.maxCount !== 1);
  const isGlobalEditable = $derived(
    !isClass && editMode && onDraftChange && onSave &&
    !detailGroup.isObjectProperty && !detailGroup.isCalculated &&
    !detailGroup.sourceClassIri && !detailGroup.isQueryProperty &&
    (isMultipleLiteralEntry || detailGroup.isEmpty || detailGroup.values.length === 1) &&
    (isTextEditable(detailGroup.datatype) || detailGroup.datatype === 'xsd:anyURI' || detailGroup.datatype === 'xsd:boolean' || isDateType(detailGroup.datatype) || isNumericType(detailGroup.datatype) || isRruleType(detailGroup.datatype) || isQueryConfigProperty(detailGroup.property))
  );
  const isGlobalReferenceEditable = $derived(
    !isClass && editMode && onDraftChange && onSaveReference &&
    (detailGroup.isObjectProperty || detailGroup.isPropertyPicker) && !detailGroup.isCalculated &&
    !detailGroup.sourceClassIri && !detailGroup.isQueryProperty
  );

  function semanticValueType() {
    if (detailGroup.presentation === 'code') return 'code';
    if (detailGroup.datatype === 'rdf:HTML') return 'html';
    if (detailGroup.datatype === 'foundation:markdown') return 'markdown';
    if (isRruleType(detailGroup.datatype)) return 'recurrence_rule';
    if (isQueryConfigProperty(detailGroup.property)) return 'query_config';
    if (detailGroup.datatype === 'xsd:dateTime') return 'date_time';
    if (detailGroup.datatype === 'xsd:date') return 'date';
    if (detailGroup.datatype === 'xsd:boolean') return 'boolean';
    if (isNumericType(detailGroup.datatype)) return 'number';
    if (detailGroup.datatype === 'xsd:anyURI') return 'url';
    if (detailGroup.datatype === 'xsd:string' && detailGroup.property.toLowerCase().includes('email')) return 'email';
    return 'text';
  }

  function currentDraft() {
    if (Object.hasOwn(drafts, detailGroup.property)) return drafts[detailGroup.property];
    if (isMultipleLiteralEntry) return [...detailGroup.values, ...extraValues].map((item) => String(item.value ?? ''));
    return detailGroup.values[0]?.value ?? '';
  }

  function currentReferenceValues() {
    if (!Object.hasOwn(drafts, detailGroup.property)) return detailGroup.values.map((item) => ({
      iri: item.value,
      label: item.valueLabel ?? item.value,
      icon: item.valueIcon ?? null,
    }));
    return (drafts[detailGroup.property] ?? []).map((iri) => {
      const previous = detailGroup.values.find((item) => item.value === iri);
      return { iri, label: previous?.valueLabel ?? iri, icon: previous?.valueIcon ?? null };
    });
  }

  let editingKey = $state(null);
  let draftValue = $state('');
  let editingDatatype = $state(null);
  let saving = $state(false);
  let saveError = $state(null);
  let editingRefKey = $state(null);
  let savingRef = $state(false);
  let editingCardinalityKey = $state(null);
  let extraValues = $state([]);
  let loadingMore = $state(false);
  let loadingAllForEdit = $state(false);
  let nextCursor = $state(null);
  let pageHasMore = $state(false);

  let _lastGroupKey = $state.raw(null);
  let _lastEntityId = $state.raw(null);

  $effect(() => {
    const groupKey = detailGroup._groupKey;
    const currentEntityId = entityId;
    if (groupKey !== _lastGroupKey || currentEntityId !== _lastEntityId) {
      _lastGroupKey = groupKey;
      _lastEntityId = currentEntityId;
      extraValues = [];
      const isBacklink = detailGroup.sourceClassIri != null;
      nextCursor = isBacklink
        ? (detailGroup.backlinkNextCursor ?? null)
        : (detailGroup.propertyNextCursor ?? null);
      pageHasMore = nextCursor != null;
    }
  });

  const hasMore = $derived(pageHasMore);

  async function loadMore() {
    if (loadingMore) return;
    loadingMore = true;
    try {
      const isBacklink = detailGroup.sourceClassIri != null;
      if (isBacklink && onLoadMoreBacklinks) {
        const result = await onLoadMoreBacklinks(
          detailGroup.property,
          detailGroup.sourceClassIri,
          nextCursor
        );
        extraValues = [...extraValues, ...result.items];
        nextCursor = result.next_cursor;
        pageHasMore = result.has_more;
      } else if (!isBacklink && onLoadMoreProperty) {
        const result = await onLoadMoreProperty(detailGroup.property, nextCursor);
        const mapped = result.items.map(item => ({
          value: item.value,
          valueLabel: item.valueLabel ?? null,
          valueIcon: item.valueIcon ?? null,
          valueEntityKind: item.valueEntityKind ?? null,
          valueStatus: item.valueStatus ?? null,
          datatype: item.datatype ?? null,
          presentation: item.presentation ?? null,
          language: item.language ?? null,
          unitLabel: null,
          formulaError: null,
          fileInfo: null,
        }));
        extraValues = [...extraValues, ...mapped];
        nextCursor = result.next_cursor;
        pageHasMore = result.has_more;
      }
    } finally {
      loadingMore = false;
    }
  }

  async function loadAllValuesForEdit() {
    if (loadingAllForEdit) return;
    loadingAllForEdit = true;
    try {
      while (pageHasMore) await loadMore();
    } finally {
      loadingAllForEdit = false;
    }
  }
  let cardinalityDraftMin = $state('');
  let cardinalityDraftMax = $state('');
  let savingCardinality = $state(false);

  let previewElement = $state(null);
  let isPreviewExpanded = $state(false);
  let hasPreviewOverflow = $state(false);

  const isReadOnlyPreview = $derived(
    !editMode && !editingKey && !editingRefKey &&
    editingCardinalityKey !== detailGroup.property &&
    editingQueryConfigKey !== detailGroup.property
  );
  const previewId = $derived(`property-preview-${String(detailGroup._groupKey ?? detailGroup.property).replace(/[^a-zA-Z0-9_-]/g, '-')}`);
  const semanticReadValues = $derived([
    ...detailGroup.values,
    ...extraValues,
  ].map((item) => String(item.valueLabel ?? item.value ?? '')).filter(Boolean));
  const semanticDetailFacts = $derived(semanticReadValues.map((value, index) => ({
    label: semanticReadValues.length === 1 ? 'Conteúdo' : `Valor ${index + 1}`,
    value,
  })));
  const semanticSummary = $derived(semanticReadValues.join(' · ').slice(0, 240));

  $effect(() => {
    const groupKey = detailGroup._groupKey;
    const currentEntityId = entityId;
    if (groupKey !== _lastGroupKey || currentEntityId !== _lastEntityId) {
      isPreviewExpanded = false;
      hasPreviewOverflow = false;
    }
  });

  $effect(() => {
    const element = previewElement;
    if (!element || !isReadOnlyPreview || isPreviewExpanded) return;

    let frame = 0;
    const measure = () => {
      cancelAnimationFrame(frame);
      frame = requestAnimationFrame(() => {
        hasPreviewOverflow = element.scrollHeight > element.clientHeight + 1;
      });
    };
    const resizeObserver = new ResizeObserver(measure);
    resizeObserver.observe(element);
    const mutationObserver = new MutationObserver(measure);
    mutationObserver.observe(element, { subtree: true, childList: true, characterData: true, attributes: true });
    measure();

    return () => {
      cancelAnimationFrame(frame);
      resizeObserver.disconnect();
      mutationObserver.disconnect();
    };
  });

  let editingQueryConfigKey = $state(null);
  let savingQueryConfig = $state(false);

  let dateInputRef = $state(null);
  let numericInputRef = $state(null);
  let cardinalityMinRef = $state(null);

  $effect(() => {
    if (editMode) return;
    editingRefKey = null;
    editingKey = null;
    draftValue = '';
    editingDatatype = null;
    saveError = null;
  });

  $effect(() => {
    if (dateInputRef) dateInputRef.focus();
  });
  $effect(() => {
    if (numericInputRef) numericInputRef.focus();
  });

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
    saveError = null;
  }

  function cancelEdit() {
    editingKey = null;
    draftValue = '';
    editingDatatype = null;
    saveError = null;
  }

  function extractErrorLine(errMsg) {
    const match = String(errMsg).match(/line\s+(\d+)/i) || String(errMsg).match(/(\d+):\d+/);
    return match ? `Linha ${match[1]}` : null;
  }

  async function saveEdit(propertyIri) {
    if (!onSave || saving) return;
    if (editMode && onDraftChange) {
      onDraftChange(propertyIri, fromInputValue(draftValue, editingDatatype), editingDatatype, 'literal');
      editingKey = null;
      draftValue = '';
      editingDatatype = null;
      return;
    }
    saving = true;
    saveError = null;
    try {
      await onSave(propertyIri, fromInputValue(draftValue, editingDatatype), editingDatatype);
      editingKey = null;
      draftValue = '';
      editingDatatype = null;
    } catch (err) {
      const msg = String(err);
      const line = extractErrorLine(msg);
      const detail = msg.replace(/^.*?error[:\s]*/i, '').split('\n')[0].trim();
      saveError = line ? `${line}: ${detail}` : detail;
    } finally {
      saving = false;
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
      if (editMode && onDraftChange) onDraftChange(propertyIri, iris, null, 'references');
      else await onSaveReference(propertyIri, iris);
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
    if (rrule.includes('FREQ=MINUTELY')) {
      const m = rrule.match(/INTERVAL=(\d+)/);
      const n = m ? parseInt(m[1], 10) : 1;
      return n === 1 ? 'A Cada Minuto' : `A Cada ${n} Minutos`;
    }
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

  // Datatypes edited through the same plain-textarea form (markdown/html are just
  // text under the hood — only their rendering differs).
  function isTextEditable(datatype) {
    return isStringType(datatype) || datatype === 'foundation:markdown' || datatype === 'rdf:HTML';
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
        <Input
          class="date-input"
          type="date"
          value={datePart(draftValue)}
          bind:ref={dateInputRef}
          oninput={(e) => draftValue = combineDatetime(e.currentTarget.value, timePart(draftValue))}
          onchange={(e) => draftValue = combineDatetime(e.currentTarget.value, timePart(draftValue))}
        />
        <Input
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
      <Input
        class="date-input"
        type="date"
        value={draftValue}
        bind:ref={dateInputRef}
        oninput={(e) => draftValue = e.currentTarget.value}
        onchange={(e) => draftValue = e.currentTarget.value}
        onkeydown={(e) => {
          if (e.key === 'Escape') cancelEdit();
          else if (e.key === 'Enter') saveEdit(propertyIri);
        }}
      />
    {/if}
    {#if !editMode}<div class="edit-actions">
      <Button
        variant="default"
        size="sm"
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
      </Button>
      <Button
        variant="ghost"
        size="sm"
        class="preview-toggle-button"
        onmousedown={(e) => e.preventDefault()}
        onclick={cancelEdit}
      >
        <span class="material-symbols-outlined">close</span>
        Cancel
      </Button>
    </div>{/if}
  </div>
{/snippet}

{#snippet numericEditor(propertyIri)}
  <div class="date-edit-container">
    <Input
      class="date-input"
      type="number"
      step={numericStep(editingDatatype)}
      bind:value={draftValue}
      bind:ref={numericInputRef}
      onkeydown={(e) => {
        if (e.key === 'Escape') cancelEdit();
        else if (e.key === 'Enter') saveEdit(propertyIri);
      }}
    />
    {#if !editMode}<div class="edit-actions">
      <Button
        variant="default"
        size="sm"
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
      </Button>
      <Button
        variant="ghost"
        size="sm"
        onmousedown={(e) => e.preventDefault()}
        onclick={cancelEdit}
      >
        <span class="material-symbols-outlined">close</span>
        Cancel
      </Button>
    </div>{/if}
  </div>
{/snippet}

<div class="detail-item" class:class-entity={isClass} transition:slide={{ duration: 300, easing: cubicOut }}>
  <div class="detail-header">
    <div class="detail-heading">
      <div class="detail-name field-label">
        {detailGroup.propertyLabel}
        {#if detailGroup.propertyComment || detailGroup.sourceClassLabel}
          <Tooltip.Root>
            <Tooltip.Trigger class="prop-info-trigger" aria-label="Ajuda sobre {detailGroup.propertyLabel}">
              <span class="material-symbols-outlined prop-info" aria-hidden="true">info</span>
            </Tooltip.Trigger>
            <Tooltip.Content class="prop-hint-content">
              {#if detailGroup.propertyComment}
                <p class="prop-hint-desc">{detailGroup.propertyComment}</p>
              {/if}
              {#if detailGroup.sourceClassLabel}
                <div class="prop-hint-source" class:with-sep={detailGroup.propertyComment}>
                  <span class="prop-hint-chip">
                    <OntologyEntityIdentity
                      kind="class"
                      label={detailGroup.sourceClassLabel}
                      icon={detailGroup.sourceClassIcon}
                      compact
                    />
                  </span>
                </div>
              {/if}
            </Tooltip.Content>
          </Tooltip.Root>
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
    </div>
    {#if isClass}
      <div class="class-prop-actions">
        {#if onSaveQueryConfig && detailGroup.isQueryProperty && !detailGroup.sourceClassLabel}
          <Button
            variant="ghost"
            size="icon-sm"
            class={editingQueryConfigKey === detailGroup.property ? 'edit-btn active' : 'edit-btn'}
            title="Editar query config"
            onclick={() => {
              editingQueryConfigKey = editingQueryConfigKey === detailGroup.property ? null : detailGroup.property;
            }}
          >
            <span class="material-symbols-outlined">manage_search</span>
          </Button>
        {/if}
        {#if onSaveCardinality && !detailGroup.sourceClassLabel}
          <Button
            variant="ghost"
            size="icon-sm"
            class={editingCardinalityKey === detailGroup.property ? 'edit-btn active' : 'edit-btn'}
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
          </Button>
        {/if}
        <Button
          variant="ghost"
          size="icon-sm"
          class="edit-btn"
          title="Inspect property"
          onclick={() => openEntityInspector(detailGroup.property)}
        >
          <span class="material-symbols-outlined">open_in_new</span>
        </Button>
        {#if onRemoveProperty && !detailGroup.sourceClassLabel}
          <Button
            variant="ghost"
            size="icon-sm"
            class="edit-btn remove-btn"
            title="Remove property"
            onclick={() => onRemoveProperty(detailGroup.property, detailGroup.propertyLabel)}
          >
            <span class="material-symbols-outlined">delete</span>
          </Button>
        {/if}
      </div>
    {:else}
      <div class="prop-actions"></div>
    {/if}
  </div>

  <InspectorInformation title={detailGroup.propertyLabel ?? detailGroup.property} active={isReadOnlyPreview}>
  <InspectorDetails
    title={detailGroup.propertyLabel ?? detailGroup.property}
    summary={semanticSummary}
    details={semanticDetailFacts.length ? semanticDetailFacts : [{ label: 'Conteúdo', value: 'Conteúdo complementar' }]}
    expanded={isPreviewExpanded}
    active={isReadOnlyPreview && hasPreviewOverflow}
  >
  <div id={previewId} bind:this={previewElement} class="detail-content" class:preview-collapsed={isReadOnlyPreview && !isPreviewExpanded}>
  {#if isGlobalReferenceEditable}
    <InspectorReferenceDecision
      title={detailGroup.propertyLabel ?? detailGroup.property}
      rangeLabel={detailGroup.rangeClassLabel}
      multiple={detailGroup.maxCount !== 1}
      selectedValues={currentReferenceValues()}
    >
      {#snippet children()}
        {#if detailGroup.isPropertyPicker}
          <PropertySelect
            nodeIri={entityId}
            propertyIri={detailGroup.property}
            currentValue={currentReferenceValues()[0] ?? null}
            saving={savingRef}
            draftOnly={editMode}
            onsave={saveRefEdit}
            oncancel={cancelRefEdit}
          />
        {:else if detailGroup.maxCount === 1}
          <ReferenceSingleSelect
            propertyIri={detailGroup.property}
            rangeClassIri={detailGroup.rangeClassIri}
            rangeClassLabel={detailGroup.rangeClassLabel}
            currentValue={currentReferenceValues()[0] ?? null}
            saving={savingRef}
            draftOnly={editMode}
            onsave={saveRefEdit}
            oncancel={cancelRefEdit}
          />
        {:else}
          <ReferenceSelect
            propertyIri={detailGroup.property}
            rangeClassIri={detailGroup.rangeClassIri}
            rangeClassLabel={detailGroup.rangeClassLabel}
            currentValues={currentReferenceValues()}
            minCount={detailGroup.minCount}
            maxCount={detailGroup.maxCount}
            saving={savingRef}
            draftOnly={editMode}
            onsave={saveRefEdit}
            oncancel={cancelRefEdit}
          />
        {/if}
      {/snippet}
    </InspectorReferenceDecision>
    {#if fieldErrors[detailGroup.property]}
      <div class="save-error" role="alert">
        <span class="material-symbols-outlined save-error-icon">error</span>
        <span class="save-error-text">{fieldErrors[detailGroup.property]}</span>
      </div>
    {/if}
  {:else if isGlobalEditable}
    {#if isMultipleLiteralEntry && hasMore}
      <div class="load-all-values" role="status">
        <span>Há valores ainda não carregados. Carregue o conjunto completo antes de editar para preservar todos os dados.</span>
        <Button variant="outline" size="sm" onclick={loadAllValuesForEdit} disabled={loadingAllForEdit || loadingMore}>
          <span class:spinning-small={loadingAllForEdit} class="material-symbols-outlined">{loadingAllForEdit ? 'progress_activity' : 'unfold_more'}</span>
          {loadingAllForEdit ? 'Carregando valores…' : 'Carregar todos para editar'}
        </Button>
      </div>
    {:else}
      <InspectorValueEntry
        label={detailGroup.propertyLabel ?? detailGroup.property}
        value={currentDraft()}
        cardinality={isMultipleLiteralEntry ? 'multiple' : 'single'}
        valueType={semanticValueType()}
        language={detailGroup.language}
        required={Boolean(detailGroup.minCount && detailGroup.minCount > 0)}
        onvaluechange={(value) => onDraftChange(detailGroup.property, value, detailGroup.datatype ?? null, Array.isArray(value) ? 'literal-set' : 'literal')}
      />
    {/if}
    {#if fieldErrors[detailGroup.property]}
      <div class="save-error" role="alert">
        <span class="material-symbols-outlined save-error-icon">error</span>
        <span class="save-error-text">{fieldErrors[detailGroup.property]}</span>
      </div>
    {/if}
  {:else}
  {#if editingCardinalityKey === detailGroup.property}
    <div class="cardinality-edit-form">
      <div class="cardinality-edit-row">
        <label class="cardinality-edit-label" for="cardinality-min-input">Min</label>
        <Input
          id="cardinality-min-input"
          class="cardinality-input"
          type="number"
          min="0"
          bind:value={cardinalityDraftMin}
          placeholder="0"
          bind:ref={cardinalityMinRef}
          onkeydown={(e) => {
            if (e.key === 'Escape') cancelCardinalityEdit();
            else if (e.key === 'Enter') saveCardinalityEdit(detailGroup.property);
          }}
        />
        <label class="cardinality-edit-label" for="cardinality-max-input">Max</label>
        <Input
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
        <Button
          variant="default"
          size="sm"
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
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onmousedown={(e) => e.preventDefault()}
          onclick={cancelCardinalityEdit}
        >
          <span class="material-symbols-outlined">close</span>
          Cancel
        </Button>
      </div>
    </div>
  {/if}

  {#if !editMode && editingQueryConfigKey === detailGroup.property}
    <div class="query-config-edit-container">
      <QueryConfigEditor
        value={detailGroup.queryConfig ?? ''}
        onconfirm={(json) => saveQueryConfigEdit(detailGroup.property, json)}
        oncancel={() => { editingQueryConfigKey = null; }}
      />
    </div>
  {/if}

  {#if !isGlobalReferenceEditable && editingRefKey === detailGroup.property}
    {#if detailGroup.isPropertyPicker}
      <PropertySelect
        nodeIri={entityId}
        propertyIri={detailGroup.property}
        currentValue={detailGroup.values[0] ? {
          iri: detailGroup.values[0].value,
          label: detailGroup.values[0].valueLabel ?? detailGroup.values[0].value,
        } : null}
        saving={savingRef}
        onsave={saveRefEdit}
        oncancel={cancelRefEdit}
      />
    {:else if detailGroup.maxCount === 1}
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
  {:else if !editMode && detailGroup.isEmpty && editingKey === editKey(detailGroup.property, 0)}
    <div class="detail-value">
      {#if isQueryConfigProperty(detailGroup.property)}
        <QueryConfigEditor
          value={detailGroup.values[0]?.value ?? ''}
          draftOnly={editMode}
          onDraftChange={(json) => onDraftChange?.(detailGroup.property, json, editingDatatype, 'literal')}
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
          draftOnly={editMode}
          onDraftChange={(rrule) => onDraftChange?.(detailGroup.property, rrule, editingDatatype, 'literal')}
          onconfirm={(rrule) => { draftValue = rrule; saveEdit(detailGroup.property); }}
          oncancel={cancelEdit}
        />
      {:else}
        <PropertyEditForm
          propertyIri={detailGroup.property}
          bind:draftValue
          {saving}
          mono={detailGroup.property === 'foundation:script'}
          onsave={saveEdit}
          oncancel={cancelEdit}
        />
      {/if}
    </div>
  {:else if isQueryConfigProperty(detailGroup.property)}
    {#if !editMode && editingKey === editKey(detailGroup.property, 0)}
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
      editingKey={editMode ? null : editingKey}
      {editingDatatype}
      bind:draftValue
      {saving}
      mono={detailGroup.property === 'foundation:script'}
      {now}
      onSaveEdit={saveEdit}
      onCancelEdit={cancelEdit}
      {openEntityInspector}
    />
    {#if hasMore && (onLoadMoreBacklinks || onLoadMoreProperty)}
      <Button variant="ghost" size="sm" class="load-more-btn" onclick={loadMore} disabled={loadingMore}>
        {#if loadingMore}
          <span class="material-symbols-outlined spinning-small">progress_activity</span>
        {:else}
          {@const remaining = detailGroup.groupTotal != null && extraValues.length === 0
            ? detailGroup.groupTotal - detailGroup.values.length
            : null}
          <span class="material-symbols-outlined">expand_more</span>
          {remaining != null ? `Carregar mais (${remaining})` : 'Carregar mais'}
        {/if}
      </Button>
    {/if}
  {/if}
  {#if saveError}
    <div class="save-error" role="alert">
      <span class="material-symbols-outlined save-error-icon">error</span>
      <span class="save-error-text">{saveError}</span>
    </div>
  {/if}
  {/if}
  </div>
  {#if isReadOnlyPreview && hasPreviewOverflow}
    <div class="preview-toggle-container">
      <Button
        variant="ghost"
        size="sm"
        class="preview-toggle-button"
        aria-controls={previewId}
        aria-expanded={isPreviewExpanded}
        onclick={() => isPreviewExpanded = !isPreviewExpanded}
      >
        <span class="material-symbols-outlined" aria-hidden="true">{isPreviewExpanded ? 'expand_less' : 'expand_more'}</span>
        {isPreviewExpanded ? 'Mostrar menos' : 'Mostrar mais'}
      </Button>
    </div>
  {/if}
  </InspectorDetails>
  </InspectorInformation>
</div>

<style>
  .detail-item {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-inspector-label-value);
    padding: 0;
  }

  .detail-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--spacing-inline-related);
    min-width: 0;
  }

  .detail-heading {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--spacing-inline-tight);
    min-width: 0;
  }

  .detail-name {
    display: flex;
    flex-direction: row;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--spacing-inline-related);
    line-height: var(--line-height-hud-compact);
    overflow-wrap: anywhere;
    min-width: 0;
  }

  .detail-content {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-inline-related);
    min-width: 0;
  }

  .detail-content.preview-collapsed {
    max-height: var(--size-inspector-preview-lines);
    overflow: hidden;
  }

  .preview-toggle-container {
    align-self: flex-start;
    margin-top: var(--spacing-inline-tight);
  }

  .preview-toggle-container :global([data-slot="button"].preview-toggle-button) {
    color: var(--color-interactive);
    justify-content: flex-start;
    border: 1px solid var(--color-hud-line-subtle);
    border-radius: 0;
  }

  .preview-toggle-container :global([data-slot="button"].preview-toggle-button:hover) {
    background: var(--color-surface-hover-subtle);
    border-color: var(--color-interactive);
  }

  .load-all-values {
    display: grid;
    gap: var(--spacing-control-internal);
    color: var(--text-secondary);
    font-size: var(--type-body-compact);
  }

  :global(.prop-info-trigger) {
    display: inline-flex;
    align-items: center;
    background: none;
    border: none;
    padding: 0;
    cursor: default;
    flex-shrink: 0;
    color: inherit;
    line-height: 1;
    border-radius: 2px;
    outline: none;
  }

  :global(.prop-info-trigger:focus-visible) {
    outline: 1px solid var(--color-selection);
    outline-offset: 1px;
  }

  .prop-info {
    font-size: var(--type-body);
    color: var(--text-tertiary);
    opacity: 0.45;
    user-select: none;
    flex-shrink: 0;
    pointer-events: none;
  }

  :global(.prop-info-trigger:hover) .prop-info,
  :global(.prop-info-trigger:focus-visible) .prop-info {
    opacity: 0.8;
  }

  :global(.prop-hint-content) {
    background: color-mix(in srgb, var(--color-black) 88%, var(--color-white) 12%);
    border: none;
    padding: 8px 10px;
    min-width: 160px;
    max-width: 260px;
    font-family: var(--font-body);
    box-shadow: 0 4px 16px color-mix(in srgb, var(--color-black) 60%, transparent);
    text-transform: none;
    letter-spacing: normal;
    border-radius: var(--radius);
  }

  :global(.prop-hint-content .prop-hint-desc) {
    font-size: var(--type-caption-compact);
    color: var(--color-neutral-active);
    line-height: 1.5;
    margin: 0;
  }

  :global(.prop-hint-content .prop-hint-source) {
    display: flex;
    align-items: center;
  }

  :global(.prop-hint-content .prop-hint-source.with-sep) {
    margin-top: 6px;
    padding-top: 6px;
  }

  :global(.prop-hint-content .prop-hint-chip) {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 7px 2px 5px;
    background: color-mix(in srgb, var(--color-neutral) 14%, transparent);
    font-size: var(--type-overline);
    color: var(--text-secondary);
    border-radius: 4px;
  }

  :global(.prop-hint-content .prop-hint-chip-icon) {
    font-size: var(--type-label);
    opacity: 0.8;
  }

  .detail-count {
    font-size: var(--type-overline);
    padding: 2px 6px;
    background: color-mix(in srgb, var(--color-accent) 20%, transparent);
    color: var(--color-accent);
  }

  .cardinality-badge {
    font-size: var(--type-overline);
    padding: 2px 6px;
    background: color-mix(in srgb, var(--color-interactive) 20%, transparent);
    color: var(--color-interactive);
    font-family: var(--font-family-app);
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
    font-size: var(--type-caption-compact);
    color: var(--color-neutral);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    min-width: 26px;
  }

  :global([data-slot="input"].cardinality-input) {
    width: 70px;
    height: auto;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    border: none;
    padding: 4px 8px;
    font-size: var(--type-body);
    color: var(--color-neutral-active);
    outline: none;
    box-shadow: none;
    text-align: center;
  }

  :global([data-slot="input"].cardinality-input::-webkit-inner-spin-button),
  :global([data-slot="input"].cardinality-input::-webkit-outer-spin-button) {
    opacity: 0.4;
  }

  :global([data-slot="button"].edit-btn) {
    color: var(--color-interactive);
    opacity: 0;
    transition: color 0.15s, opacity 0.15s;
    flex-shrink: 0;
  }

  :global([data-slot="button"].edit-btn:hover) {
    color: var(--color-neutral-active);
    background: color-mix(in srgb, var(--color-white) 8%, transparent);
  }

  .detail-item:hover :global([data-slot="button"].edit-btn) {
    opacity: 1;
  }

  :global([data-slot="button"].edit-btn.active) {
    color: var(--color-selection);
    background: var(--color-selection-subtle);
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

  :global([data-slot="button"].remove-btn:hover) {
    color: var(--color-error) !important;
  }

  .empty-value {
    font-family: var(--font-body);
    font-size: var(--type-body);
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
    font-size: var(--type-body);
    color: var(--color-interactive);
  }

  .qc-class {
    font-family: var(--font-family-app);
    font-size: var(--type-body-compact);
    color: var(--color-neutral-active);
  }

  .qc-filters {
    display: flex;
    flex-direction: column;
    gap: 3px;
    padding-left: 19px;
  }

  .qc-filter-line {
    font-family: var(--font-family-app);
    font-size: var(--type-label);
    color: var(--color-neutral);
  }

  .calculated-badge {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: var(--type-overline);
    letter-spacing: 0.04em;
    padding: 2px 8px 2px 5px;
    background: color-mix(in srgb, var(--color-accent) 18%, transparent);
    color: var(--color-accent);
    line-height: 1;
    cursor: default;
  }

  .calculated-icon {
    font-size: var(--type-body-compact);
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

  .datetime-pair :global([data-slot="input"].date-input) {
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

  :global([data-slot="input"].date-input) {
    background: color-mix(in srgb, var(--color-black) 50%, transparent);
    border: none;
    color: var(--color-neutral-active);
    font-family: var(--font-body);
    font-size: var(--type-body);
    padding: 6px 8px;
    outline: none;
    box-shadow: none;
    height: auto;
    color-scheme: dark;
  }

  .edit-actions {
    display: flex;
    gap: 6px;
  }

  .spinning-small {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  :global([data-slot="button"].load-more-btn) {
    margin-top: 6px;
    color: var(--color-interactive);
    opacity: 0.7;
    font-size: var(--type-label);
    padding-inline: 0;
  }

  :global([data-slot="button"].load-more-btn:disabled) {
    opacity: 0.4;
  }

  .save-error {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    margin-top: 6px;
    padding: 6px 8px;
    background: color-mix(in srgb, var(--color-error) 12%, transparent);
  }

  .save-error-icon {
    font-size: var(--type-body);
    color: var(--color-error);
    flex-shrink: 0;
    line-height: 1.4;
  }

  .save-error-text {
    font-family: var(--font-family-app);
    font-size: var(--type-label);
    color: var(--color-error);
    line-height: 1.4;
    word-break: break-word;
  }
</style>
