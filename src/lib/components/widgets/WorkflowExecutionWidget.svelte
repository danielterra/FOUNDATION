<script>
  import { onMount, onDestroy } from 'svelte'
  import { invoke } from '@tauri-apps/api/core'
  import { listen } from '@tauri-apps/api/event'
  import { createEntitySubscription } from '$lib/realtime/subscriptions'
  import MarkdownValue from './inspector/MarkdownValue.svelte'
  import WidgetContainer from './WidgetContainer.svelte'
  import { Button } from '$lib/components/ui/button'
  import { openEntity } from '$lib/blackboard'

  let { widgetId, entityId, conversationIri = null, windowState = 'normal', onWindowStateChange } = $props()

  let detail = $state(null)
  let loading = $state(true)
  let error = $state(null)
  let expandedStep = $state(null)
  let errorDismissed = $state(null)
  let materializationByStep = $state(new Map())

  let unlisteners = []
  const entitySub = createEntitySubscription((event) => {
    if (event.type === 'updated') loadDetail()
  })

  const NODE_TYPE_ICONS = {
    automation_AgentTask:       'assignment_ind',
    automation_CodeTask:        'code',
    automation_ScriptTask:      'terminal',
    automation_RequestTask:     'http',
    automation_UserTask:        'person',
    automation_NOVAMessageTask: 'chat',
    automation_StartEvent:      'play_circle',
    automation_EndEvent:        'stop_circle',
    automation_Gateway:         'alt_route',
    automation_SubProcess:      'account_tree',
  }

  function nodeIcon(nodeType) {
    return NODE_TYPE_ICONS[nodeType] ?? 'smart_toy'
  }

  function formatDuration(startedAt, finishedAt) {
    if (!startedAt) return null
    const start = new Date(startedAt).getTime()
    const end = finishedAt ? new Date(finishedAt).getTime() : Date.now()
    const ms = end - start
    if (ms < 1000) return `${ms}ms`
    return `${(ms / 1000).toFixed(1)}s`
  }

  function formatTime(iso) {
    if (!iso) return ''
    return new Date(iso).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit' })
  }

  function executionStartedAt(iri) {
    const match = iri.match(/_(\d+)$/)
    if (!match) return null
    return new Date(parseInt(match[1])).toLocaleString([], {
      month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit'
    })
  }

  function isInProgress(statusLabel) {
    return statusLabel === 'In Progress' || statusLabel === 'InProgress'
  }

  async function loadDetail() {
    try {
      const raw = await invoke('automation__get_execution', { executionIri: entityId })
      detail = JSON.parse(raw)
      error = null
    } catch (e) {
      error = String(e)
    } finally {
      loading = false
    }
  }

  function describeMaterialization(progress) {
    if (progress.status === 'failed') return progress.reason || 'Falha ao atualizar os dados necessários'
    if (progress.status === 'cancelled') return progress.reason || 'Atualização dos dados cancelada'
    if (progress.status === 'completed') return 'Dados atualizados'
    if (progress.status === 'waiting') return 'Aguardando confirmação dos dados necessários'
    if (progress.phase === 'query-properties') return 'Atualizando propriedades relacionadas'
    return 'Atualizando propriedades calculadas'
  }

  function hasDeterminateProgress(progress) {
    return progress.completedCount !== undefined
      && progress.totalCount !== undefined
      && progress.totalCount > 0
      && progress.completedCount >= 0
      && progress.completedCount <= progress.totalCount
  }

  function handleMaterializationProgress(event) {
    const progress = event.payload
    if (progress.executionIri !== entityId) return
    if (!progress.phase) {
      loadDetail()
      return
    }

    const next = new Map(materializationByStep)
    next.set(progress.stepIri, progress)
    materializationByStep = next
    loadDetail()
  }

  function handleExecutionFinished(event) {
    if (event.payload.executionIri === entityId) loadDetail()
  }

  async function openControlInstanceInspector() {
    if (!detail?.controlInstanceIri) return
    await invoke('widget_blackboard__add_widget', {
      widgetType: 'inspector',
      entityId: detail.controlInstanceIri,
      content: null,
      position: null,
      size: null,
      conversationId: conversationIri,
    }).catch(() => {})
  }

  async function openRelatedEntity(iri) {
    await openEntity(iri, conversationIri).catch(() => {})
  }

  async function closeWidget() {
    await invoke('widget_blackboard__remove_widget', { widgetId }).catch(() => {})
  }

  onMount(async () => {
    await loadDetail()

    const unlistenProgress = await listen('automation-step-progress', handleMaterializationProgress)
    const unlistenFinished = await listen('automation-execution-finished', handleExecutionFinished)
    unlisteners.push(unlistenProgress, unlistenFinished)
  })

  onDestroy(() => {
    unlisteners.forEach(fn => fn())
    entitySub.destroy()
  })

  $effect(() => {
    entitySub.setIris(entityId ? [entityId] : [])
  })
</script>

<WidgetContainer
  icon="play_circle"
  title={detail?.processLabel ?? 'Execution'}
  {windowState}
  {onWindowStateChange}
  onClose={closeWidget}
  {entityId}
  {conversationIri}
