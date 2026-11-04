<script lang="ts">
  import { onMount, onDestroy, tick } from 'svelte'
  import { invoke } from '@tauri-apps/api/core'
  import { createEntitySubscription, type EntityEvent } from '$lib/realtime/subscriptions'

  type Cursor = { orderKey: string; iri: string }
  type Execution = { iri: string; startedAt: string | null; statusIri: string | null; finishedAt: string | null }
  type Step = {
    iri: string
    label?: string | null
    nodeLabel?: string | null
    statusIri?: string | null
    inputContext: unknown | null
    executionSummary?: string | null
    executionOutput: unknown[]
    stepError: unknown | null
  }
  type Page<T> = { items: T[]; totalCount: number | null; loadedCount: number; nextCursor: Cursor | null; hasMore: boolean; snapshotTx: number }
  type ExecutionDetail = { steps: Step[]; stepPage: Omit<Page<Step>, 'items'> }
  type Kpis = { totalStarted: number; terminalCount: number; minDurationMs: number | null; averageDurationMs: number | null; maxDurationMs: number | null; errorRate: number | null }
  type ExecutionPage = Page<Execution> & { kpis: Kpis }

  let { automationIri } = $props<{ automationIri: string }>()
  const PAGE_SIZE = 25
  let rangeEnd = $state(new Date())
  let rangeStart = $state(new Date(Date.now() - 30 * 24 * 60 * 60 * 1000))
  let page = $state<ExecutionPage | null>(null)
  let selectedIri = $state<string | null>(null)
  let detail = $state<ExecutionDetail | null>(null)
  let expandedSteps = $state(new Set<string>())
  let loading = $state(true)
  let loadingMore = $state(false)
  let detailLoading = $state(false)
  let error = $state<string | null>(null)
  let detailError = $state<string | null>(null)
  let executionItemsElement = $state<HTMLDivElement | null>(null)
  let loadInFlight = false
  let pendingReset = false
  let pendingPreserveSelection = false
  let loadWaiters: Array<() => void> = []
  let refreshInFlight = false
  let refreshPending = false

  const entitySub = createEntitySubscription(handleExecutionEntityEvent)

  const selected = $derived(page?.items.find(item => item.iri === selectedIri) ?? null)

  function handleExecutionEntityEvent(event: EntityEvent) {
    if (event.type === 'joined-set' &&
        event.classIri === 'foundation:WorkflowExecution' &&
        event.predicate === 'foundation:executesProcess' &&
        event.objectValue === automationIri) {
      void refreshHistory()
      return
    }
    if (event.type === 'updated' || event.type === 'referenced' || event.type === 'deleted') {
      void refreshHistory()
    }
  }

  async function loadExecutions(reset = true, preserveSelection = false) {
    if (loadInFlight) {
      pendingReset = pendingReset || reset
      pendingPreserveSelection = pendingPreserveSelection || preserveSelection
      await new Promise<void>(resolve => loadWaiters.push(resolve))
      return
    }
    loadInFlight = true
    if (reset) {
      loading = true
      error = null
    } else {
      loadingMore = true
      error = null
    }
    try {
      const cursor = reset ? null : page?.nextCursor ?? null
      const raw = await invoke<string>('automation__list_executions', {
        automationIri,
        rangeStart: rangeStart.toISOString(),
        rangeEnd: rangeEnd.toISOString(),
        snapshotTx: reset ? null : page?.snapshotTx ?? null,
        cursor,
        limit: PAGE_SIZE,
      })
      const result = JSON.parse(raw) as ExecutionPage
      const priorSelection = selectedIri
      page = reset ? result : {
        ...result,
        loadedCount: (page?.items.length ?? 0) + result.items.length,
        items: [...(page?.items ?? []), ...result.items],
      }
      if (preserveSelection && priorSelection) selectedIri = priorSelection
      else if (!page.items.some(item => item.iri === priorSelection)) selectedIri = page.items[0]?.iri ?? null
      else selectedIri = priorSelection
      entitySub.setIris([...page.items.map(item => item.iri), selectedIri ?? ''])
      if (reset) {
        entitySub.setSinceTx(result.snapshotTx)
        entitySub.replayMissed()
      }
      if (selectedIri && (page.items.some(item => item.iri === selectedIri) || preserveSelection)) await loadDetail(selectedIri, true)
      else { detail = null; detailError = null }
    } catch (cause) {
      error = String(cause)
    } finally {
      loading = false
      loadingMore = false
      loadInFlight = false
      if (pendingReset) {
        pendingReset = false
        const preserveQueuedSelection = pendingPreserveSelection
        pendingPreserveSelection = false
        await loadExecutions(true, preserveQueuedSelection)
      }
      loadWaiters.splice(0).forEach(resolve => resolve())
    }
  }

  async function refreshHistory() {
    if (refreshInFlight) {
      refreshPending = true
      return
    }
    refreshInFlight = true
    const preservedSelection = selectedIri
    const preservedScrollTop = executionItemsElement?.scrollTop ?? 0
    const focusedIri = (document.activeElement as HTMLElement | null)?.closest<HTMLElement>('[data-execution-iri]')?.dataset.executionIri ?? null
    try {
      await loadExecutions(true, true)
      while (preservedSelection && page?.hasMore && !page.items.some(item => item.iri === preservedSelection)) {
        await loadExecutions(false, true)
      }
      await tick()
      if (executionItemsElement) executionItemsElement.scrollTop = preservedScrollTop
      if (focusedIri) {
        Array.from(executionItemsElement?.querySelectorAll<HTMLElement>('[data-execution-iri]') ?? [])
          .find(element => element.dataset.executionIri === focusedIri)
          ?.focus({ preventScroll: true })
      }
    } finally {
      refreshInFlight = false
      if (refreshPending) {
        refreshPending = false
        void refreshHistory()
      }
    }
  }

  async function loadDetail(iri: string, reset = true) {
    if (reset) { detailLoading = true; detailError = null }
    else detailLoading = true
    try {
      const cursor = reset ? null : detail?.stepPage.nextCursor ?? null
      const raw = await invoke<string>('automation__get_execution', {
        executionIri: iri,
        snapshotTx: reset ? page?.snapshotTx ?? null : detail?.stepPage.snapshotTx ?? null,
        cursor,
        limit: PAGE_SIZE,
      })
      const result = JSON.parse(raw) as ExecutionDetail
      if (selectedIri !== iri) return
      detail = reset ? result : {
        ...result,
        stepPage: {
          ...result.stepPage,
          loadedCount: (detail?.steps.length ?? 0) + result.steps.length,
        },
        steps: [...(detail?.steps ?? []), ...result.steps],
      }
    } catch (cause) {
      if (selectedIri === iri) detailError = String(cause)
    } finally {
      if (selectedIri === iri) detailLoading = false
    }
  }

  function selectExecution(execution: Execution) {
    if (execution.iri === selectedIri) return
    selectedIri = execution.iri
    detail = null
    expandedSteps = new Set()
    loadDetail(execution.iri)
  }

  function toggleStep(iri: string) {
    const next = new Set(expandedSteps)
    if (next.has(iri)) next.delete(iri)
    else next.add(iri)
    expandedSteps = next
  }

  function applyRange() { loadExecutions(true) }

  function statusFor(execution: Execution) {
    const status = execution.statusIri ?? ''
    if (status === 'foundation:Completed' || status.includes('Completed') || status.includes('Concluído')) return { label: 'Sucesso', icon: 'check_circle', tone: 'success' }
    if (status.includes('Failed') || status.includes('Falhou') || status.includes('Error') || status.includes('1772993026091')) return { label: 'Falha', icon: 'error', tone: 'danger' }
    if (status.includes('Cancelled') || status.includes('Cancelado') || status.includes('1772570972069')) return { label: 'Cancelada', icon: 'cancel', tone: 'warning' }
    return { label: 'Em execução', icon: 'progress_activity', tone: 'running' }
  }

  function formatDate(value: string | null) {
    if (!value) return 'Data não registrada'
    const date = new Date(value)
    return Number.isNaN(date.getTime()) ? 'Data não registrada' : date.toLocaleString([], { dateStyle: 'short', timeStyle: 'short' })
  }

  function formatDateInput(date: Date) {
    const year = date.getFullYear()
    const month = String(date.getMonth() + 1).padStart(2, '0')
    const day = String(date.getDate()).padStart(2, '0')
    return `${year}-${month}-${day}`
  }

  function formatDuration(start: string | null, finish: string | null) {
    if (!start || !finish) return 'Não registrado'
    const elapsed = new Date(finish).getTime() - new Date(start).getTime()
    if (!Number.isFinite(elapsed) || elapsed < 0) return 'Não registrado'
    if (elapsed < 1000) return `${elapsed} ms`
    if (elapsed < 60000) return `${(elapsed / 1000).toFixed(1)} s`
    return `${Math.floor(elapsed / 60000)} min ${Math.floor((elapsed % 60000) / 1000)} s`
  }

  function formatMetric(ms: number | null) {
    if (ms == null || !Number.isFinite(ms)) return '—'
    if (ms < 1000) return `${Math.round(ms)} ms`
    if (ms < 60000) return `${(ms / 1000).toFixed(1)} s`
    return `${(ms / 60000).toFixed(1)} min`
  }

  function displayValue(value: unknown) {
    if (value == null || value === '') return 'Não registrado'
    if (typeof value === 'string') {
      try { return JSON.stringify(JSON.parse(value), null, 2) } catch { return value }
    }
    return JSON.stringify(value, null, 2) ?? 'Não registrado'
  }

  function stepLabel(step: Step, index: number) { return step.nodeLabel || step.label || `Etapa ${index + 1}` }

  $effect(() => {
    entitySub.setIris([...(page?.items.map(item => item.iri) ?? []), selectedIri ?? ''])
    entitySub.setCreationQueries([{
      classIri: 'foundation:WorkflowExecution',
      predicate: 'foundation:executesProcess',
      objectValue: automationIri,
    }])
  })

  onMount(async () => {
    entitySub.setCreationQueries([{
      classIri: 'foundation:WorkflowExecution',
      predicate: 'foundation:executesProcess',
      objectValue: automationIri,
    }])
    await loadExecutions(true)
  })
  onDestroy(() => { entitySub.destroy() })
