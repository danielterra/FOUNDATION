<script lang="ts">
  import { getBoundProp, type BaseComponentProps } from '@json-render/svelte';
  import { getContext } from 'svelte';
  import { Button } from '$lib/components/ui/button';
  import * as Tooltip from '$lib/components/ui/tooltip';
  import { resolveDecisionPresentation } from './decisionPresentation';
  import CodeEditor from '$lib/components/widgets/inspector/CodeEditor.svelte';
  import QueryConfigEditor from '$lib/components/widgets/inspector/QueryConfigEditor.svelte';
  import RecurrenceEditor from '$lib/components/widgets/inspector/RecurrenceEditor.svelte';
  import OntologyEntityReferenceChip from '$lib/components/widgets/inspector/OntologyEntityReferenceChip.svelte';
  import { ENTITY_BINDING_CONTEXT, type BoundPropertyItem, type EntityBindingContext, type SemanticRecordsCriteria, type SemanticRecordsRow } from './entityBindingContext';
  import { semanticCatalog } from './catalog';
  import { informationFactLabelVisible, informationPrimaryChildSpans as shouldSpanPrimaryInformationChild, informationSummaryVisible, SEMANTIC_COMPONENT_CONTRACT_VERSIONS } from './runtimeSpec';
  import { createSemanticDomIds } from './domIds';
  import { areComparisonUnitsCompatible, calculateComparison, canCalculateComparison, formatComparisonMonth, formatSemanticAmount, formatSemanticPercent, isComparisonPeriodClosed, previousComparisonMonth } from './comparison';
  import { aggregateDistribution, resolveDistributionCoverage } from './distribution';
  import { formatLocalizedDate, formatLocalizedDateTime, formatRelativeCalendarDate, formatRelativeTime } from '$lib/utils/dateFormatters';
  import { onMount } from 'svelte';

  type Candidate = { value: string; title: string; description?: string; disabled?: boolean; comparison?: Record<string, string> };
  type Fact = { label: string; value: string; unit?: string };
  type Relationship = { label: string; value: string };
  type Detail = { label: string; value: string };
  type SemanticProps = BaseComponentProps<Record<string, unknown>> & { componentType: string };

  let { props, componentType, children, emit, on, bindings = {} }: SemanticProps = $props();
  const componentContractVersions = getContext<Record<string, number | null>>(SEMANTIC_COMPONENT_CONTRACT_VERSIONS);
  let componentContractVersion = $derived(componentContractVersions?.[componentType] ?? null);
  const componentInstanceId = $props.id();
  const semanticDomIds = createSemanticDomIds(componentInstanceId);
  const entityBindingContext = getContext<EntityBindingContext | undefined>(ENTITY_BINDING_CONTEXT);
  let selected = $state<string[]>([]);
  let contextTooltipOpen = $state(false);
  let search = $state('');
  let value = $state('');
  let values = $state<string[]>([]);
  const selectedBinding = getBoundProp<string | string[]>(() => props.selected as string | string[] | undefined, () => bindings.selected);
  const valueBinding = getBoundProp<string | string[]>(() => props.value as string | string[] | undefined, () => bindings.value);

  $effect(() => {
    const current = selectedBinding.current ?? props.selected;
    selected = Array.isArray(current) ? current.map(String) : typeof current === 'string' ? [current] : [];
    const currentValue = valueBinding.current ?? props.value ?? '';
    if (Array.isArray(currentValue)) {
      values = currentValue.map(String);
      value = '';
    } else {
      value = String(currentValue);
      values = [];
    }
  });

  let candidates = $derived((Array.isArray(props.candidates) ? props.candidates : []) as Candidate[]);
  let comparisonFactors = $derived((Array.isArray(props.comparisonFactors) ? props.comparisonFactors : []) as string[]);
  let visibleCandidates = $derived(candidates.filter((candidate) =>
    `${candidate.title} ${candidate.description ?? ''}`.toLocaleLowerCase().includes(search.toLocaleLowerCase()),
  ));
  let decisionPresentation = $derived(resolveDecisionPresentation(candidates.length, props.complexity === 'complex' ? 'complex' : 'simple'));
  let ontologyBindings = $derived((props.__ontologyBindings as Record<string, string> | undefined) ?? {});
  let valuePage = $derived(ontologyBindings.value ? entityBindingContext?.get(ontologyBindings.value) : undefined);
  let factsPage = $derived(ontologyBindings.facts ? entityBindingContext?.get(ontologyBindings.facts) : undefined);
  let relationshipsPage = $derived(ontologyBindings.relationships ? entityBindingContext?.get(ontologyBindings.relationships) : undefined);
  let detailsPage = $derived(ontologyBindings.details ? entityBindingContext?.get(ontologyBindings.details) : undefined);
  let currentComparisonPage = $derived(ontologyBindings.current ? entityBindingContext?.get(ontologyBindings.current) : undefined);
  let referenceComparisonPage = $derived(ontologyBindings.reference ? entityBindingContext?.get(ontologyBindings.reference) : undefined);
  let comparisonPeriodStartPage = $derived(ontologyBindings.periodStart ? entityBindingContext?.get(ontologyBindings.periodStart) : undefined);
  let comparisonPeriodEndPage = $derived(ontologyBindings.periodEnd ? entityBindingContext?.get(ontologyBindings.periodEnd) : undefined);
  let comparisonPeriodStart = $derived(comparisonPeriodStartPage?.items[0]?.value ?? null);
  let comparisonPeriodEnd = $derived(comparisonPeriodEndPage?.items[0]?.value ?? null);
  let comparisonPeriodClosed = $derived(isComparisonPeriodClosed(comparisonPeriodEnd));
  let currentPeriodLabel = $derived(formatComparisonMonth(comparisonPeriodStart) ?? String(props.currentPeriod));
  let referencePeriodLabel = $derived(previousComparisonMonth(comparisonPeriodStart) ?? String(props.referencePeriod));
  let distributionPage = $derived(ontologyBindings.collection ? entityBindingContext?.get(ontologyBindings.collection) : undefined);
  let informationColumns = $derived(Number((props.layout as { columns?: unknown } | undefined)?.columns ?? 1));
  let informationPrimaryChildSpans = $derived(componentType === 'Information' && shouldSpanPrimaryInformationChild(componentContractVersion));
  let informationContractBase = $derived(componentType === 'Information' && (componentContractVersion === 0 || componentContractVersion === 7));
  let informationSummaryIsVisible = $derived(informationSummaryVisible(componentContractVersion, props.summary, props.title));
  let boundFactLabelIsVisible = $derived(informationFactLabelVisible(componentContractVersion, factsPage?.items.length ?? 0));
  let distributionProjectionBindings = $derived((props.__ontologyProjectionBindings as Record<string, Record<string, string>> | undefined) ?? {});
  let distributionProjection = $derived(distributionProjectionBindings.collection ?? {});
  let recordsElementId = $derived(String(props.__semanticElementId ?? ''));
  let recordsPage = $derived(recordsElementId ? entityBindingContext?.getRecords(recordsElementId) : undefined);
  let recordsColumns = $derived((Array.isArray(props.columns) ? props.columns : []) as Array<{ id: string; label: string; valueType: string; unit?: string; sortable?: boolean; filterOperators?: string[] }>);
  let recordsFilterColumn = $state('');
  let recordsFilterValue = $state('');
  let recordsFilterOperator = $state('contains');
  let relativeNow = $state(Date.now());
  onMount(() => {
    const timer = setInterval(() => relativeNow = Date.now(), 60_000);
    return () => clearInterval(timer);
  });
  let comparison = $derived.by(() => {
    if (!comparisonPeriodClosed) return null;
    const currentItem = currentComparisonPage?.items[0];
    const referenceItem = referenceComparisonPage?.items[0];
    const unitsCompatible = areComparisonUnitsCompatible(
      currentComparisonPage?.unitIri,
      currentComparisonPage?.unitLabel,
      referenceComparisonPage?.unitIri,
      referenceComparisonPage?.unitLabel,
    );
    if (!currentItem || !referenceItem || !canCalculateComparison(comparisonPeriodClosed, currentItem.value, referenceItem.value, unitsCompatible)) return null;
    return calculateComparison(currentItem.value, referenceItem.value, props.favorableDirection === 'higher' || props.favorableDirection === 'lower' ? props.favorableDirection : 'neutral');
  });
  let comparisonHasCompatibleUnit = $derived(areComparisonUnitsCompatible(
    currentComparisonPage?.unitIri,
    currentComparisonPage?.unitLabel,
    referenceComparisonPage?.unitIri,
    referenceComparisonPage?.unitLabel,
  ));
  let distribution = $derived.by(() => distributionPage && distributionProjection.measure && distributionProjection.groupBy
    ? aggregateDistribution(distributionPage.items, distributionProjection.measure, distributionProjection.groupBy)
    : null);
  let distributionCoverage = $derived(resolveDistributionCoverage(Boolean(distributionPage?.hasMore)));
  let distributionShares = $derived.by(() => {
    const total = distribution?.total ?? 0;
    return new Map((distribution?.groups ?? []).map((group) => [`${group.unitLabel ?? ''}\u0000${group.group}`, total > 0 && !distribution?.hasMixedUnits ? (group.amount / total) * 100 : 0]));
  });
  let supportsDefaultSlot = $derived(semanticCatalog.data.components[componentType as keyof typeof semanticCatalog.data.components]?.slots.includes('default') ?? false);
  let hasLabelableValueControl = $derived(
    !valuePage && props.editable !== false && props.cardinality !== 'multiple' &&
    props.valueType !== 'code' && props.valueType !== 'query_config' && props.valueType !== 'recurrence_rule',
  );

  function emitEvent(name: string, event?: Event) {
    const action = on(name);
    if (event && action.shouldPreventDefault) event.preventDefault();
    if (name === 'change') {
      emit(name);
      return;
    }
    if (action.bound) action.emit();
  }

  function isAtomicDisplayValue(item: BoundPropertyItem) {
    return /(?:integer|decimal|double|float|date|datetime|time)$/.test((item.datatype ?? '').toLocaleLowerCase());
  }

  function updateDraftValue(nextValue: string) {
    value = nextValue;
    valueBinding.current = value;
    emitEvent('change');
  }

  function choose(candidateValue: string, event: Event) {
    if (props.selectionMode === 'multiple') {
      selected = selected.includes(candidateValue)
        ? selected.filter((item) => item !== candidateValue)
        : [...selected, candidateValue];
    } else {
      selected = [candidateValue];
    }
    selectedBinding.current = props.selectionMode === 'multiple' ? selected : candidateValue;
    emitEvent('change', event);
  }

  function inputType(type: unknown) {
    if (type === 'email' || type === 'date') return type;
    if (type === 'date_time') return 'datetime-local';
    if (type === 'number' || type === 'currency' || type === 'percentage') return 'number';
    if (type === 'url') return 'url';
    return 'text';
  }

  function updateMultipleValue(index: number, nextValue: string) {
    values = values.map((current, currentIndex) => currentIndex === index ? nextValue : current);
    valueBinding.current = values;
    emitEvent('change');
  }

  function addMultipleValue() {
    values = [...values, ''];
    valueBinding.current = values;
    emitEvent('change');
  }

  function removeMultipleValue(index: number) {
    values = values.filter((_, currentIndex) => currentIndex !== index);
    valueBinding.current = values;
    emitEvent('change');
  }

  function statusIcon(urgency: unknown) {
    if (urgency === 'urgent') return 'priority_high';
    if (urgency === 'attention') return 'warning';
    return 'info';
  }

  function reversibilityLabel(value: unknown) {
    if (value === 'irreversible') return 'Não pode ser desfeita';
    if (value === 'hard_to_reverse') return 'Difícil de reverter';
    return 'Pode ser desfeita';
  }

  function submitRecordsFilter() {
    if (!recordsElementId || !recordsPage || !recordsFilterColumn || !recordsFilterValue.trim()) return;
    entityBindingContext?.loadRecords(recordsElementId, {
      filters: [{ columnId: recordsFilterColumn, operator: recordsFilterOperator as SemanticRecordsCriteria['filters'][number]['operator'], value: recordsFilterValue.trim() }],
      sort: recordsPage.criteria.sort,
    });
  }

  function clearRecordsFilter() {
    if (!recordsElementId || !recordsPage) return;
    recordsFilterValue = '';
    entityBindingContext?.loadRecords(recordsElementId, { filters: [], sort: recordsPage.criteria.sort });
  }

  function sortRecords(columnId: string) {
    if (!recordsElementId || !recordsPage) return;
    const direction = recordsPage.criteria.sort.columnId === columnId && recordsPage.criteria.sort.direction === 'asc' ? 'desc' : 'asc';
    entityBindingContext?.loadRecords(recordsElementId, { ...recordsPage.criteria, sort: { columnId, direction } });
  }

  function recordCell(row: SemanticRecordsRow, column: typeof recordsColumns[number]) {
    const item = row.values[column.id]?.[0];
    if (!item) return '—';
    if (item.isObjectProperty) return item.valueLabel ?? item.value;
    if (column.valueType === 'currency' || column.valueType === 'number' || column.valueType === 'percentage') {
      const number = Number(item.value);
      if (Number.isFinite(number)) {
        if (column.valueType === 'currency' && column.unit && /^[A-Z]{3}$/.test(column.unit)) {
          return new Intl.NumberFormat(Intl.DateTimeFormat().resolvedOptions().locale, { style: 'currency', currency: column.unit }).format(number);
        }
        const formatted = new Intl.NumberFormat(Intl.DateTimeFormat().resolvedOptions().locale, { maximumFractionDigits: 2 }).format(number);
        return column.valueType === 'percentage' ? `${formatted}%` : `${formatted}${column.unit ? ` ${column.unit}` : ''}`;
      }
    }
    return item.valueLabel ?? item.value;
  }

  function dateValue(value: string, valueType: 'date' | 'date_time') {
    const datePrefix = /^(\d{4}-\d{2}-\d{2})/.exec(value)?.[1];
    const isUtcCalendarBoundary = valueType === 'date_time' && /T(?:00:00(?::00(?:\.0+)?)?|23:59:59\.999)(?:Z|\+00:00)$/.test(value);
    const isCalendarDate = valueType === 'date' || isUtcCalendarBoundary;
    // Some ontology date-only values are stored as UTC day boundaries. Preserve their calendar day
    // instead of shifting them to the previous local day (for example, midnight UTC in São Paulo).
    const date = isCalendarDate && datePrefix ? new Date(`${datePrefix}T12:00:00`) : new Date(value);
    if (Number.isNaN(date.getTime())) return null;
    const calendarDate = `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
    return {
      absolute: isCalendarDate ? formatLocalizedDate(date, { dateStyle: 'medium' }) : formatLocalizedDateTime(date),
      relative: isCalendarDate ? formatRelativeCalendarDate(date, relativeNow) : formatRelativeTime(date, relativeNow),
      dateTime: isCalendarDate ? calendarDate : date.toISOString(),
    };
  }
</script>

{#snippet boundText(item: BoundPropertyItem)}
  {#if !item.isObjectProperty && item.datatype === 'xsd:dateTime'}
    {@const date = dateValue(item.value, 'date_time')}
    {#if date}<span class="date-value-display"><time class="date-value-absolute" datetime={date.dateTime}>{date.absolute}</time><span class="date-value-relative semantic-metadata">{date.relative}</span></span>{:else}{item.value}{/if}
  {:else if !item.isObjectProperty && item.datatype === 'xsd:date'}
    {@const date = dateValue(item.value, 'date')}
    {#if date}<span class="date-value-display"><time class="date-value-absolute" datetime={item.value}>{date.absolute}</time><span class="date-value-relative semantic-metadata">{date.relative}</span></span>{:else}{item.value}{/if}
  {:else}<span class:atomic-value={isAtomicDisplayValue(item)}>{item.valueLabel ?? item.value}</span>{/if}
{/snippet}

{#if componentType === 'Decision'}
  <section class="semantic-block" aria-labelledby={semanticDomIds.decisionTitle}>
    <header class="semantic-heading">
      <div>
        {#if props.domain}<p class="eyebrow">{props.domain}</p>{/if}
        <h2 id={semanticDomIds.decisionTitle}>{props.title}</h2>
        {#if props.description}<p class="supporting-text">{props.description}</p>{/if}
      </div>
      <span class="semantic-icon material-symbols-outlined" aria-hidden="true">{props.selectionMode === 'multiple' ? 'checklist' : 'fact_check'}</span>
    </header>
    {#if decisionPresentation.searchable}
      <label class="visually-hidden" for={semanticDomIds.decisionSearch}>Buscar alternativas</label>
      <input id={semanticDomIds.decisionSearch} class="semantic-input" type="search" bind:value={search} placeholder="Buscar alternativas" />
    {/if}
    <div class:comparison-list={decisionPresentation.searchable} class="candidate-list" role={props.selectionMode === 'single' ? 'radiogroup' : 'group'} aria-label={String(props.title)}>
      {#each visibleCandidates as candidate (candidate.value)}
        <label
          class="candidate"
          class:selected={selected.includes(candidate.value)}
        >
          <input
            class="selection-control"
            type={props.selectionMode === 'multiple' ? 'checkbox' : 'radio'}
            name={String(props.groupName ?? props.domain ?? props.title)}
            value={candidate.value}
            checked={selected.includes(candidate.value)}
            disabled={Boolean(candidate.disabled)}
            onchange={(event) => choose(candidate.value, event)}
          />
          <span class="candidate-selection material-symbols-outlined" aria-hidden="true">
            {selected.includes(candidate.value) ? (props.selectionMode === 'multiple' ? 'check_box' : 'radio_button_checked') : (props.selectionMode === 'multiple' ? 'check_box_outline_blank' : 'radio_button_unchecked')}
          </span>
          <span class="candidate-copy">
            <span class="candidate-title">{candidate.title}</span>
            {#if candidate.description}<span class="supporting-text">{candidate.description}</span>{/if}
            {#if decisionPresentation.comparison && comparisonFactors.length}
              <span class="comparison-values">
                {#each comparisonFactors as factor (factor)}
                  {#if candidate.comparison?.[factor]}
                    <span><span class="comparison-label">{factor}</span>{candidate.comparison[factor]}</span>
                  {/if}
                {/each}
              </span>
            {/if}
          </span>
        </label>
      {:else}
        <p class="supporting-text">Nenhuma alternativa corresponde à busca.</p>
      {/each}
    </div>
    {#if selected.length}
      <p class="selection-result" role="status" aria-live="polite">Selecionado: {selected.map((item) => candidates.find((candidate) => candidate.value === item)?.title ?? item).join(', ')}</p>
    {/if}
    {#if props.consequence}<p class="context-note"><span class="material-symbols-outlined" aria-hidden="true">swap_horiz</span>{props.consequence}</p>{/if}
    {#if props.reversibility}<p class="supporting-text">{reversibilityLabel(props.reversibility)}</p>{/if}
  </section>
{:else if componentType === 'ValueEntry'}
  <section class="semantic-block value-entry">
    <label class="field-label" class:visually-hidden={props.labelVisible === false} for={hasLabelableValueControl ? semanticDomIds.value : undefined} id={semanticDomIds.valueLabel}>{props.label}{#if props.required}<span aria-hidden="true"> ·</span>{/if}</label>
    {#if props.description}<p class="supporting-text" id={semanticDomIds.valueDescription}>{props.description}</p>{/if}
    {#if valuePage}
      {#if valuePage.loading && !valuePage.items.length}
        <p class="supporting-text" role="status" aria-live="polite">Carregando {valuePage.propertyLabel}…</p>
      {:else if valuePage.error && !valuePage.items.length}
        <div class="binding-error" role="alert"><span>{valuePage.error}</span><Button variant="outline" size="sm" onclick={() => entityBindingContext?.retry(valuePage.propertyIri)}>Tentar novamente</Button></div>
      {:else if valuePage.items.length}
        <div class="bound-value-list">
          {#each valuePage.items as item, index (`${item.value}:${index}`)}
            {#if item.isObjectProperty && item.valueEntityKind}
              <OntologyEntityReferenceChip kind={item.valueEntityKind} label={item.valueLabel ?? item.value} icon={item.valueIcon} entityIri={item.value} onopen={(iri) => entityBindingContext?.openEntity(iri)} />
            {:else}
              <output class="read-only-value">{@render boundText(item)}{#if props.unit}<span>{props.unit}</span>{/if}</output>
            {/if}
          {/each}
        </div>
        {#if valuePage.error}<p class="binding-error" role="alert">{valuePage.error}</p>{/if}
        {#if valuePage.hasMore}<Button variant="outline" size="sm" disabled={valuePage.loadingMore} onclick={() => entityBindingContext?.loadMore(valuePage.propertyIri)}><span class="material-symbols-outlined" aria-hidden="true">{valuePage.loadingMore ? 'progress_activity' : 'expand_more'}</span>{valuePage.loadingMore ? 'Carregando…' : 'Mostrar mais'}</Button>{/if}
      {:else}
        <p class="supporting-text">Sem valor registrado.</p>
      {/if}
    {:else if props.editable === false}
      <output class="read-only-value">
        {#if props.valueType === 'date' || props.valueType === 'date_time'}
          {@const date = dateValue(value, props.valueType)}
          {#if date}<span class="date-value-display"><time class="date-value-absolute" datetime={date.dateTime}>{date.absolute}</time><span class="date-value-relative semantic-metadata">{date.relative}</span></span>{:else}{value}{/if}
        {:else}{value}{/if}
        {#if props.unit}<span>{props.unit}</span>{/if}
      </output>
    {:else if props.cardinality === 'multiple'}
      <div class="multiple-value-list">
        {#each values as item, index (index)}
          <div class="multiple-value-row">
            {#if props.valueType === 'code'}
              <CodeEditor value={item} language={String(props.language ?? '')} fieldLabel={`${props.label}, valor ${index + 1}`} oninput={(event) => updateMultipleValue(index, event.currentTarget.value)} />
            {:else if props.valueType === 'markdown' || props.valueType === 'html' || props.valueType === 'query_config' || props.valueType === 'recurrence_rule'}
              <textarea
                class="semantic-input semantic-multiline"
                aria-label={`${props.label}, valor ${index + 1}`}
                value={item}
                required={Boolean(props.required)}
                aria-invalid={props.valid === false}
                oninput={(event) => updateMultipleValue(index, event.currentTarget.value)}
              ></textarea>
            {:else if props.valueType === 'boolean'}
              <select class="semantic-input" aria-label={`${props.label}, valor ${index + 1}`} value={item} onchange={(event) => updateMultipleValue(index, event.currentTarget.value)}>
                <option value="true">Sim</option>
                <option value="false">Não</option>
              </select>
            {:else}
              <input
                class="semantic-input"
                type={inputType(props.valueType)}
                aria-label={`${props.label}, valor ${index + 1}`}
                value={item}
                required={Boolean(props.required)}
                aria-invalid={props.valid === false}
                oninput={(event) => updateMultipleValue(index, event.currentTarget.value)}
              />
            {/if}
            <Button variant="ghost" size="icon-sm" aria-label={`Remover valor ${index + 1} de ${props.label}`} disabled={Boolean(props.required) && values.length <= 1} onclick={() => removeMultipleValue(index)}>
              <span class="material-symbols-outlined" aria-hidden="true">remove</span>
            </Button>
          </div>
        {/each}
        <Button variant="outline" size="sm" onclick={addMultipleValue}>
          <span class="material-symbols-outlined" aria-hidden="true">add</span>
          Adicionar valor
        </Button>
      </div>
    {:else if props.valueType === 'code'}
      <CodeEditor value={value} language={String(props.language ?? '')} fieldLabel={String(props.label)} oninput={(event) => { value = event.currentTarget.value; valueBinding.current = value; emitEvent('change', event); }} />
    {:else if props.valueType === 'query_config'}
      <QueryConfigEditor
        value={value}
        draftOnly={true}
        onDraftChange={updateDraftValue}
        onconfirm={updateDraftValue}
        oncancel={() => {}}
      />
    {:else if props.valueType === 'recurrence_rule'}
      <RecurrenceEditor
        value={value}
        draftOnly={true}
        onDraftChange={updateDraftValue}
        onconfirm={updateDraftValue}
        oncancel={() => {}}
      />
    {:else if props.valueType === 'boolean'}
      <select
        id={semanticDomIds.value}
        class="semantic-input"
        value={value}
        aria-labelledby={semanticDomIds.valueLabel}
        aria-describedby={props.description ? semanticDomIds.valueDescription : undefined}
        aria-invalid={props.valid === false}
        oninput={(event) => { value = event.currentTarget.value; valueBinding.current = value; emitEvent('change', event); }}
      >
        <option value="true">Sim</option>
        <option value="false">Não</option>
      </select>
    {:else if props.valueType === 'markdown' || props.valueType === 'html'}
      <textarea
        id={semanticDomIds.value}
        class="semantic-input semantic-multiline"
        value={value}
        aria-labelledby={semanticDomIds.valueLabel}
        aria-describedby={props.description ? semanticDomIds.valueDescription : undefined}
        placeholder={String(props.placeholder ?? '')}
        required={Boolean(props.required)}
        aria-invalid={props.valid === false}
        oninput={(event) => { value = event.currentTarget.value; valueBinding.current = value; emitEvent('change', event); }}
      ></textarea>
    {:else}
      <div class="value-control">
        <input
          id={semanticDomIds.value}
          class="semantic-input"
          type={inputType(props.valueType)}
          value={value}
          aria-labelledby={semanticDomIds.valueLabel}
          aria-describedby={props.description ? semanticDomIds.valueDescription : undefined}
          placeholder={String(props.placeholder ?? '')}
          required={Boolean(props.required)}
          aria-invalid={props.valid === false}
          oninput={(event) => { value = event.currentTarget.value; valueBinding.current = value; emitEvent('change', event); }}
        />
        {#if props.unit}<span class="unit-label">{props.unit}</span>{/if}
        {#if props.sensitive}<span class="material-symbols-outlined sensitivity" aria-label="Valor sensível">lock</span>{/if}
      </div>
    {/if}
    {#if props.validationMessage}<p class:invalid={props.valid === false} class="validation-message" role={props.valid === false ? 'alert' : 'status'}>{props.validationMessage}</p>{/if}
  </section>
{:else if componentType === 'Action'}
  <section class="semantic-block action-block">
    <div class="action-copy">
      <h2>{props.label}</h2>
      {#if props.objective}<p class="supporting-text">{props.objective}</p>{/if}
      {#if props.impact}<p class="context-note"><span class="material-symbols-outlined" aria-hidden="true">warning</span>{props.impact}</p>{/if}
      {#if props.progress}<p class="action-status" role="status"><span class="material-symbols-outlined" aria-hidden="true">progress_activity</span>{props.progress}</p>{/if}
      {#if props.result}<p class="action-status" role="status"><span class="material-symbols-outlined" aria-hidden="true">task_alt</span>{props.result}</p>{/if}
    </div>
    <Button
      variant={props.priority === 'destructive' ? 'destructive' : props.priority === 'supporting' ? 'ghost' : 'default'}
      disabled={props.available === false}
      onclick={(event) => emitEvent('press', event)}
    >
      <span class="material-symbols-outlined action-icon" aria-hidden="true">{props.icon}</span>
      {props.label}
    </Button>
  </section>
{:else if componentType === 'Information'}
  <section class="semantic-block information-block" class:first-child-spans={informationPrimaryChildSpans} class:contract-base={informationContractBase} aria-labelledby={semanticDomIds.informationTitle}>
    <header class="semantic-heading">
      <div>
        {#if props.role}<p class="eyebrow">{props.role}</p>{/if}
        <div class="information-title-row">
          <h2 id={semanticDomIds.informationTitle}>{props.title}</h2>
          {#if informationSummaryIsVisible}
            <Tooltip.Provider delayDuration={200}>
              <Tooltip.Root open={contextTooltipOpen} onOpenChange={(open) => contextTooltipOpen = open} disableCloseOnTriggerClick>
                <Tooltip.Trigger
                  class="context-tooltip-trigger"
                  aria-label="Mais informações sobre {props.title}"
                  type="button"
                  onclick={() => contextTooltipOpen = !contextTooltipOpen}
                  onkeydown={(event) => {
                    if (event.key === 'Escape') {
                      contextTooltipOpen = false;
                      event.preventDefault();
                    }
                  }}
                >
                  <span class="material-symbols-outlined" aria-hidden="true">info</span>
                </Tooltip.Trigger>
                <Tooltip.Content>{props.summary}</Tooltip.Content>
              </Tooltip.Root>
            </Tooltip.Provider>
          {/if}
        </div>
      </div>
      {#if props.status}<span class="status-indicator"><span aria-hidden="true" class="material-symbols-outlined">{statusIcon(props.urgency)}</span>{props.status}</span>{/if}
    </header>
    {#if !ontologyBindings.facts && Array.isArray(props.facts) && props.facts.length}
      <dl class="fact-list">
        {#each props.facts as fact (fact.label)}
          <div class="fact fact-labeled"><dt>{fact.label}</dt><dd>{fact.value}{#if fact.unit}<span>{fact.unit}</span>{/if}</dd></div>
        {/each}
      </dl>
    {:else if ontologyBindings.facts && factsPage}
      {#if factsPage.loading && !factsPage.items.length}<p class="supporting-text" role="status">Carregando {factsPage.propertyLabel}…</p>
      {:else if factsPage.error && !factsPage.items.length}<div class="binding-error" role="alert"><span>{factsPage.error}</span><Button variant="outline" size="sm" onclick={() => entityBindingContext?.retry(factsPage.propertyIri)}>Tentar novamente</Button></div>
      {:else if factsPage.items.length}
        <dl class="fact-list">
          {#each factsPage.items as item, index (`${item.value}:${index}`)}
            <div class="fact" class:fact-labeled={boundFactLabelIsVisible}>{#if boundFactLabelIsVisible}<dt>{factsPage.propertyLabel}</dt>{/if}<dd>
              {#if item.isObjectProperty && item.valueEntityKind}<OntologyEntityReferenceChip kind={item.valueEntityKind} label={item.valueLabel ?? item.value} icon={item.valueIcon} entityIri={item.value} onopen={(iri) => entityBindingContext?.openEntity(iri)} />{:else}{@render boundText(item)}{/if}
            </dd></div>
          {/each}
        </dl>
        {#if factsPage.error}<p class="binding-error" role="alert">{factsPage.error}</p>{/if}
        {#if factsPage.hasMore}<Button variant="outline" size="sm" disabled={factsPage.loadingMore} onclick={() => entityBindingContext?.loadMore(factsPage.propertyIri)}><span class="material-symbols-outlined" aria-hidden="true">expand_more</span>{factsPage.loadingMore ? 'Carregando…' : 'Mostrar mais'}</Button>{/if}
      {:else}<p class="supporting-text">Sem valor registrado.</p>{/if}
    {/if}
    {#if !ontologyBindings.relationships && Array.isArray(props.relationships) && props.relationships.length}
      <dl class="relationship-list">
        {#each props.relationships as relationship (relationship.label)}
          <div><dt>{relationship.label}</dt><dd>{relationship.value}</dd></div>
        {/each}
      </dl>
    {:else if ontologyBindings.relationships && relationshipsPage}
      {#if relationshipsPage.loading && !relationshipsPage.items.length}<p class="supporting-text" role="status">Carregando {relationshipsPage.propertyLabel}…</p>
      {:else if relationshipsPage.error && !relationshipsPage.items.length}<div class="binding-error" role="alert"><span>{relationshipsPage.error}</span><Button variant="outline" size="sm" onclick={() => entityBindingContext?.retry(relationshipsPage.propertyIri)}>Tentar novamente</Button></div>
      {:else if relationshipsPage.items.length}<div class="bound-value-list">{#each relationshipsPage.items as item, index (`${item.value}:${index}`)}{#if item.isObjectProperty && item.valueEntityKind}<OntologyEntityReferenceChip kind={item.valueEntityKind} label={item.valueLabel ?? item.value} icon={item.valueIcon} entityIri={item.value} onopen={(iri) => entityBindingContext?.openEntity(iri)} />{:else}{@render boundText(item)}{/if}{/each}</div>{#if relationshipsPage.error}<p class="binding-error" role="alert">{relationshipsPage.error}</p>{/if}{#if relationshipsPage.hasMore}<Button variant="outline" size="sm" disabled={relationshipsPage.loadingMore} onclick={() => entityBindingContext?.loadMore(relationshipsPage.propertyIri)}><span class="material-symbols-outlined" aria-hidden="true">expand_more</span>{relationshipsPage.loadingMore ? 'Carregando…' : 'Mostrar mais'}</Button>{/if}
      {:else}<p class="supporting-text">Sem valor registrado.</p>{/if}
    {/if}
    {#if supportsDefaultSlot && children}
    <div class="semantic-children" data-layout-columns={informationColumns} style={`--information-columns: ${informationColumns}`}>
      {@render children()}
    </div>
    {/if}
  </section>
{:else if componentType === 'Details'}
  <details class="semantic-block detail-block" open={Boolean(props.initiallyExpanded)}>
    <summary>
      <span><span class="material-symbols-outlined" aria-hidden="true">unfold_more</span><span class="detail-title">{props.title}</span>{#if props.summary}<span class="supporting-text">{props.summary}</span>{/if}</span>
      <span class="material-symbols-outlined disclosure-icon" aria-hidden="true">expand_more</span>
    </summary>
    {#if props.relevance}<p class="supporting-text">{props.relevance}</p>{/if}
    <dl class="fact-list detail-list">
      {#if ontologyBindings.details && detailsPage}
        {#if detailsPage.loading && !detailsPage.items.length}<p class="supporting-text" role="status">Carregando {detailsPage.propertyLabel}…</p>
        {:else if detailsPage.error && !detailsPage.items.length}<div class="binding-error" role="alert"><span>{detailsPage.error}</span><Button variant="outline" size="sm" onclick={() => entityBindingContext?.retry(detailsPage.propertyIri)}>Tentar novamente</Button></div>
        {:else if detailsPage.items.length}
          {#each detailsPage.items as item, index (`${item.value}:${index}`)}
            <div class="fact"><dt>{detailsPage.propertyLabel}</dt><dd>
              {#if item.isObjectProperty && item.valueEntityKind}<OntologyEntityReferenceChip kind={item.valueEntityKind} label={item.valueLabel ?? item.value} icon={item.valueIcon} entityIri={item.value} onopen={(iri) => entityBindingContext?.openEntity(iri)} />{:else}{@render boundText(item)}{/if}
            </dd></div>
          {/each}
          {#if detailsPage.error}<p class="binding-error" role="alert">{detailsPage.error}</p>{/if}
          {#if detailsPage.hasMore}<Button variant="outline" size="sm" disabled={detailsPage.loadingMore} onclick={() => entityBindingContext?.loadMore(detailsPage.propertyIri)}><span class="material-symbols-outlined" aria-hidden="true">expand_more</span>{detailsPage.loadingMore ? 'Carregando…' : 'Mostrar mais'}</Button>{/if}
        {:else}<p class="supporting-text">Sem valor registrado.</p>{/if}
      {:else}
      {#each (props.details as Detail[] | undefined) ?? [] as detail (detail.label)}
        <div class="fact"><dt>{detail.label}</dt><dd>{detail.value}</dd></div>
      {/each}
      {/if}
    </dl>
  </details>
{:else if componentType === 'Comparison'}
  <section class="semantic-block comparison-block" aria-labelledby={semanticDomIds.comparisonTitle}>
    <header class="comparison-heading">
      <div><p class="eyebrow">{props.metric}</p><h2 id={semanticDomIds.comparisonTitle}>{props.title}</h2></div>
      <span class="material-symbols-outlined semantic-icon" aria-hidden="true">compare_arrows</span>
    </header>
    {#if (currentComparisonPage?.loading && !currentComparisonPage.items.length) || (referenceComparisonPage?.loading && !referenceComparisonPage.items.length) || (comparisonPeriodStartPage?.loading && !comparisonPeriodStartPage.items.length) || (comparisonPeriodEndPage?.loading && !comparisonPeriodEndPage.items.length)}
      <p class="supporting-text" role="status" aria-live="polite">Carregando os períodos para comparação…</p>
    {:else if currentComparisonPage?.error || referenceComparisonPage?.error || comparisonPeriodStartPage?.error || comparisonPeriodEndPage?.error}
      <div class="binding-error" role="alert">
        <span>{currentComparisonPage?.error || referenceComparisonPage?.error || comparisonPeriodStartPage?.error || comparisonPeriodEndPage?.error}</span>
        {#if currentComparisonPage}<Button variant="outline" size="sm" onclick={() => entityBindingContext?.retry(currentComparisonPage.propertyIri)}>Tentar novamente</Button>{/if}
        {#if referenceComparisonPage}<Button variant="outline" size="sm" onclick={() => entityBindingContext?.retry(referenceComparisonPage.propertyIri)}>Tentar novamente</Button>{/if}
        {#if comparisonPeriodStartPage}<Button variant="outline" size="sm" onclick={() => entityBindingContext?.retry(comparisonPeriodStartPage.propertyIri)}>Tentar novamente</Button>{/if}
        {#if comparisonPeriodEndPage}<Button variant="outline" size="sm" onclick={() => entityBindingContext?.retry(comparisonPeriodEndPage.propertyIri)}>Tentar novamente</Button>{/if}
      </div>
    {:else if currentComparisonPage?.items.length && referenceComparisonPage?.items.length && comparisonPeriodClosed && comparisonHasCompatibleUnit && comparison}
      {@const favorable = comparison.interpretation === 'favorable'}
      {@const unfavorable = comparison.interpretation === 'unfavorable'}
      <div class="comparison-values" aria-label={`${props.metric}: ${comparison.interpretation === 'neutral' ? 'variação neutra' : favorable ? 'variação favorável' : 'variação desfavorável'}`}>
        <div class="comparison-period-value comparison-current">
          <span class="semantic-metadata comparison-period">{currentPeriodLabel}</span>
          <strong class="comparison-value">{formatSemanticAmount(comparison.current, currentComparisonPage.unitLabel)}</strong>
        </div>
        <div class="comparison-period-value comparison-reference">
          <span class="semantic-metadata comparison-period">{referencePeriodLabel}</span>
          <span class="comparison-value">{formatSemanticAmount(comparison.reference, referenceComparisonPage.unitLabel)}</span>
        </div>
      </div>
      <p class="comparison-change" class:favorable class:unfavorable role="status" aria-live="polite"
        aria-label={`${comparison.interpretation === 'neutral' ? 'Sem variação' : favorable ? 'Variação favorável' : 'Variação desfavorável'}: ${formatSemanticAmount(comparison.delta, currentComparisonPage.unitLabel)}${formatSemanticPercent(comparison.percentChange) ? `, ${formatSemanticPercent(comparison.percentChange)}` : ''}`}>
        <span class="material-symbols-outlined" aria-hidden="true">{comparison.direction === 'higher' ? 'trending_up' : comparison.direction === 'lower' ? 'trending_down' : 'trending_flat'}</span>
        <span>{formatSemanticAmount(comparison.delta, currentComparisonPage.unitLabel)}</span>
        {#if formatSemanticPercent(comparison.percentChange)}
          <span class="comparison-change-divider" aria-hidden="true"></span>
          <span>{formatSemanticPercent(comparison.percentChange)}</span>
        {/if}
      </p>
    {:else if currentComparisonPage?.items.length && !comparisonPeriodClosed}
      <p class="supporting-text" role="status">{comparisonPeriodClosed
        ? props.unavailableBaselineMessage ?? 'Ainda não há um período de referência comparável.'
        : 'O período ainda está aberto. A referência equivalente do mês anterior não está disponível, então exibimos somente o valor parcial atual.'}</p>
      <div class="comparison-current comparison-current-only"><span class="semantic-metadata comparison-period">{currentPeriodLabel}</span><strong class="comparison-value">{formatSemanticAmount(Number(currentComparisonPage.items[0].value), currentComparisonPage.unitLabel)}</strong></div>
    {:else if currentComparisonPage?.items.length && referenceComparisonPage?.items.length && !comparisonHasCompatibleUnit}
      <p class="supporting-text" role="status">Os períodos usam unidades incompatíveis; não é possível comparar os valores.</p>
    {:else if currentComparisonPage?.items.length && referenceComparisonPage?.items.length && Number(referenceComparisonPage.items[0].value) === 0}
      <p class="supporting-text" role="status">A referência é zero; não há base válida para calcular uma variação ou percentual.</p>
      <div class="comparison-current comparison-current-only"><span class="semantic-metadata comparison-period">{currentPeriodLabel}</span><strong class="comparison-value">{formatSemanticAmount(Number(currentComparisonPage.items[0].value), currentComparisonPage.unitLabel)}</strong></div>
    {:else if currentComparisonPage?.items.length}
      <p class="supporting-text" role="status">{props.unavailableBaselineMessage ?? 'Ainda não há um período de referência comparável.'}</p>
      <div class="comparison-current comparison-current-only"><span class="semantic-metadata comparison-period">{currentPeriodLabel}</span><strong class="comparison-value">{formatSemanticAmount(Number(currentComparisonPage.items[0].value), currentComparisonPage.unitLabel)}</strong></div>
    {:else}
      <p class="supporting-text">Não há dados suficientes para esta comparação.</p>
    {/if}
  </section>
{:else if componentType === 'Distribution'}
  <section class="semantic-block distribution-block" aria-labelledby={semanticDomIds.distributionTitle}>
    <header class="comparison-heading">
      <div><h2 id={semanticDomIds.distributionTitle}>{props.title}</h2></div>
      {#if props.description}
        <Tooltip.Provider delayDuration={200}>
          <Tooltip.Root open={contextTooltipOpen} onOpenChange={(open) => contextTooltipOpen = open} disableCloseOnTriggerClick>
            <Tooltip.Trigger
              class="context-tooltip-trigger"
              aria-label="Mais informações sobre {props.title}"
              type="button"
              onclick={() => contextTooltipOpen = !contextTooltipOpen}
              onkeydown={(event) => {
                if (event.key === 'Escape') {
                  contextTooltipOpen = false;
                  event.preventDefault();
                }
              }}
            >
              <span class="material-symbols-outlined" aria-hidden="true">info</span>
            </Tooltip.Trigger>
            <Tooltip.Content>{props.description}</Tooltip.Content>
          </Tooltip.Root>
        </Tooltip.Provider>
      {/if}
      <span class="material-symbols-outlined semantic-icon" aria-hidden="true">donut_small</span>
    </header>
    {#if distributionPage?.loading && !distributionPage.items.length}
      <p class="supporting-text" role="status" aria-live="polite">Carregando itens…</p>
    {:else if distributionPage?.error && !distributionPage.items.length}
      <div class="binding-error" role="alert"><span>{distributionPage.error}</span><Button variant="outline" size="sm" onclick={() => entityBindingContext?.retry(distributionPage.propertyIri)}>Tentar novamente</Button></div>
    {:else if distributionPage && distribution && distributionPage.items.length}
      {#if distributionPage.error}<div class="binding-error" role="alert"><span>{distributionPage.error}</span><Button variant="outline" size="sm" onclick={() => entityBindingContext?.retry(distributionPage.propertyIri)}>Tentar novamente</Button></div>{/if}
      {#if distribution.hasMixedUnits}<p class="supporting-text" role="status">Os itens usam unidades diferentes; os grupos permanecem separados e não são somados entre si.</p>{/if}
      <ul class="distribution-groups" aria-label="Itens agrupados">
        {#each distribution.groups as group (`${group.unitLabel ?? ''}:${group.group}`)}
          {@const share = distributionShares.get(`${group.unitLabel ?? ''}\u0000${group.group}`) ?? 0}
          <li class="distribution-row">
            <div class="distribution-row-heading"><span>{group.group}</span><strong>{formatSemanticAmount(group.amount, group.unitLabel)}</strong></div>
            {#if !distribution.hasMixedUnits}<div class="distribution-track" aria-hidden="true"><span style={`--distribution-share: ${share}%`}></span></div>{/if}
            <span class="semantic-metadata distribution-share">{group.count} {group.count === 1 ? 'item' : 'itens'}{#if !distribution.hasMixedUnits} · {new Intl.NumberFormat(Intl.DateTimeFormat().resolvedOptions().locale, { maximumFractionDigits: 1 }).format(share)}% do total{/if}</span>
          </li>
        {:else}
          <li class="supporting-text">{props.emptyMessage ?? 'Nenhum agrupamento disponível.'}</li>
        {/each}
      </ul>
      <div class="distribution-total">
        <span>{distributionCoverage.totalLabel}</span>
        {#if !distribution.hasMixedUnits}<strong>{formatSemanticAmount(distribution.total, distribution.unitLabel)}</strong>{:else}<strong>Totais separados por unidade</strong>{/if}
      </div>
      <p class="semantic-metadata distribution-coverage" role="status" aria-live="polite">
        {distribution.pricedCount} itens com medida, de {distributionPage.items.length} carregados
        {#if distribution.unpricedCount}, {distribution.unpricedCount} sem medida numérica legível{/if}
        {#if distributionPage.hasMore}; há mais itens para carregar{/if}
      </p>
      {#if distributionPage.hasMore}<Button variant="outline" size="sm" disabled={distributionPage.loadingMore} onclick={() => entityBindingContext?.loadMore(distributionPage.propertyIri)}><span class="material-symbols-outlined" aria-hidden="true">{distributionPage.loadingMore ? 'progress_activity' : 'expand_more'}</span>{distributionPage.loadingMore ? 'Carregando…' : 'Carregar mais'}</Button>{/if}
    {:else if distributionPage && !distributionPage.items.length}
      <p class="supporting-text">{props.emptyMessage ?? 'A coleção não contém itens.'}</p>
    {:else}
      <p class="supporting-text">A coleção não está disponível.</p>
    {/if}
  </section>
{:else if componentType === 'Records'}
  <section class="semantic-block records-block" aria-labelledby={semanticDomIds.recordsTitle} aria-busy={recordsPage?.loading ?? true}>
    <header class="comparison-heading">
      <div><h2 id={semanticDomIds.recordsTitle}>{props.title}</h2></div>
      <span class="material-symbols-outlined semantic-icon" aria-hidden="true">table_view</span>
    </header>
    {#if recordsPage}
      <form class="records-filters" onsubmit={(event) => { event.preventDefault(); submitRecordsFilter(); }}>
        <label class="visually-hidden" for={semanticDomIds.recordsFilterColumn}>Campo para filtrar</label>
        <select id={semanticDomIds.recordsFilterColumn} class="semantic-input" bind:value={recordsFilterColumn} onchange={(event) => {
          recordsFilterColumn = event.currentTarget.value;
          recordsFilterOperator = recordsColumns.find((column) => column.id === recordsFilterColumn)?.filterOperators?.[0] ?? 'equals';
        }}>
          <option value="">Filtrar por campo</option>
          {#each recordsColumns.filter((column) => column.filterOperators?.length) as column (column.id)}
            <option value={column.id}>{column.label}</option>
          {/each}
        </select>
        <label class="visually-hidden" for={semanticDomIds.recordsFilterValue}>Valor do filtro</label>
        <input id={semanticDomIds.recordsFilterValue} class="semantic-input" type={recordsColumns.find((column) => column.id === recordsFilterColumn)?.valueType === 'date' ? 'date' : ['number', 'currency', 'percentage'].includes(recordsColumns.find((column) => column.id === recordsFilterColumn)?.valueType ?? '') ? 'number' : 'search'} bind:value={recordsFilterValue} placeholder="Valor para filtrar" />
        {#if recordsFilterColumn}
          <label class="visually-hidden" for={semanticDomIds.recordsFilterOperator}>Tipo de filtro</label>
          <select id={semanticDomIds.recordsFilterOperator} class="semantic-input" bind:value={recordsFilterOperator}>
            {#each recordsColumns.find((column) => column.id === recordsFilterColumn)?.filterOperators ?? [] as operator (operator)}
              <option value={operator}>{operator === 'contains' ? 'Contém' : operator === 'equals' ? 'Igual a' : operator}</option>
            {/each}
          </select>
        {/if}
        <Button type="submit" variant="outline" size="sm" disabled={!recordsFilterColumn || !recordsFilterValue.trim()}><span class="material-symbols-outlined" aria-hidden="true">search</span>Filtrar</Button>
        {#if recordsPage.criteria.filters.length}<Button type="button" variant="ghost" size="sm" onclick={clearRecordsFilter}><span class="material-symbols-outlined" aria-hidden="true">filter_alt_off</span>Limpar</Button>{/if}
      </form>
      {#if recordsPage.loading && !recordsPage.rows.length}
        <p class="supporting-text" role="status">Carregando registros…</p>
      {:else if recordsPage.error && !recordsPage.rows.length}
        <div class="binding-error" role="alert"><span>{recordsPage.error}</span><Button variant="outline" size="sm" onclick={() => entityBindingContext?.retryRecords(recordsElementId)}>Tentar novamente</Button></div>
      {:else if recordsPage.rows.length}
        <div class="records-table-scroll">
          <table class="records-table">
            <thead><tr>
              {#each recordsColumns as column (column.id)}
                <th scope="col" data-column-type={column.valueType}>
                  {#if column.sortable}<button type="button" class="records-sort" aria-label="Ordenar por {column.label}" onclick={() => sortRecords(column.id)}>{column.label}<span class="material-symbols-outlined" aria-hidden="true">{recordsPage.criteria.sort.columnId === column.id ? (recordsPage.criteria.sort.direction === 'asc' ? 'arrow_upward' : 'arrow_downward') : 'unfold_more'}</span></button>
                  {:else}{column.label}{/if}
                </th>
              {/each}
              {#if recordsPage.sources.length > 1}<th scope="col">Tipo</th>{/if}
              <th scope="col"><span class="visually-hidden">Abrir registro</span></th>
            </tr></thead>
            <tbody>
              {#each recordsPage.rows as row (`${row.sourceAlias}:${row.entityIri}`)}
                <tr>
                  {#each recordsColumns as column (column.id)}
                    {@const cellItem = row.values[column.id]?.[0]}
                    <td data-column-type={column.valueType}>
                      {#if cellItem?.isObjectProperty}
                        <OntologyEntityReferenceChip kind={cellItem.valueEntityKind ?? 'individual'} label={cellItem.valueLabel ?? cellItem.value} icon={cellItem.valueIcon} entityIri={cellItem.value} onopen={(iri) => entityBindingContext?.openEntity(iri)} />
                      {:else if cellItem && (column.valueType === 'date' || column.valueType === 'date_time')}
                        {@const date = dateValue(cellItem.value, column.valueType)}
                        {#if date}<span class="date-value-display"><time class="date-value-absolute" datetime={date.dateTime}>{date.absolute}</time><span class="date-value-relative semantic-metadata">{date.relative}</span></span>{:else}{recordCell(row, column)}{/if}
                      {:else}{recordCell(row, column)}{/if}
                    </td>
                  {/each}
                  {#if recordsPage.sources.length > 1}<td><span class="records-source">{recordsPage.sources.find((source) => source.alias === row.sourceAlias)?.label ?? row.sourceAlias}</span></td>{/if}
                  <td><button type="button" class="records-open" aria-label="Abrir registro {row.entityIri}" title="Abrir registro" onclick={() => entityBindingContext?.openEntity(row.entityIri)}><span class="material-symbols-outlined" aria-hidden="true">open_in_new</span></button></td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
        <footer class="records-pagination" aria-live="polite">
          <span class="semantic-metadata">{recordsPage.rows.length} de {recordsPage.pageInfo.totalCount} registros{#if recordsPage.loading} · atualizando{/if}</span>
          {#if recordsPage.pageInfo.hasMore}<Button variant="outline" size="sm" disabled={recordsPage.loading} onclick={() => entityBindingContext?.loadRecords(recordsElementId, recordsPage.criteria, recordsPage.pageInfo.nextCursor)}><span class="material-symbols-outlined" aria-hidden="true">{recordsPage.loading ? 'progress_activity' : 'expand_more'}</span>Carregar próxima página</Button>{/if}
        </footer>
      {:else}
        <p class="supporting-text">{props.emptyMessage ?? 'Nenhum registro encontrado.'}</p>
      {/if}
    {:else}
      <p class="supporting-text" role="status">Preparando a lista de registros…</p>
    {/if}
  </section>
{:else if componentType === 'Confirmation'}
  <section class="semantic-block confirmation-block" aria-labelledby={semanticDomIds.confirmationTitle}>
    <header class="confirmation-heading">
      <span class="material-symbols-outlined" aria-hidden="true">{props.reversibility === 'irreversible' ? 'warning' : 'help'}</span>
      <div><p class="eyebrow">Confirmação necessária</p><h2 id={semanticDomIds.confirmationTitle}>{props.action}</h2></div>
    </header>
    <p class="confirmation-consequence">{props.consequence}</p>
    {#if props.impact}<p class="context-note">{props.impact}</p>{/if}
    <p class="supporting-text">{reversibilityLabel(props.reversibility)}</p>
    <div class="confirmation-actions">
      <Button variant="outline" onclick={(event) => emitEvent('cancel', event)}>{String(props.cancelLabel ?? 'Cancelar')}</Button>
      {#if props.requiresConfirmation !== false}
        <Button variant={props.reversibility === 'irreversible' ? 'destructive' : 'default'} onclick={(event) => emitEvent('confirm', event)}>{String(props.confirmLabel ?? 'Confirmar')}</Button>
      {/if}
    </div>
  </section>
{/if}

<style>
  .semantic-block { display: grid; gap: var(--spacing-field-group); min-width: 0; color: var(--text-body); }
  .semantic-children { display: grid; grid-template-columns: repeat(var(--information-columns, 1), minmax(0, 1fr)); align-items: start; gap: var(--spacing-section); min-width: 0; }
  :global(.semantic-children > *) { min-width: 0; }
  :global(.information-block.first-child-spans > .semantic-children > .semantic-block:first-child) { grid-column: 1 / -1; }
  :global(.information-block > .semantic-children > .semantic-block:not(:first-child) h2) { font-size: var(--type-body); }
  :global(.information-block.contract-base > .semantic-children > .semantic-block) {
    padding: var(--spacing-widget-inset);
    background: var(--color-information-group-surface);
  }
  :global(.information-block.contract-base > .semantic-children > .semantic-block:first-child h2) {
    font-size: var(--type-title);
    font-weight: var(--weight-primary-title);
  }
  :global(.information-block.contract-base > .semantic-children > .semantic-block:not(:first-child) h2) { font-size: var(--type-body-prominent); }
  :global(.information-block > .semantic-children > .semantic-block:first-child .fact-list dd),
  :global(.information-block > .semantic-children > .semantic-block:first-child .read-only-value) { font-size: var(--type-title); }
  :global(.information-block.contract-base > .semantic-children > .semantic-block:first-child .fact-list dd),
  :global(.information-block.contract-base > .semantic-children > .semantic-block:first-child .read-only-value) {
    font-size: var(--type-display);
    font-weight: var(--weight-primary-title);
  }
  .semantic-heading, .confirmation-heading { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--spacing-section); }
  h2, p { margin: 0; }
  h2 { color: var(--text-primary); font-size: var(--type-heading); font-weight: 400; overflow-wrap: anywhere; }
  .eyebrow { margin-block-end: var(--spacing-label-to-value); color: var(--text-accent); font-family: var(--font-mono); font-size: var(--type-label); text-transform: uppercase; letter-spacing: 0.08em; }
  .supporting-text { color: var(--text-secondary); font-size: var(--type-body-compact); line-height: 1.45; overflow-wrap: anywhere; }
  .semantic-icon { color: var(--text-accent); font-size: var(--type-icon-large); }
  .candidate-list { display: grid; gap: var(--spacing-control-internal); }
  .candidate { position: relative; display: grid; grid-template-columns: auto minmax(0, 1fr); align-items: start; gap: var(--spacing-control-internal); width: 100%; padding: var(--spacing-control-internal); border: var(--size-hud-frame-line) solid var(--color-hud-line-subtle); border-radius: var(--radius); background: var(--color-surface-2); color: var(--text-body); text-align: start; cursor: pointer; }
  .candidate:hover, .candidate.selected, .candidate:has(.selection-control:checked) { border-color: var(--color-interactive); background: color-mix(in srgb, var(--color-interactive) 10%, var(--color-surface-2)); }
  .candidate:has(.selection-control:focus-visible) { outline: 2px solid var(--color-interactive); outline-offset: 2px; }
  .candidate:has(.selection-control:disabled) { color: var(--text-disabled); cursor: not-allowed; }
  .selection-control { position: absolute; width: 1px; height: 1px; opacity: 0; }
  .candidate-selection { color: var(--text-accent); font-size: var(--type-icon-medium); }
  .candidate-copy { display: grid; gap: var(--spacing-label-to-value); min-width: 0; }
  .candidate-title { color: var(--text-primary); font-size: var(--type-body); }
  .comparison-values { display: flex; flex-wrap: wrap; gap: var(--spacing-control-internal) var(--spacing-section); color: var(--text-primary); font-size: var(--type-body-compact); }
  .comparison-values > span { display: grid; gap: var(--spacing-label-to-value); }
  .comparison-label, .field-label { color: var(--text-secondary); font-size: var(--type-label); }
  .semantic-input { box-sizing: border-box; width: 100%; min-width: 0; padding: var(--spacing-control-internal); border: var(--size-hud-frame-line) solid var(--color-hud-line-subtle); border-radius: var(--radius); background: var(--color-surface-2); color: var(--text-primary); font: inherit; }
  .semantic-input:focus-visible, summary:focus-visible { outline: 2px solid var(--color-interactive); outline-offset: 2px; }
  .semantic-multiline { min-height: 8rem; resize: vertical; white-space: pre-wrap; overflow-wrap: anywhere; }
  .context-note, .action-status { display: flex; align-items: flex-start; gap: var(--spacing-label-to-value); color: var(--text-secondary); font-size: var(--type-body-compact); }
  .context-note .material-symbols-outlined, .action-status .material-symbols-outlined { flex: 0 0 auto; color: var(--text-accent); font-size: var(--type-icon-small); }
  .value-entry { gap: var(--spacing-label-to-value); }
  .field-label { color: var(--text-primary); }
  .value-control { display: flex; align-items: center; gap: var(--spacing-control-internal); }
  .multiple-value-list { display: grid; gap: var(--spacing-control-internal); }
  .multiple-value-row { display: grid; grid-template-columns: minmax(0, 1fr) auto; align-items: start; gap: var(--spacing-control-internal); }
  .unit-label, .sensitivity { color: var(--text-secondary); white-space: nowrap; }
  .read-only-value { display: flex; gap: var(--spacing-control-internal); color: var(--text-primary); }
  .bound-value-list { display: grid; justify-items: start; gap: var(--spacing-control-internal); min-width: 0; }
  .binding-error { display: flex; align-items: center; flex-wrap: wrap; gap: var(--spacing-control-internal); color: var(--color-error); overflow-wrap: anywhere; }
  .validation-message { color: var(--color-success); font-size: var(--type-body-compact); }
  .validation-message.invalid { color: var(--color-error); }
  .action-block { grid-template-columns: minmax(0, 1fr) auto; align-items: center; }
  .action-icon { font-size: var(--type-icon-small); }
  .action-copy { display: grid; gap: var(--spacing-label-to-value); }
  .status-indicator { display: inline-flex; align-items: center; gap: var(--spacing-label-to-value); color: var(--text-accent); font-size: var(--type-body-compact); }
  .status-indicator .material-symbols-outlined { font-size: var(--type-icon-small); }
  .information-title-row { display: flex; align-items: center; gap: var(--spacing-label-to-value); }
  :global(.context-tooltip-trigger) { display: inline-grid; place-items: center; flex: 0 0 auto; min-width: var(--size-touch-target); min-height: var(--size-touch-target); padding: 0; border: 0; background: transparent; color: var(--text-secondary); cursor: pointer; }
  :global(.context-tooltip-trigger:hover), :global(.context-tooltip-trigger:focus-visible) { color: var(--color-interactive); }
  :global(.context-tooltip-trigger:focus-visible) { outline: var(--size-hud-frame-line) solid var(--color-interactive); outline-offset: var(--spacing-label-to-value); }
  :global(.context-tooltip-trigger .material-symbols-outlined) { font-size: var(--type-icon-small); }
  .fact-list, .relationship-list { display: grid; gap: var(--spacing-control-internal); margin: 0; }
  .fact, .relationship-list > div { display: grid; grid-template-columns: minmax(0, 0.7fr) minmax(0, 1.3fr); gap: var(--spacing-label-to-value); min-width: 0; }
  .fact:not(.fact-labeled) { grid-template-columns: minmax(0, 1fr); }
  dt { min-width: 0; color: var(--text-secondary); font-size: var(--type-body-compact); overflow-wrap: anywhere; }
  dd { display: flex; flex-wrap: wrap; align-items: flex-start; gap: var(--spacing-label-to-value); min-width: 0; margin: 0; color: var(--text-primary); overflow-wrap: normal; word-break: normal; }
  .atomic-value { white-space: nowrap; }
  details > summary { display: flex; align-items: center; justify-content: space-between; gap: var(--spacing-section); cursor: pointer; list-style: none; }
  details > summary::-webkit-details-marker { display: none; }
  details summary > span:first-child { display: grid; grid-template-columns: auto minmax(0, 1fr); align-items: center; gap: var(--spacing-label-to-value) var(--spacing-control-internal); min-width: 0; }
  .detail-title { color: var(--text-primary); }
  .detail-list { margin-block-start: var(--spacing-control-internal); }
  .disclosure-icon { color: var(--text-secondary); }
  details[open] .disclosure-icon { transform: rotate(180deg); }
  .confirmation-heading { justify-content: flex-start; }
  .confirmation-heading > .material-symbols-outlined { color: var(--color-danger); }
  .confirmation-consequence { color: var(--text-primary); }
  .confirmation-actions { display: flex; justify-content: flex-end; gap: var(--spacing-control-internal); }
  .comparison-heading { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--spacing-section); }
  .comparison-values { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: var(--spacing-control-internal); }
  .comparison-period-value { display: grid; gap: var(--spacing-label-to-value); min-width: 0; padding: var(--spacing-control-internal); background: var(--color-surface-2); }
  .comparison-current strong { color: var(--text-primary); font-size: var(--type-heading); font-weight: 500; }
  .comparison-reference > .comparison-value { color: var(--text-primary); font-size: var(--type-body); }
  .comparison-value { white-space: nowrap; overflow-wrap: normal; word-break: normal; font-variant-numeric: tabular-nums; }
  .comparison-change { display: flex; flex-wrap: wrap; align-items: center; gap: var(--spacing-control-internal); margin: 0; color: var(--text-secondary); }
  .comparison-change .material-symbols-outlined { font-size: var(--type-icon-small); }
  .comparison-change.favorable { color: var(--color-success); }
  .comparison-change.unfavorable { color: var(--color-warning); }
  .comparison-change-divider { align-self: stretch; width: var(--size-hud-frame-line); min-height: var(--size-touch-target); background: var(--color-hud-line-subtle); }
  .distribution-groups { display: grid; gap: var(--spacing-section); margin: 0; padding: 0; list-style: none; }
  .distribution-row { display: grid; gap: var(--spacing-label-to-value); min-width: 0; }
  .distribution-row-heading { display: flex; justify-content: space-between; gap: var(--spacing-section); color: var(--text-primary); }
  .distribution-row-heading strong { text-align: end; overflow-wrap: anywhere; }
  .distribution-track { height: var(--size-hud-frame-line); background: var(--color-surface-3); }
  .distribution-track > span { display: block; width: var(--distribution-share); height: 100%; background: var(--color-interactive); }
  .distribution-total { display: flex; justify-content: space-between; gap: var(--spacing-section); padding-block-start: var(--spacing-control-internal); border-block-start: var(--size-hud-frame-line) solid var(--color-hud-line-subtle); color: var(--text-primary); }
  .distribution-total strong { text-align: end; }
  .records-filters { display: flex; flex-wrap: wrap; align-items: center; gap: var(--spacing-inline-related); }
  .records-filters .semantic-input { min-width: 9rem; max-width: 16rem; }
  .records-table-scroll { overflow-x: auto; }
  .records-table { width: 100%; border-collapse: collapse; color: var(--text-primary); }
  .records-table th, .records-table td { padding: var(--spacing-control-internal); text-align: start; vertical-align: top; border-block-end: var(--size-hud-frame-line) solid var(--color-hud-line-subtle); }
  .records-table th { color: var(--text-metadata); font: inherit; }
  .records-table td { overflow-wrap: anywhere; }
  .records-table td[data-column-type="currency"], .records-table td[data-column-type="number"], .records-table td[data-column-type="percentage"] { white-space: nowrap; overflow-wrap: normal; }
  .records-table td[data-column-type="date"], .records-table td[data-column-type="date_time"] { white-space: normal; }
  .date-value-display { display: grid; gap: var(--spacing-label-to-value); min-width: 0; }
  .date-value-absolute { white-space: nowrap; }
  .date-value-relative { white-space: nowrap; }
  .records-sort, .records-open { display: inline-flex; align-items: center; gap: var(--spacing-inline); padding: 0; color: var(--color-interactive); background: transparent; border: 0; cursor: pointer; }
  .records-sort .material-symbols-outlined, .records-open .material-symbols-outlined { font-size: var(--type-icon-small); }
  .records-open { justify-content: center; min-width: var(--size-touch-target); min-height: var(--size-touch-target); }
  .records-pagination { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: var(--spacing-control-internal); }
  .visually-hidden { position: absolute; width: 1px; height: 1px; padding: 0; margin: -1px; overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; border: 0; }

  @media (max-width: 34rem) {
    .action-block { grid-template-columns: 1fr; }
    .action-block :global(button) { justify-self: start; }
  }
</style>