>
  {#snippet headerExtra()}
    {#if detail}
      <span class="header-sub">{executionStartedAt(entityId) ?? ''}</span>
    {/if}
  {/snippet}

  {#snippet headerActions()}
    {#if detail}
      <span
        class="status-badge"
      style="color: {detail.statusColor ?? 'var(--color-neutral-disabled)'}"
      >
        <span class="material-symbols-outlined status-icon" class:spinning={isInProgress(detail.statusLabel)}>{detail.statusIcon ?? 'help'}</span>
        <span class="status-label">{detail.statusLabel}</span>
      </span>
    {/if}
  {/snippet}

  <div class="body">
    {#if loading}
      <div class="center-state">
        <span class="material-symbols-outlined spinning">progress_activity</span>
      </div>
    {:else if error}
      <div class="center-state error-text">
        <span class="material-symbols-outlined">error</span>
        <p>{error}</p>
      </div>
    {:else if detail}
      {#if detail.errorMessage && errorDismissed !== detail.errorMessage}
        <div class="execution-error">
          <span class="material-symbols-outlined">error</span>
          <span class="error-message-text">{detail.errorMessage}</span>
          <Button variant="ghost" size="icon" class="error-dismiss-btn" aria-label="Dispensar erro" onclick={() => errorDismissed = detail.errorMessage} title="Dismiss"><span class="material-symbols-outlined">close</span></Button>
        </div>
      {/if}

      {#if detail.steps.length === 0}
        <div class="center-state muted">Nenhum passo registrado ainda…</div>
      {:else}
        <div class="steps" role="list" aria-label="Passos da execução">
          {#each detail.steps as step (step.iri)}
            {@const isExpanded = expandedStep === step.iri}
            {@const duration = formatDuration(step.startedAt, step.finishedAt)}
            {@const materialization = materializationByStep.get(step.iri)}
            <div class="step" class:expanded={isExpanded} role="listitem">
              <Button
                variant="ghost"
                class="step-header"
                aria-expanded={isExpanded}
                aria-label={materialization ? `${step.nodeLabel}. ${describeMaterialization(materialization)}` : step.nodeLabel}
                onclick={() => expandedStep = isExpanded ? null : step.iri}
                onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); expandedStep = isExpanded ? null : step.iri } }}
              >
                <span
                  class="material-symbols-outlined step-icon"
                  class:spinning={isInProgress(step.statusLabel)}
                  style="color: {step.statusColor ?? 'var(--color-neutral-disabled)'}"
                >{step.statusIcon ?? 'radio_button_unchecked'}</span>
                <span class="material-symbols-outlined step-type-icon">{nodeIcon(step.nodeType)}</span>
                <span class="step-label">{step.nodeLabel}</span>
                {#if materialization}
                  <span class="materialization-phase" aria-live="off">
                    <span class="material-symbols-outlined" class:spinning={materialization.status === 'waiting' || materialization.status === 'running'}>
                      {materialization.status === 'failed' ? 'error' : materialization.status === 'cancelled' ? 'cancel' : 'sync'}
                    </span>
                    <span role="status" aria-live={materialization.status === 'failed' || materialization.status === 'cancelled' ? 'assertive' : 'polite'}>{describeMaterialization(materialization)}</span>
                    {#if hasDeterminateProgress(materialization)}
                      <span>{materialization.completedCount}/{materialization.totalCount}</span>
                    {/if}
                  </span>
                {/if}
                <span class="step-duration">{duration ?? ''}</span>
                {#if step.inputContext || step.executionOutput?.length > 0 || step.executionSummary || step.stepError}
                  <span class="material-symbols-outlined step-chevron" class:rotated={isExpanded}>
                    expand_more
                  </span>
                {/if}
              </Button>

              {#if isExpanded}
                <div class="step-detail">
                  {#if step.inputContext}
                    <div class="detail-section">
                      <span class="detail-label">Entrada</span>
                      <pre class="detail-content">{step.inputContext}</pre>
                    </div>
                  {/if}
                  {#if step.stepError}
                    <div class="detail-section error-section">
                      <span class="detail-label">Erro</span>
                      <pre class="detail-content error-text">{step.stepError}</pre>
                    </div>
                  {/if}
                  {#if step.executionOutput?.length > 0 || step.executionSummary}
                    <div class="detail-section">
                      <span class="detail-label">Saída</span>
                      <div class="detail-content detail-content-output">
                        {#if step.executionOutput?.length > 0}
                          {#each step.executionOutput as output, outputIndex (outputIndex)}
                            <MarkdownValue value={output} openEntityInspector={openRelatedEntity} />
                          {/each}
                        {/if}
                        {#if step.executionSummary}
                          <MarkdownValue value={step.executionSummary} openEntityInspector={openRelatedEntity} />
                        {/if}
                      </div>
                    </div>
                  {/if}
                  {#if step.startedAt}
                    <div class="detail-meta">
                      <span>{formatTime(step.startedAt)}</span>
                      {#if step.finishedAt}
                        <span>→ {formatTime(step.finishedAt)}</span>
                      {/if}
                    </div>
                  {/if}
                </div>
              {/if}
            </div>
          {/each}
        </div>
      {/if}
    {/if}
  </div>

  {#if detail?.controlInstanceIri}
    <div class="widget-footer">
      <Button variant="ghost" class="footer-btn" aria-label="Abrir inspetor da instância de controle" onclick={openControlInstanceInspector}>
        <span class="material-symbols-outlined">open_in_new</span>
        Abrir instância de controle
      </Button>
    </div>
  {/if}
</WidgetContainer>

<style>
  .header-sub {
    font-size: 10px;
    color: var(--color-neutral-disabled);
    margin-left: 2px;
  }

  .status-badge {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 10px;
    font-weight: 600;
  }

  .status-icon {
    font-size: 14px;
  }

  .status-label {
    white-space: nowrap;
  }

  .body {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
  }

  .center-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 8px;
    color: var(--color-neutral);
    font-size: 14px;
    padding: 24px;
    text-align: center;
  }

  .center-state .material-symbols-outlined {
    font-size: 32px;
  }

  .muted {
    color: var(--color-neutral-disabled);
  }

  .execution-error {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    margin: 12px;
    padding: 10px 12px;
    background: color-mix(in srgb, var(--color-danger) 12%, transparent);
    font-size: 12px;
    color: var(--color-danger-hover);
  }

  .execution-error .material-symbols-outlined {
    font-size: 16px;
    flex-shrink: 0;
    margin-top: 1px;
  }

  .error-message-text {
    flex: 1;
  }

  :global(.error-dismiss-btn) {
    color: var(--color-danger-hover) !important;
    opacity: 0.6;
    flex-shrink: 0;
  }

  :global(.error-dismiss-btn:hover) {
    opacity: 1;
  }

  :global(.error-dismiss-btn .material-symbols-outlined) {
    font-size: 14px !important;
  }

  .steps {
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .step {
    overflow: hidden;
    background: color-mix(in srgb, var(--color-white) 3%, transparent);
  }

  :global([data-slot="button"].step-header) {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px 10px;
    height: auto;
    justify-content: flex-start;
    color: var(--color-neutral);
    transition: background 0.1s;
    border-radius: 0;
  }

  :global([data-slot="button"].step-header:hover) {
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    color: var(--color-neutral);
  }

  .step-icon {
    font-size: 14px;
    flex-shrink: 0;
  }

  .step-type-icon {
    font-size: 14px;
    flex-shrink: 0;
    color: var(--color-neutral-disabled);
  }

  .step-label {
    flex: 1;
    font-size: 12px;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .materialization-phase {
    display: inline-flex;
    align-items: center;
    gap: var(--spacing-inline-tight);
    color: var(--color-transition);
    font-size: var(--type-caption-compact);
    min-width: 0;
    max-width: 40%;
  }

  .materialization-phase .material-symbols-outlined {
    font-size: var(--type-icon-small);
    flex-shrink: 0;
  }

  .materialization-phase :global(span:not(.material-symbols-outlined)) {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .step-duration {
    font-size: 10px;
    color: var(--color-neutral-disabled);
    flex-shrink: 0;
  }

  .step-chevron {
    font-size: 16px;
    color: var(--color-neutral-disabled);
    flex-shrink: 0;
    transition: transform 0.15s;
  }

  .step-chevron.rotated {
    transform: rotate(180deg);
  }

  .step-detail {
    padding: 0 10px 10px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .detail-section {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .detail-label {
    font-size: 10px;
    font-weight: 600;
    color: var(--color-neutral-disabled);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .detail-content {
    font-size: 11px;
    color: var(--color-neutral);
    background: color-mix(in srgb, var(--color-white) 4%, transparent);
    padding: 8px;
    white-space: pre-wrap;
    word-break: break-word;
    margin: 0;
    font-family: var(--font-mono, monospace);
    max-height: 120px;
    overflow-y: auto;
  }

  .detail-content-output {
    font-family: inherit;
    white-space: normal;
    display: flex;
    flex-direction: column;
  }

  .error-section .detail-content {
    color: var(--color-danger-hover);
    background: color-mix(in srgb, var(--color-danger) 8%, transparent);
  }

  .error-text {
    color: var(--color-danger-hover);
  }

  .detail-meta {
    display: flex;
    gap: 8px;
    font-size: 10px;
    color: var(--color-neutral-disabled);
  }

  .widget-footer {
    flex-shrink: 0;
    display: flex;
    padding: 0;
    background: color-mix(in srgb, var(--color-interactive) 8%, transparent);
  }

  :global(.widget-footer .footer-btn) {
    width: 100%;
    justify-content: flex-start;
    color: var(--color-interactive) !important;
    font-size: 12px;
    font-weight: 600;
    padding: 8px 12px;
    height: auto;
  }

  :global(.widget-footer .footer-btn .material-symbols-outlined) {
    font-size: 16px !important;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  .spinning {
    animation: spin 1s linear infinite;
  }
</style>
