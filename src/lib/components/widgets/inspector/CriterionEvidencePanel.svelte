<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { Button } from '$lib/components/ui/button';
  import { Label } from '$lib/components/ui/label';
  import * as RadioGroup from '$lib/components/ui/radio-group';
  import { Textarea } from '$lib/components/ui/textarea';

  interface ValidationCursor {
    createdAtJulianDay: number;
    iri: string;
    snapshotTx: number;
  }

  interface ValidationItem {
    iri: string;
    createdAt: string;
    action: string | null;
    expectedResult: string | null;
    observedResult: string | null;
    reason: string | null;
    statusIri: string | null;
  }

  interface ValidationPage {
    items: ValidationItem[];
    latest: ValidationItem | null;
    currentStatusIri: string | null;
    consistency: 'consistent' | 'missing_evidence' | 'divergent' | 'not_terminal';
    nextCursor: ValidationCursor | null;
    hasMore: boolean;
    snapshotTx: number;
  }

  interface AllowedStatus {
    iri: string;
    label: string;
    color?: string | null;
  }

  let {
    criterionIri,
    allowedStatuses = [],
    refreshKey = 0,
    howToTest = '',
    successResult = ''
  }: {
    criterionIri: string;
    allowedStatuses?: AllowedStatus[];
    refreshKey?: number;
    howToTest?: string;
    successResult?: string;
  } = $props();

  const PAGE_SIZE = 20;
  let page = $state<ValidationPage | null>(null);
  let items = $state<ValidationItem[]>([]);
  let snapshotTx = $state<number | null>(null);
  let nextCursor = $state<ValidationCursor | null>(null);
  let hasMore = $state(false);
  let loading = $state(true);
  let loadingMore = $state(false);
  let error = $state<string | null>(null);
  let formOpen = $state(false);
  let outcome = $state<'passed' | 'failed' | 'blocked' | null>(null);
  let actionDraft = $state('');
  let observedDraft = $state('');
  let reasonDraft = $state('');
  let saving = $state(false);
  let formError = $state<string | null>(null);
  let savedButNotRefreshed = $state(false);
  let savedMessage = $state<string | null>(null);
  let requestSequence = 0;

  const idSuffix = $derived(criterionIri.replaceAll(/[^a-zA-Z0-9_-]/g, '-'));
  const titleId = $derived(`criterion-evidence-title-${idSuffix}`);
  const latestTitleId = $derived(`latest-evidence-title-${idSuffix}`);
  const historyTitleId = $derived(`validation-history-title-${idSuffix}`);
  const formTitleId = $derived(`validation-form-title-${idSuffix}`);
  const priorItems = $derived(items.filter((item) => item.iri !== page?.latest?.iri));
  const currentStatus = $derived(statusFor(page?.currentStatusIri ?? null));
  const latestStatus = $derived(statusFor(page?.latest?.statusIri ?? null));
  const consistencyInfo = $derived.by(() => {
    switch (page?.consistency) {
      case 'consistent':
        return { icon: 'verified', label: 'Status e evidência coerentes', tone: 'success' };
      case 'missing_evidence':
        return { icon: 'warning', label: 'Status final sem evidência', tone: 'warning' };
      case 'divergent':
        return { icon: 'sync_problem', label: 'Status diferente da validação mais recente', tone: 'error' };
      case 'not_terminal':
        return { icon: 'schedule', label: 'Critério ainda sem validação final', tone: 'neutral' };
      default:
        return null;
    }
  });

  $effect(() => {
    const requestedCriterion = criterionIri;
    refreshKey;
    if (!requestedCriterion) return;
    let active = true;
    void loadFirstPage(requestedCriterion, () => active);
    return () => {
      active = false;
      requestSequence += 1;
    };
  });

  function statusFor(iri: string | null) {
    return allowedStatuses.find((status) => status.iri === iri) ?? null;
  }

  function openForm() {
    formOpen = true;
    outcome = null;
    actionDraft = howToTest;
    observedDraft = '';
    reasonDraft = '';
    formError = null;
    savedMessage = null;
    savedButNotRefreshed = false;
  }

  function closeForm() {
    if (saving) return;
    formOpen = false;
    formError = null;
  }

  function selectOutcome(selected: string) {
    if (selected === 'passed' || selected === 'failed' || selected === 'blocked') outcome = selected;
    formError = null;
  }

  function statusIriForOutcome() {
    if (outcome === 'passed') return 'foundation:Completed';
    if (outcome === 'failed') return 'foundation:Rejected';
    if (outcome === 'blocked') return 'foundation:Blocked';
    return null;
  }

  function applyPage(result: ValidationPage) {
    page = result;
    items = result.items;
    snapshotTx = result.snapshotTx;
    nextCursor = result.nextCursor;
    hasMore = result.hasMore;
  }

  async function refreshAfterSave() {
    const result = await invoke<ValidationPage>('inspector__list_criterion_validations', {
      request: { criterionIri, limit: PAGE_SIZE }
    });
    applyPage(result);
  }

  async function retryRefresh() {
    if (loading || saving) return;
    loading = true;
    formError = null;
    try {
      await refreshAfterSave();
      savedButNotRefreshed = false;
      formOpen = false;
      savedMessage = 'Validação registrada e histórico atualizado.';
    } catch (cause) {
      formError = `A validação já foi gravada, mas não foi possível atualizar o histórico: ${String(cause)}`;
    } finally {
      loading = false;
    }
  }

  async function submitValidation(event: Event) {
    event.preventDefault();
    if (saving || savedButNotRefreshed) return;
    const statusIri = statusIriForOutcome();
    const action = actionDraft.trim();
    const observedResult = observedDraft.trim();
    const reason = reasonDraft.trim();
    if (!statusIri) {
      formError = 'Escolha se o critério passou, falhou ou ficou bloqueado.';
      return;
    }
    if (!action) {
      formError = 'Informe o teste executado.';
      return;
    }
    if (!successResult.trim()) {
      formError = 'Este critério precisa ter um resultado esperado configurado antes de registrar a validação.';
      return;
    }
    if (!observedResult) {
      formError = 'Informe o resultado observado.';
      return;
    }
    if ((outcome === 'failed' || outcome === 'blocked') && !reason) {
      formError = 'Informe o motivo da falha ou do bloqueio.';
      return;
    }

    saving = true;
    formError = null;
    savedMessage = null;
    try {
      await invoke('inspector__record_criterion_validation', {
        request: {
          criterionIri,
          action,
          expectedResult: successResult,
          observedResult,
          reason: reason || null,
          statusIri
        }
      });
      try {
        await refreshAfterSave();
        formOpen = false;
        savedMessage = 'Validação registrada e histórico atualizado.';
      } catch (cause) {
        savedButNotRefreshed = true;
        formError = `A validação foi gravada, mas não foi possível atualizar o histórico: ${String(cause)}`;
      }
    } catch (cause) {
      formError = `Não foi possível registrar a validação: ${String(cause)}`;
    } finally {
      saving = false;
    }
  }

  function formatDate(value: string | null) {
    if (!value) return 'Data não informada';
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return value;
    return date.toLocaleString('pt-BR', { dateStyle: 'medium', timeStyle: 'short' });
  }

  async function loadFirstPage(iri: string, isActive: () => boolean = () => true) {
    const sequence = ++requestSequence;
    loading = true;
    error = null;
    page = null;
    items = [];
    snapshotTx = null;
    nextCursor = null;
    hasMore = false;

    try {
      const result = await invoke<ValidationPage>('inspector__list_criterion_validations', {
        request: { criterionIri: iri, limit: PAGE_SIZE }
      });
      if (!isActive() || sequence !== requestSequence || iri !== criterionIri) return;
      page = result;
      items = result.items;
      snapshotTx = result.snapshotTx;
      nextCursor = result.nextCursor;
      hasMore = result.hasMore;
    } catch (cause) {
      if (!isActive() || sequence !== requestSequence || iri !== criterionIri) return;
      error = `Não foi possível carregar as evidências: ${String(cause)}`;
    } finally {
      if (isActive() && sequence === requestSequence && iri === criterionIri) loading = false;
    }
  }

  async function loadMore() {
    if (!criterionIri || snapshotTx === null || nextCursor === null || loadingMore) return;
    const requestedCriterion = criterionIri;
    const sequence = requestSequence;
    loadingMore = true;
    error = null;

    try {
      const result = await invoke<ValidationPage>('inspector__list_criterion_validations', {
        request: {
          criterionIri: requestedCriterion,
          snapshotTx,
          cursor: nextCursor,
          limit: PAGE_SIZE
        }
      });
      if (sequence !== requestSequence || requestedCriterion !== criterionIri) return;
      page = result;
      items = [...items, ...result.items];
      nextCursor = result.nextCursor;
      hasMore = result.hasMore;
    } catch (cause) {
      if (sequence !== requestSequence || requestedCriterion !== criterionIri) return;
      error = `Não foi possível carregar as validações anteriores: ${String(cause)}`;
    } finally {
      if (sequence === requestSequence && requestedCriterion === criterionIri) loadingMore = false;
    }
  }
