<script>
  import { onMount, onDestroy } from 'svelte'
  import { invoke } from '@tauri-apps/api/core'
  import { listen } from '@tauri-apps/api/event'
  import ChatMessageBubble from '../ChatMessageBubble.svelte'
  import WidgetContainer from './WidgetContainer.svelte'

  let { widgetId, entityId, conversationIri = null, windowState = 'normal', onWindowStateChange } = $props()

  let detail = $state(null)
  let loading = $state(true)
  let error = $state(null)
  let expandedStep = $state(null)
  let errorDismissed = $state(null)

  let unlisteners = []

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

  function parseContent(contentJson) {
    try {
      const parsed = JSON.parse(contentJson)
      if (Array.isArray(parsed)) return parsed
    } catch {}
    return [{ type: 'text', text: contentJson ?? '' }]
  }

  function toMessageObjects(messages) {
    return messages.map((msg, i) => ({
      iri: `_msg_${i}`,
      role: msg.role,
      content: parseContent(msg.content),
      sentAt: null,
    }))
  }

  function isDisplayable(msg) {
    if (!Array.isArray(msg.content)) return false
    return msg.content.some(b => b.type === 'text' || b.type === 'tool_use')
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

  async function openInspector() {
    await invoke('widget_blackboard__add_widget', {
      widgetType: 'inspector',
      entityId,
      content: null,
      position: null,
      size: null,
      conversationId: conversationIri,
    }).catch(() => {})
  }

  async function closeWidget() {
    await invoke('widget_blackboard__remove_widget', { widgetId }).catch(() => {})
  }

  onMount(async () => {
    await loadDetail()

    if (detail && isInProgress(detail.status_label)) {
      const unlistenProgress = await listen('automation-step-progress', async (event) => {
        if (event.payload.executionIri === entityId) {
          await loadDetail()
        }
      })
      const unlistenFinished = await listen('automation-execution-finished', async (event) => {
        if (event.payload.executionIri === entityId) {
          await loadDetail()
        }
      })
      const unlistenMessage = await listen('automation-step-message', (event) => {
        if (event.payload.executionIri !== entityId || !detail) return
        const { stepIri, role, content } = event.payload
        const stepIndex = detail.steps.findIndex(s => s.iri === stepIri)
        if (stepIndex === -1) return
        const msg = { role, content }
        detail.steps[stepIndex].messages = [...detail.steps[stepIndex].messages, msg]
        detail = detail
        expandedStep = stepIri
      })
      unlisteners.push(unlistenProgress, unlistenFinished, unlistenMessage)
    }
  })

  onDestroy(() => {
    unlisteners.forEach(fn => fn())
  })
</script>

<WidgetContainer
  icon="play_circle"
  title={detail?.process_label ?? 'Execution'}
  {windowState}
  {onWindowStateChange}
  onClose={closeWidget}
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
        style="color: {detail.status_color ?? '#94A3B8'}"
      >
        <span class="material-symbols-outlined status-icon" class:spinning={isInProgress(detail.status_label)}>{detail.status_icon ?? 'help'}</span>
        <span class="status-label">{detail.status_label}</span>
      </span>
    {/if}
    <button class="action-btn" onclick={openInspector} title="Open inspector">
      <span class="material-symbols-outlined">info</span>
    </button>
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
      {#if detail.error_message && errorDismissed !== detail.error_message}
        <div class="execution-error">
          <span class="material-symbols-outlined">error</span>
          <span class="error-message-text">{detail.error_message}</span>
          <button class="error-dismiss" onclick={() => errorDismissed = detail.error_message} title="Dismiss">
            <span class="material-symbols-outlined">close</span>
          </button>
        </div>
      {/if}

      {#if detail.steps.length === 0}
        <div class="center-state muted">No steps recorded yet…</div>
      {:else}
        <div class="steps">
          {#each detail.steps as step (step.iri)}
            {@const isExpanded = expandedStep === step.iri}
            {@const duration = formatDuration(step.started_at, step.finished_at)}
            <div class="step" class:expanded={isExpanded}>
              <button
                class="step-header"
                onclick={() => expandedStep = isExpanded ? null : step.iri}
              >
                <span
                  class="material-symbols-outlined step-icon"
                  class:spinning={isInProgress(step.status_label)}
                  style="color: {step.status_color ?? '#94A3B8'}"
                >{step.status_icon ?? 'radio_button_unchecked'}</span>
                <span class="material-symbols-outlined step-type-icon">{nodeIcon(step.node_type)}</span>
                <span class="step-label">{step.node_label}</span>
                <span class="step-duration">{duration ?? ''}</span>
                {#if step.error || step.output || step.messages.length > 0}
                  <span class="material-symbols-outlined step-chevron" class:rotated={isExpanded}>
                    expand_more
                  </span>
                {/if}
              </button>

              {#if isExpanded}
                <div class="step-detail">
                  {#if step.error}
                    <div class="detail-section error-section">
                      <span class="detail-label">Error</span>
                      <pre class="detail-content error-text">{step.error}</pre>
                    </div>
                  {/if}
                  {#if step.output}
                    <div class="detail-section">
                      <span class="detail-label">Output</span>
                      <pre class="detail-content">{step.output}</pre>
                    </div>
                  {/if}
                  {#if step.messages.length > 0}
                    {@const msgObjects = toMessageObjects(step.messages)}
                    <div class="detail-section">
                      <span class="detail-label">Conversation</span>
                      <div class="messages">
                        {#each msgObjects.filter(isDisplayable) as msg (msg.iri)}
                          <ChatMessageBubble message={msg} messages={msgObjects} />
                        {/each}
                      </div>
                    </div>
                  {/if}
                  {#if step.started_at}
                    <div class="detail-meta">
                      <span>{formatTime(step.started_at)}</span>
                      {#if step.finished_at}
                        <span>→ {formatTime(step.finished_at)}</span>
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
    background: color-mix(in srgb, #EF4444 12%, transparent);
    border: 1px solid color-mix(in srgb, #EF4444 30%, transparent);
    font-size: 12px;
    color: #FCA5A5;
  }

  .execution-error .material-symbols-outlined {
    font-size: 16px;
    flex-shrink: 0;
    margin-top: 1px;
  }

  .error-message-text {
    flex: 1;
  }

  .error-dismiss {
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    color: #FCA5A5;
    opacity: 0.6;
    display: flex;
    align-items: center;
    flex-shrink: 0;
    transition: opacity 0.15s;
  }

  .error-dismiss:hover {
    opacity: 1;
  }

  .error-dismiss .material-symbols-outlined {
    font-size: 14px;
    margin-top: 0;
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
    border: 1px solid color-mix(in srgb, var(--color-white) 8%, transparent);
  }

  .step.expanded {
    border-color: color-mix(in srgb, var(--color-interactive) 30%, transparent);
  }

  .step-header {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 8px 10px;
    background: none;
    border: none;
    cursor: pointer;
    text-align: left;
    color: var(--color-neutral);
    transition: background 0.1s;
  }

  .step-header:hover {
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
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
    border: 1px solid color-mix(in srgb, var(--color-white) 8%, transparent);
    padding: 8px;
    white-space: pre-wrap;
    word-break: break-word;
    margin: 0;
    font-family: var(--font-mono, monospace);
    max-height: 120px;
    overflow-y: auto;
  }

  .error-section .detail-content {
    color: #FCA5A5;
    border-color: color-mix(in srgb, #EF4444 25%, transparent);
    background: color-mix(in srgb, #EF4444 8%, transparent);
  }

  .error-text {
    color: #FCA5A5;
  }

  .messages {
    display: flex;
    flex-direction: column;
    gap: 4px;
    max-height: 400px;
    overflow-y: auto;
    padding: 4px 0;
  }

  .detail-meta {
    display: flex;
    gap: 8px;
    font-size: 10px;
    color: var(--color-neutral-disabled);
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  .spinning {
    animation: spin 1s linear infinite;
  }
</style>