</script>

<section class="history" aria-label="Histórico de execuções">
  <header class="history-header">
    <div class="metrics" aria-label="Métricas do período" aria-live="polite">
      {#each [
        { label: 'Execuções', value: page ? String(page.kpis.totalStarted) : '—' },
        { label: 'Duração mínima', value: page ? formatMetric(page.kpis.minDurationMs) : '—' },
        { label: 'Duração média', value: page ? formatMetric(page.kpis.averageDurationMs) : '—' },
        { label: 'Duração máxima', value: page ? formatMetric(page.kpis.maxDurationMs) : '—' },
        { label: 'Taxa de erro', value: page && page.kpis.errorRate != null ? `${(page.kpis.errorRate * 100).toFixed(1)}%` : '—' },
      ] as metric}
        <div class="metric"><span>{metric.label}</span><output>{metric.value}</output></div>
      {/each}
      {#if page && page.kpis.terminalCount === 0}
        <span class="metric-note">Sem execuções concluídas no período</span>
      {/if}
    </div>
    <form class="period" onsubmit={(event) => { event.preventDefault(); applyRange() }}>
      <label>De <input aria-label="Data inicial" type="date" value={formatDateInput(rangeStart)} onchange={(event) => { const date = new Date(`${event.currentTarget.value}T00:00:00`); if (!Number.isNaN(date.getTime())) rangeStart = date }} /></label>
      <label>Até <input aria-label="Data final" type="date" value={formatDateInput(rangeEnd)} onchange={(event) => { const date = new Date(`${event.currentTarget.value}T23:59:59.999`); if (!Number.isNaN(date.getTime())) rangeEnd = date }} /></label>
      <button class="apply" type="submit">Aplicar período</button>
    </form>
  </header>

  <div class="history-body">
    <aside class="execution-list" aria-label="Execuções">
      <div class="list-heading"><span>Execuções recentes</span><span class="pagination-count" aria-live="polite">{#if page}{#if page.totalCount != null}Mostrando {page.loadedCount} de {page.totalCount}{:else}{page.loadedCount} carregadas{/if}{/if}</span></div>
      {#if loading && !page}
        <div class="skeleton-list" aria-label="Carregando execuções">{#each [1, 2, 3, 4, 5] as item}<div class="skeleton" aria-hidden="true"></div>{/each}</div>
      {:else if error}
        <div class="inline-state" role="alert"><span>{error}</span><button class="apply" onclick={() => loadExecutions(true)}>Tentar novamente</button></div>
      {:else if page?.items.length}
        <div class="execution-items" aria-label="Lista de execuções" bind:this={executionItemsElement}>
          {#each page.items as execution (execution.iri)}
            {@const status = statusFor(execution)}
            <button data-execution-iri={execution.iri} class:selected={execution.iri === selectedIri} class="execution-item" aria-current={execution.iri === selectedIri ? 'true' : undefined} onclick={() => selectExecution(execution)}>
              <span class="status-icon {status.tone}" aria-hidden="true"><span class="material-symbols-outlined">{status.icon}</span></span>
              <span class="execution-copy"><span class="execution-state {status.tone}">{status.label}</span><span class="execution-date">{formatDate(execution.startedAt)}</span><span class="execution-duration">Duração · {formatDuration(execution.startedAt, execution.finishedAt)}</span></span>
            </button>
          {/each}
        </div>
        {#if page.hasMore}<button class="load-more" disabled={loadingMore} onclick={() => loadExecutions(false)}>{loadingMore ? 'Carregando…' : 'Carregar mais'}</button>{/if}
      {:else}
        <div class="inline-state empty"><span class="material-symbols-outlined" aria-hidden="true">history</span><span>Nenhuma execução neste período.</span><span>Amplie o intervalo para consultar mais histórico.</span></div>
      {/if}
    </aside>

    <main class="execution-detail" aria-label="Detalhes da execução" aria-live="polite">
      {#if detailError}
        <div class="inline-state" role="alert"><span>{detailError}</span><button class="apply" onclick={() => selectedIri && loadDetail(selectedIri)}>Tentar novamente</button></div>
      {:else if detailLoading && !detail}
        <div class="detail-skeleton" aria-label="Carregando detalhes"><div class="skeleton"></div><div class="skeleton"></div><div class="skeleton"></div></div>
      {:else if selected}
        {@const status = statusFor(selected)}
        <div class="detail-heading"><div><span class="eyebrow">EXECUÇÃO</span><h2>{formatDate(selected.startedAt)}</h2></div><span class="execution-state {status.tone}"><span class="material-symbols-outlined" aria-hidden="true">{status.icon}</span>{status.label}</span></div>
        <div class="detail-meta"><span>Início · {formatDate(selected.startedAt)}</span><span>Tempo · {formatDuration(selected.startedAt, selected.finishedAt)}</span><span>Resultado · {selected.finishedAt ? status.label : 'Em andamento'}</span></div>
        <div class="step-list" aria-label="Etapas da execução">
          {#if detail}<div class="step-count">{#if detail.stepPage.totalCount != null}Mostrando {detail.stepPage.loadedCount} de {detail.stepPage.totalCount} etapas{:else}{detail.stepPage.loadedCount} etapas carregadas{/if}</div>{/if}
          {#each detail?.steps ?? [] as step, index (step.iri)}
            <section class="step">
              <button class="step-toggle" aria-expanded={expandedSteps.has(step.iri)} onclick={() => toggleStep(step.iri)}>
                <span class="step-index">{String(index + 1).padStart(2, '0')}</span><span class="step-name">{stepLabel(step, index)}</span><span class="material-symbols-outlined disclosure" aria-hidden="true">{expandedSteps.has(step.iri) ? 'expand_less' : 'expand_more'}</span>
              </button>
              {#if expandedSteps.has(step.iri)}
                <div class="step-content">
                  <div class="data-block"><h3>Entrada</h3><pre>{displayValue(step.inputContext)}</pre></div>
                  {#if step.executionSummary}<div class="data-block"><h3>Resumo</h3><pre>{displayValue(step.executionSummary)}</pre></div>{/if}
                  <div class="data-block"><h3>Saída</h3><pre>{displayValue(step.executionOutput)}</pre></div>
                  {#if step.stepError != null}<div class="data-block failure"><h3>Erro</h3><pre>{displayValue(step.stepError)}</pre></div>{/if}
                </div>
              {/if}
            </section>
          {:else}
            {#if !detailLoading}<p class="no-steps">Nenhuma etapa registrada nesta execução.</p>{/if}
          {/each}
        </div>
        {#if detail?.stepPage.hasMore}<button class="load-more detail-more" disabled={detailLoading} onclick={() => selectedIri && loadDetail(selectedIri, false)}>{detailLoading ? 'Carregando…' : `Carregar mais etapas (${detail.stepPage.loadedCount}${detail.stepPage.totalCount != null ? ` de ${detail.stepPage.totalCount}` : ' carregadas'})`}</button>{/if}
      {:else if !loading && !error}
        <div class="inline-state empty"><span class="material-symbols-outlined" aria-hidden="true">fact_check</span><span>Selecione uma execução para inspecionar as etapas.</span></div>
      {/if}
    </main>
  </div>
</section>

<style>
  .history { height: 100%; min-height: 0; display: flex; flex-direction: column; color: var(--text-body); background: color-mix(in srgb, var(--color-surface-1) 96%, transparent); }
  .history-header { flex: 0 0 auto; padding: var(--spacing-widget-inset); border-bottom: var(--size-hud-frame-line) solid var(--color-hud-line-subtle); display: flex; flex-direction: column; gap: var(--spacing-field-group); }
  .metrics { display: flex; flex-wrap: wrap; gap: var(--spacing-field-group) var(--spacing-section); align-items: end; }
  .metric { min-width: var(--size-toast-recalc-min-width); display: grid; gap: var(--spacing-label-to-value); }
  .metric > span, .period label, .list-heading, .metric-note { color: var(--text-secondary); font-size: var(--type-caption-compact); }
  .metric output { color: var(--text-primary); font-size: var(--type-body); font-variant-numeric: tabular-nums; }
  .metric-note { color: var(--text-secondary); }
  .period { display: flex; flex-wrap: wrap; align-items: end; gap: var(--spacing-inline-related); }
  .period label { display: grid; gap: var(--spacing-label-to-value); }
  .period input { min-height: var(--size-touch-target); padding: var(--space-1) var(--space-2); border: var(--size-hud-frame-line) solid var(--color-hud-line-subtle); border-radius: var(--radius); background: var(--color-surface-2); color: var(--text-primary); color-scheme: dark; font: inherit; }
  .apply, .load-more { min-height: var(--size-touch-target); padding: var(--space-1) var(--space-3); border: var(--size-hud-frame-line) solid var(--color-hud-line); border-radius: var(--radius); background: var(--color-surface-2); color: var(--color-interactive); cursor: pointer; font: inherit; font-size: var(--type-caption-compact); }
  .apply:hover, .load-more:hover:not(:disabled) { background: color-mix(in srgb, var(--color-interactive) 12%, var(--color-surface-2)); }
  .apply:focus-visible, .load-more:focus-visible, .execution-item:focus-visible, .step-toggle:focus-visible { outline: var(--size-toast-edge) solid var(--color-selection); outline-offset: var(--size-hud-frame-line); }
  .history-body { display: grid; grid-template-columns: minmax(220px, 0.34fr) minmax(0, 1fr); flex: 1 1 auto; min-height: 0; }
  .execution-list { min-height: 0; display: flex; flex-direction: column; border-right: var(--size-hud-frame-line) solid var(--color-hud-line-subtle); }
  .list-heading { min-height: var(--size-touch-target); display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: var(--space-1) var(--space-2); padding: var(--space-2) var(--spacing-widget-inset); border-bottom: var(--size-hud-frame-line) solid var(--color-hud-line-subtle); }
  .pagination-count { font-variant-numeric: tabular-nums; }
  .execution-items, .step-list { min-height: 0; overflow-y: auto; overscroll-behavior: contain; }
  .execution-items { flex: 1; }
  .execution-item { width: 100%; display: flex; align-items: start; gap: var(--spacing-inline-related); padding: var(--space-2) var(--space-3); border: 0; border-bottom: var(--size-hud-frame-line) solid var(--color-hud-line-subtle); border-left: var(--size-toast-edge) solid transparent; background: transparent; color: inherit; text-align: left; cursor: pointer; }
  .execution-item:hover { background: var(--color-surface-hover-subtle); }
  .execution-item.selected { border-left-color: var(--color-selection); background: var(--color-selection-subtle); }
  .status-icon, .execution-state { display: inline-flex; align-items: center; gap: var(--space-1); }
  .status-icon .material-symbols-outlined, .execution-state .material-symbols-outlined { font-size: var(--type-icon-medium); }
  .success { color: var(--color-success); } .danger, .failure { color: var(--color-danger); } .warning { color: var(--color-warning); } .running { color: var(--color-transition); }
  .execution-copy { min-width: 0; display: grid; gap: var(--spacing-label-to-value); }
  .execution-state { font-size: var(--type-caption-compact); }
  .execution-date { color: var(--text-primary); font-size: var(--type-body-compact); }
  .execution-duration { color: var(--text-secondary); font-size: var(--type-caption-compact); }
  .load-more { margin: var(--space-2); flex: 0 0 auto; }
  .execution-detail { min-width: 0; min-height: 0; display: flex; flex-direction: column; padding: var(--spacing-widget-inset); }
  .detail-heading { display: flex; justify-content: space-between; align-items: start; gap: var(--spacing-inline-related); padding-bottom: var(--spacing-field-group); border-bottom: var(--size-hud-frame-line) solid var(--color-hud-line-subtle); }
  .eyebrow { color: var(--text-secondary); font-size: var(--type-caption-compact); letter-spacing: .08em; }
  h2 { margin: var(--spacing-label-to-value) 0 0; color: var(--text-primary); font-size: var(--type-heading); font-weight: 400; overflow-wrap: anywhere; }
  .detail-meta { display: flex; flex-wrap: wrap; gap: var(--space-1) var(--spacing-section); padding: var(--spacing-field-group) 0; color: var(--text-secondary); font-size: var(--type-caption-compact); }
  .step-list { flex: 1; }
  .step { border-top: var(--size-hud-frame-line) solid var(--color-hud-line-subtle); }
  .step-count { padding: var(--space-2) var(--space-1); color: var(--text-secondary); font-size: var(--type-caption-compact); }
  .step-toggle { display: flex; align-items: center; gap: var(--spacing-inline-related); width: 100%; min-height: var(--size-touch-target); padding: var(--space-2) var(--space-1); border: 0; background: transparent; color: var(--text-primary); text-align: left; cursor: pointer; }
  .step-index { color: var(--color-interactive); font-family: var(--font-mono); font-size: var(--type-caption-compact); }
  .step-name { flex: 1; min-width: 0; font-size: var(--type-body-compact); overflow-wrap: anywhere; }
  .disclosure { color: var(--text-secondary); font-size: var(--type-icon-medium); }
  .step-content { display: grid; gap: var(--spacing-field-group); padding: 0 0 var(--spacing-widget-inset) var(--spacing-section); }
  .data-block { min-width: 0; }
  .data-block h3 { margin: 0 0 var(--spacing-label-to-value); color: var(--text-secondary); font-size: var(--type-caption-compact); font-weight: 400; }
  .data-block pre { max-height: var(--size-code-editor-max-height); margin: 0; overflow: auto; white-space: pre-wrap; overflow-wrap: anywhere; padding: var(--space-2); border-left: var(--size-hud-frame-line) solid var(--color-hud-line); background: var(--color-surface-2); color: var(--text-primary); font: var(--type-body-compact)/var(--line-height-body) var(--font-mono); }
  .data-block.failure pre { color: var(--color-danger-hover); }
  .no-steps { color: var(--text-secondary); font-size: var(--type-body-compact); }
  .detail-more { align-self: start; margin: var(--space-2) 0 0; }
  .inline-state { display: flex; flex-direction: column; align-items: center; justify-content: center; gap: var(--spacing-inline-related); min-height: var(--size-toast-min-width); padding: var(--spacing-widget-inset); color: var(--text-secondary); text-align: center; font-size: var(--type-body-compact); overflow-wrap: anywhere; }
  .inline-state .material-symbols-outlined { color: var(--color-interactive); font-size: var(--type-title); }
  .empty { flex: 1; }
  .skeleton-list, .detail-skeleton { display: grid; gap: var(--spacing-inline-related); padding: var(--spacing-field-group); }
  .skeleton { height: var(--size-touch-target); border-radius: var(--radius); background: linear-gradient(90deg, var(--color-surface-2), var(--color-surface-3), var(--color-surface-2)); background-size: 200% 100%; animation: shimmer 1.4s ease infinite; }
  .detail-skeleton .skeleton:nth-child(2) { height: var(--size-toast-min-width); } .detail-skeleton .skeleton:nth-child(3) { height: calc(var(--size-toast-min-width) + var(--space-5)); }
  @keyframes shimmer { to { background-position: -200% 0; } }
  @media (max-width: 700px) { .history-body { grid-template-columns: minmax(0, 1fr); grid-template-rows: minmax(var(--size-toast-recalc-min-width), 0.4fr) minmax(var(--size-toast-min-width), 1fr); } .execution-list { border-right: 0; border-bottom: var(--size-hud-frame-line) solid var(--color-hud-line-subtle); } .execution-items { overflow-x: hidden; } .execution-item { width: 100%; border-left: var(--size-toast-edge) solid transparent; } .execution-item.selected { border-left-color: var(--color-selection); border-bottom-color: var(--color-hud-line-subtle); } }
  @media (prefers-reduced-motion: reduce) { .skeleton { animation: none; } }
</style>