</script>

<section class="criterion-evidence" aria-labelledby={titleId}>
  <div class="section-heading">
    <span class="material-symbols-outlined section-icon" aria-hidden="true">fact_check</span>
    <h2 id={titleId}>Evidências de validação</h2>
  </div>

  {#if loading}
    <p class="state-message" role="status" aria-live="polite">
      <span class="material-symbols-outlined spinning" aria-hidden="true">progress_activity</span>
      Carregando evidências…
    </p>
  {:else if error && !page}
    <div class="error-state" role="alert">
      <p>{error}</p>
      <Button variant="outline" size="sm" onclick={() => loadFirstPage(criterionIri)}>
        Tentar novamente
      </Button>
    </div>
  {:else if page}
    {#if consistencyInfo}
      <div class="consistency consistency-{consistencyInfo.tone}" role="status" aria-live="polite">
        <span class="material-symbols-outlined" aria-hidden="true">{consistencyInfo.icon}</span>
        <span>{consistencyInfo.label}</span>
        {#if currentStatus}
          <span class="current-status">
            {#if page.consistency === 'divergent' && latestStatus}
              Status atual: {currentStatus.label} · validação mais recente: {latestStatus.label}
            {:else}
              Status atual: {currentStatus.label}
            {/if}
          </span>
        {/if}
      </div>
    {/if}

    {#if savedMessage}
      <p class="saved-message" role="status" aria-live="polite">{savedMessage}</p>
    {/if}

        {#if page.latest}
      <article class="latest-evidence" aria-labelledby={latestTitleId}>
        <div class="evidence-heading">
          <h3 id={latestTitleId}>Validação mais recente</h3>
          {#if latestStatus}
            <span class="result-status" style="--status-color: {latestStatus.color || 'var(--color-neutral)'}">
              <span class="material-symbols-outlined" aria-hidden="true">radio_button_checked</span>
              {latestStatus.label}
            </span>
          {/if}
        </div>
        <time class="evidence-date" datetime={page.latest.createdAt}>{formatDate(page.latest.createdAt)}</time>
        {#if page.latest.action}
          <div class="evidence-field">
            <span class="field-label">Teste executado</span>
            <p>{page.latest.action}</p>
          </div>
        {/if}
        <div class="result-grid">
          {#if page.latest.expectedResult}
            <div class="evidence-field">
              <span class="field-label">Resultado esperado</span>
              <p>{page.latest.expectedResult}</p>
            </div>
          {/if}
          {#if page.latest.observedResult}
            <div class="evidence-field">
              <span class="field-label">Resultado observado</span>
              <p>{page.latest.observedResult}</p>
            </div>
          {/if}
        </div>
        {#if page.latest.reason}
          <div class="evidence-field reason-field">
            <span class="field-label">Motivo</span>
            <p>{page.latest.reason}</p>
          </div>
        {/if}
      </article>
    {:else}
      <p class="empty-state">
        <span class="material-symbols-outlined" aria-hidden="true">history</span>
        Nenhuma validação registrada para este critério.
      </p>
    {/if}

    {#if formOpen}
      <form class="validation-form" aria-labelledby={formTitleId} novalidate onsubmit={submitValidation}>
        <h3 id={formTitleId}>Registrar validação</h3>
        <fieldset class="outcome-picker" disabled={saving || savedButNotRefreshed}>
          <legend>Resultado do critério</legend>
          <RadioGroup.Root
            value={outcome ?? ''}
            onValueChange={selectOutcome}
            class="outcome-options"
            aria-label="Resultado do critério"
          >
            <div class="outcome-choice" class:selected={outcome === 'passed'}>
              <RadioGroup.Item value="passed" id={`outcome-passed-${idSuffix}`} />
              <Label for={`outcome-passed-${idSuffix}`} class="outcome-label">
                <span class="material-symbols-outlined" aria-hidden="true">check_circle</span> Passou
              </Label>
            </div>
            <div class="outcome-choice" class:selected={outcome === 'failed'}>
              <RadioGroup.Item value="failed" id={`outcome-failed-${idSuffix}`} />
              <Label for={`outcome-failed-${idSuffix}`} class="outcome-label">
                <span class="material-symbols-outlined" aria-hidden="true">cancel</span> Falhou
              </Label>
            </div>
            <div class="outcome-choice" class:selected={outcome === 'blocked'}>
              <RadioGroup.Item value="blocked" id={`outcome-blocked-${idSuffix}`} />
              <Label for={`outcome-blocked-${idSuffix}`} class="outcome-label">
                <span class="material-symbols-outlined" aria-hidden="true">block</span> Bloqueada
              </Label>
            </div>
          </RadioGroup.Root>
        </fieldset>
        <label class="form-field">
          <span>Teste executado</span>
          <Textarea bind:value={actionDraft} rows={3} required disabled={saving || savedButNotRefreshed} />
        </label>
        <div class="form-field">
          <span>Resultado esperado</span>
          <p class="expected-result">{successResult || 'Este critério não informa um resultado esperado.'}</p>
        </div>
        <label class="form-field">
          <span>Resultado observado</span>
          <Textarea bind:value={observedDraft} rows={3} required disabled={saving || savedButNotRefreshed} />
        </label>
        <label class="form-field">
          <span>Motivo {#if outcome === 'failed' || outcome === 'blocked'}<span class="required-note">obrigatório</span>{:else}<span class="optional-note">opcional</span>{/if}</span>
          <Textarea bind:value={reasonDraft} rows={2} required={outcome === 'failed' || outcome === 'blocked'} disabled={saving || savedButNotRefreshed} />
        </label>
        {#if formError}
          <p class="form-error" role="alert">{formError}</p>
        {/if}
        <div class="form-actions">
          {#if savedButNotRefreshed}
            <Button type="button" variant="outline" size="sm" disabled={loading} onclick={retryRefresh}>
              {#if loading}<span class="material-symbols-outlined spinning" aria-hidden="true">progress_activity</span>{/if}
              Atualizar histórico
            </Button>
          {:else}
            <Button type="button" variant="ghost" size="sm" disabled={saving} onclick={closeForm}>Cancelar</Button>
            <Button type="submit" size="sm" disabled={saving} onclick={submitValidation}>
              {#if saving}<span class="material-symbols-outlined spinning" aria-hidden="true">progress_activity</span>{/if}
              Registrar
            </Button>
          {/if}
        </div>
      </form>
    {:else}
      <Button variant="outline" size="sm" class="open-form" onclick={openForm}>
        <span class="material-symbols-outlined" aria-hidden="true">add_task</span>
        {page.latest ? 'Registrar validação' : 'Registrar primeira validação'}
      </Button>
    {/if}

    {#if priorItems.length > 0}
      <details class="history">
        <summary id={historyTitleId}>
          <h3>Validações anteriores</h3>
          <span class="history-summary-meta">{priorItems.length} · mais recentes primeiro</span>
          <span class="material-symbols-outlined history-chevron" aria-hidden="true">expand_more</span>
        </summary>
        {#each priorItems as item (item.iri)}
          {@const itemStatus = statusFor(item.statusIri)}
          <details class="history-entry">
            <summary>
              <time datetime={item.createdAt}>{formatDate(item.createdAt)}</time>
              {#if itemStatus}
                <span class="result-status" style="--status-color: {itemStatus.color || 'var(--color-neutral)'}">
                  <span class="material-symbols-outlined" aria-hidden="true">radio_button_checked</span>
                  {itemStatus.label}
                </span>
              {/if}
            </summary>
            <div class="history-content">
              {#if item.action}<div class="evidence-field"><span class="field-label">Teste executado</span><p>{item.action}</p></div>{/if}
              {#if item.expectedResult}<div class="evidence-field"><span class="field-label">Resultado esperado</span><p>{item.expectedResult}</p></div>{/if}
              {#if item.observedResult}<div class="evidence-field"><span class="field-label">Resultado observado</span><p>{item.observedResult}</p></div>{/if}
              {#if item.reason}<div class="evidence-field"><span class="field-label">Motivo</span><p>{item.reason}</p></div>{/if}
            </div>
          </details>
        {/each}
      </details>
    {/if}

    {#if error}
      <p class="load-more-error" role="alert">{error}</p>
    {/if}
    {#if hasMore}
      <Button variant="outline" size="sm" class="load-more" disabled={loadingMore} onclick={loadMore}>
        {#if loadingMore}
          <span class="material-symbols-outlined spinning" aria-hidden="true">progress_activity</span>
          Carregando…
        {:else}
          <span class="material-symbols-outlined" aria-hidden="true">expand_more</span>
          Carregar validações anteriores
        {/if}
      </Button>
    {/if}
  {/if}
</section>

<style>
  .criterion-evidence {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-field-group);
    margin-block: var(--spacing-section);
  }

  .section-heading,
  .evidence-heading,
  .result-status,
  .consistency,
  .state-message,
  .empty-state {
    display: flex;
    align-items: center;
  }

  .section-heading {
    gap: var(--spacing-inline-related);
    padding-block-end: var(--space-2);
    border-bottom: 1px solid var(--color-hud-line-subtle);
  }

  .section-icon {
    color: var(--color-interactive);
    font-size: var(--type-icon-small);
  }

  h2,
  h3,
  p {
    margin: 0;
  }

  h2 {
    color: var(--color-neutral-active);
    font-size: var(--type-body-prominent);
    font-weight: 400;
  }

  h3 {
    color: var(--color-neutral-active);
    font-size: var(--type-body-compact);
    font-weight: 400;
  }

  .consistency {
    gap: var(--spacing-inline-tight);
    padding: var(--space-2) var(--space-3);
    border-inline-start: 2px solid currentColor;
    background: var(--color-surface-hover-subtle);
    color: var(--color-neutral);
    font-size: var(--type-caption);
  }

  .consistency :global(.material-symbols-outlined),
  .result-status :global(.material-symbols-outlined),
  .state-message :global(.material-symbols-outlined),
  .empty-state :global(.material-symbols-outlined) {
    font-size: var(--type-icon-small);
  }

  .consistency-success { color: var(--color-success); }
  .consistency-warning { color: var(--color-warning); }
  .consistency-error { color: var(--color-error); }
  .consistency-neutral { color: var(--color-neutral); }

  .current-status {
    margin-inline-start: auto;
    color: var(--color-neutral-active);
  }

  .latest-evidence,
  .history-entry {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-field-block);
    padding: var(--space-3);
    border: 1px solid var(--color-hud-line-subtle);
    border-radius: var(--radius);
    background: color-mix(in srgb, var(--color-surface-2) 80%, transparent);
  }

  .evidence-heading {
    justify-content: space-between;
    gap: var(--spacing-inline-related);
    flex-wrap: wrap;
  }

  .result-status {
    gap: var(--spacing-inline-tight);
    color: var(--status-color);
    font-size: var(--type-caption);
  }

  .evidence-date,
  .history-entry summary time {
    color: var(--color-neutral-secondary);
    font-size: var(--type-caption);
  }

  .evidence-field {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-inline-tight);
    min-width: 0;
  }

  .field-label {
    color: var(--color-neutral-secondary);
    font-size: var(--type-overline);
    letter-spacing: var(--tracking-hud-label);
    text-transform: uppercase;
  }

  .evidence-field p,
  .state-message,
  .empty-state,
  .error-state p,
  .load-more-error {
    color: var(--color-neutral);
    font-size: var(--type-body-compact);
    line-height: 1.5;
    overflow-wrap: anywhere;
  }

  .saved-message {
    color: var(--color-success);
    font-size: var(--type-caption);
  }

  .validation-form {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-field-group);
    padding: var(--space-3);
    border: 1px solid var(--color-hud-line-subtle);
    border-radius: var(--radius);
    background: color-mix(in srgb, var(--color-surface-2) 80%, transparent);
  }

  .outcome-picker {
    border: 0;
    padding: 0;
    margin: 0;
    min-width: 0;
  }

  .outcome-picker legend,
  .form-field > span,
  .form-field > span:first-child {
    display: block;
    margin-block-end: var(--spacing-inline-tight);
    color: var(--color-neutral-secondary);
    font-size: var(--type-overline);
    letter-spacing: var(--tracking-hud-label);
    text-transform: uppercase;
  }

  .outcome-options {
    display: flex;
    flex-wrap: wrap;
    gap: var(--spacing-inline-tight);
  }

  .outcome-choice {
    display: inline-flex;
    align-items: center;
    gap: var(--spacing-inline-tight);
    min-height: 36px;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--color-hud-line-subtle);
    border-radius: var(--radius);
    background: var(--color-surface-1);
    color: var(--color-neutral);
  }

  .outcome-choice:hover { border-color: var(--color-interactive); }
  .outcome-choice.selected { border-color: var(--color-interactive); color: var(--color-interactive); background: color-mix(in srgb, var(--color-interactive) 12%, transparent); }
  .outcome-choice :global([data-slot="label"]) { display: inline-flex; align-items: center; gap: var(--spacing-inline-tight); color: inherit; font-size: var(--type-body-compact); font-weight: 400; cursor: pointer; }
  .outcome-choice :global(.material-symbols-outlined) { font-size: var(--type-icon-small); }

  .form-field {
    display: block;
    min-width: 0;
    color: var(--color-neutral);
    font-size: var(--type-body-compact);
  }

  .validation-form :global([data-slot="textarea"]) {
    background: color-mix(in srgb, var(--color-surface-4) 80%, transparent);
    color: var(--color-neutral-active);
    font-size: var(--type-body-compact);
  }

  .expected-result {
    padding: var(--space-2) var(--space-3);
    border-inline-start: 2px solid var(--color-neutral-secondary);
    background: color-mix(in srgb, var(--color-surface-3) 70%, transparent);
    overflow-wrap: anywhere;
  }

  .required-note,
  .optional-note { margin-inline-start: var(--space-2); text-transform: none; letter-spacing: normal; }
  .required-note { color: var(--color-warning); }
  .optional-note { color: var(--color-neutral-secondary); }
  .form-error { color: var(--color-error); font-size: var(--type-caption); }

  .form-actions {
    display: flex;
    justify-content: flex-end;
    gap: var(--spacing-inline-related);
  }

  .open-form { align-self: flex-start; }

  .result-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--spacing-field-group);
  }

  .reason-field {
    border-top: 1px solid var(--color-hud-line-subtle);
    padding-top: var(--space-2);
  }

  .history {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-inline-related);
  }

  .history > summary {
    display: flex;
    justify-content: space-between;
    gap: var(--spacing-inline-related);
    align-items: center;
    list-style: none;
    cursor: pointer;
  }

  .history > summary::-webkit-details-marker { display: none; }

  .history > summary:focus-visible {
    outline: 2px solid var(--color-interactive);
    outline-offset: 2px;
  }

  .history-summary-meta {
    color: var(--color-neutral-secondary);
    font-size: var(--type-caption);
  }

  .history-chevron {
    color: var(--color-neutral-secondary);
    font-size: var(--type-icon-small);
    transition: transform 160ms ease;
  }

  .history[open] > summary .history-chevron { transform: rotate(180deg); }

  @media (prefers-reduced-motion: reduce) {
    .history-chevron { transition: none; }
  }

  .history-entry {
    gap: 0;
    padding: 0;
    overflow: hidden;
  }

  .history-entry summary {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--spacing-inline-related);
    padding: var(--space-2) var(--space-3);
    cursor: pointer;
  }

  .history-entry summary:focus-visible {
    outline: 2px solid var(--color-interactive);
    outline-offset: -2px;
  }

  .history-content {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-field-block);
    padding: var(--space-3);
    border-top: 1px solid var(--color-hud-line-subtle);
  }

  .state-message,
  .empty-state {
    gap: var(--spacing-inline-related);
    padding: var(--space-3);
    background: color-mix(in srgb, var(--color-surface-2) 70%, transparent);
  }

  .error-state {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--spacing-field-group);
    padding: var(--space-3);
    border-inline-start: 2px solid var(--color-error);
    background: color-mix(in srgb, var(--color-error) 10%, transparent);
  }

  .load-more {
    align-self: flex-start;
  }

  .load-more-error {
    color: var(--color-error);
  }

  .spinning {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  @media (max-width: 600px) {
    .result-grid { grid-template-columns: minmax(0, 1fr); }
    .history-entry summary { align-items: flex-start; }
  }
</style>
