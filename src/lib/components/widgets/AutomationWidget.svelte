<script>
  import { onMount, onDestroy } from 'svelte'
  import { invoke } from '@tauri-apps/api/core'
  import { listen } from '@tauri-apps/api/event'
  import { SvelteFlow, Controls, Background, BackgroundVariant } from '@xyflow/svelte'
  import '@xyflow/svelte/dist/style.css'
  import { applyDagreLayout } from './automation/layout.js'
  import NodeStartEvent        from './automation/NodeStartEvent.svelte'
  import NodeTimerStartEvent   from './automation/NodeTimerStartEvent.svelte'
  import NodeEndEvent     from './automation/NodeEndEvent.svelte'
  import NodeAgentTask    from './automation/NodeAgentTask.svelte'
  import NodeCodeTask     from './automation/NodeCodeTask.svelte'
  import NodeScriptTask   from './automation/NodeScriptTask.svelte'
  import NodeRequestTask  from './automation/NodeRequestTask.svelte'
  import NodeUserTask     from './automation/NodeUserTask.svelte'
  import NodeGateway      from './automation/NodeGateway.svelte'
  import NodeSubProcess        from './automation/NodeSubProcess.svelte'
  import NodeNOVAMessageTask   from './automation/NodeNOVAMessageTask.svelte'

  let { widgetId, entityId, windowState = 'normal', onWindowStateChange, conversationIri = null } = $props()

  const nodeTypes = {
    automation_StartEvent:       NodeStartEvent,
    automation_TimerStartEvent:  NodeTimerStartEvent,
    automation_EndEvent:         NodeEndEvent,
    automation_AgentTask:        NodeAgentTask,
    automation_CodeTask:         NodeCodeTask,
    automation_ScriptTask:       NodeScriptTask,
    automation_RequestTask:      NodeRequestTask,
    automation_UserTask:         NodeUserTask,
    automation_Gateway:          NodeGateway,
    automation_SubProcess:       NodeSubProcess,
    automation_NOVAMessageTask:  NodeNOVAMessageTask,
  }

  let automationLabel = $state('')
  let nodes = $state.raw([])
  let edges = $state.raw([])
  let loading = $state(true)
  let error = $state(null)
  const expanded = $derived(windowState === 'maximized')
  let hoveredNodeId = $state(null)
  let running = $state(false)

  let execNodeStatus = $state(new Map())
  let activeExecutionIri = $state(null)
  let activeStepLabel = $state(null)

  const nodesWithExecStatus = $derived(
    execNodeStatus.size === 0
      ? nodes
      : nodes.map(n => {
          const s = execNodeStatus.get(n.id)
          if (!s) return n
          return { ...n, data: { ...n.data, status: s.statusLabel, statusColor: s.statusColor, statusIcon: s.statusIcon } }
        })
  )

  const displayEdges = $derived(
    hoveredNodeId === null
      ? edges
      : edges.map(e => {
          const connected = e.source === hoveredNodeId || e.target === hoveredNodeId
          return { ...e, class: connected ? 'edge-highlighted' : undefined }
        })
  )

  let watchedIris = new Set()
  let unlisten = null
  let unlistenExecStarted = null
  let unlistenStepProgress = null
  let unlistenExecFinished = null

  async function loadGraph() {
    loading = true
    error = null
    execNodeStatus = new Map()
    activeStepLabel = null
    try {
      const raw = await invoke('automation__get_graph', { automationIri: entityId })
      const data = JSON.parse(raw)
      automationLabel = data.process_label

      const flowNodes = data.nodes.map(n => ({
        id: n.id,
        type: n.type,
        data: {
          label: n.label,
          nodeType: n.type,
          assignedAgent: n.assigned_agent ?? null,
          invokesProcess: n.invokes_process ?? null,
          status: n.status ?? null,
          statusColor: n.status_color ?? null,
          statusIcon: n.status_icon ?? null,
          inputConceptLabel: n.input_concept_label ?? null,
          inputConceptIcon: n.input_concept_icon ?? null,
          outputConceptLabel: n.output_concept_label ?? null,
          outputConceptIcon: n.output_concept_icon ?? null,
          messagePayload: n.message_payload ?? null,
          usesTools: n.uses_tools ?? [],
          assignedToRoles: n.assigned_to_roles ?? [],
          assignedToUsers: n.assigned_to_users ?? [],
          outputConcepts: n.output_concepts ?? [],
          timerCycle: n.timer_cycle ?? null,
        },
        position: { x: 0, y: 0 },
      }))

      const gatewayIds = new Set(data.nodes.filter(n => n.type === 'automation_Gateway').map(n => n.id))

      const flowEdges = data.edges.map(e => {
        const isErrorEdge = e.source_handle === 'error'
        const conditionLabel = !isErrorEdge && gatewayIds.has(e.source) ? (e.condition_expression ?? null) : null
        return {
          id: e.id,
          source: e.source,
          target: e.target,
          type: 'default',
          animated: !isErrorEdge,
          sourceHandle: isErrorEdge ? undefined : (e.source_handle ?? undefined),
          style: isErrorEdge ? 'stroke:#ef4444;stroke-dasharray:5,4;stroke-width:1.5px;' : undefined,
          label: isErrorEdge ? (e.label ?? 'on error') : (conditionLabel ?? undefined),
          labelStyle: isErrorEdge
            ? 'background:#450a0a;color:#fca5a5;padding:2px 7px;border-radius:4px;border:1px solid #ef4444;font-size:11px;font-weight:500;'
            : conditionLabel ? 'background:#1e293b;color:#f1f5f9;padding:2px 7px;border-radius:4px;border:1px solid #475569;font-size:11px;font-weight:500;' : undefined,
        }
      })

      nodes = applyDagreLayout(flowNodes, flowEdges)
      edges = flowEdges
      const iris = new Set([entityId, ...data.nodes.map(n => n.id), ...(data.sequence_flow_iris ?? [])])
      for (const n of data.nodes.filter(n => n.invokes_process)) {
        iris.add(n.invokes_process)
        try {
          const subRaw = await invoke('automation__get_graph', { automationIri: n.invokes_process })
          const subData = JSON.parse(subRaw)
          subData.nodes.forEach(sn => iris.add(sn.id))
        } catch {}
      }
      watchedIris = iris
    } catch (e) {
      error = String(e)
    } finally {
      loading = false
    }
  }

  async function handleNodeClick({ node, event }) {
    if (event?.metaKey && node.data.invokesProcess) {
      await invoke('widget_blackboard__add_widget', {
        widgetType: 'inspector',
        entityId: node.id,
        content: null,
        position: null,
        size: null,
        conversationId: conversationIri,
      }).catch(e => console.error('Failed to open inspector:', e))
    } else if (node.data.invokesProcess) {
      await invoke('widget_blackboard__add_widget', {
        widgetType: 'automation',
        entityId: node.data.invokesProcess,
        content: null,
        position: null,
        size: null,
        conversationId: conversationIri,
      }).catch(e => console.error('Failed to open sub-process widget:', e))
    } else {
      await invoke('widget_blackboard__add_widget', {
        widgetType: 'inspector',
        entityId: node.id,
        content: null,
        position: null,
        size: null,
        conversationId: conversationIri,
      }).catch(e => console.error('Failed to open inspector:', e))
    }
  }

  function toggleMinimize() {
    onWindowStateChange?.(windowState === 'minimized' ? 'normal' : 'minimized')
  }

  function toggleExpanded() {
    onWindowStateChange?.(expanded ? 'normal' : 'maximized')
  }

  async function runAutomation() {
    if (running) return
    running = true
    try {
      await invoke('automation__run', { automationIri: entityId })
    } catch (e) {
      console.error('Failed to start automation:', e)
    } finally {
      running = false
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
    await loadGraph()
    unlisten = await listen('entity-updated', async (event) => {
      if (watchedIris.has(event.payload.entityId)) await loadGraph()
    })
    unlistenExecStarted = await listen('automation-execution-started', (event) => {
      if (event.payload.processIri === entityId) {
        activeExecutionIri = event.payload.executionIri
        execNodeStatus = new Map()
        activeStepLabel = null
        running = true
      }
    })
    unlistenStepProgress = await listen('automation-step-progress', (event) => {
      if (event.payload.executionIri !== activeExecutionIri) return
      const { nodeIri, nodeLabel, status } = event.payload
      const map = new Map(execNodeStatus)
      if (status === 'started') {
        map.set(nodeIri, { statusLabel: 'Running', statusColor: '#F59E0B', statusIcon: 'progress_activity' })
        activeStepLabel = nodeLabel
      } else if (status === 'completed') {
        map.set(nodeIri, { statusLabel: 'Done', statusColor: '#22C55E', statusIcon: 'check_circle' })
        activeStepLabel = null
      } else if (status === 'failed') {
        map.set(nodeIri, { statusLabel: 'Failed', statusColor: '#EF4444', statusIcon: 'error' })
        activeStepLabel = null
      }
      execNodeStatus = map
    })
    unlistenExecFinished = await listen('automation-execution-finished', (event) => {
      if (event.payload.executionIri !== activeExecutionIri) return
      running = false
      activeStepLabel = null
    })
  })

  onDestroy(() => {
    if (unlisten) unlisten()
    if (unlistenExecStarted) unlistenExecStarted()
    if (unlistenStepProgress) unlistenStepProgress()
    if (unlistenExecFinished) unlistenExecFinished()
  })
</script>

<div class="automation-widget" class:minimized={windowState === 'minimized'}>
  <div class="widget-header">
    <div class="header-left">
      <span class="material-symbols-outlined header-icon">bolt</span>
      <span class="header-title">{automationLabel || 'Automation'}</span>
      {#if activeStepLabel}
        <span class="active-step-label">{activeStepLabel}…</span>
      {/if}
    </div>
    <div class="header-actions">
      <button class="action-btn run-btn" onclick={runAutomation} disabled={running} title="Run automation">
        <span class="material-symbols-outlined">{running ? 'progress_activity' : 'play_arrow'}</span>
      </button>
      <button class="action-btn" onclick={toggleExpanded} title={expanded ? 'Restore' : 'Expand'}>
        <span class="material-symbols-outlined">{expanded ? 'close_fullscreen' : 'open_in_full'}</span>
      </button>
      <button class="action-btn" onclick={toggleMinimize} title={windowState === 'minimized' ? 'Expand' : 'Minimize'}>
        <span class="material-symbols-outlined">{windowState === 'minimized' ? 'expand_more' : 'expand_less'}</span>
      </button>
      <button class="action-btn" onclick={openInspector} title="Open inspector">
        <span class="material-symbols-outlined">info</span>
      </button>
      <button class="close-btn" onclick={closeWidget}>
        <span class="material-symbols-outlined">close</span>
      </button>
    </div>
  </div>

  <div class="content-wrapper">
    <div class="widget-content">
      {#if loading}
        <div class="loading">
          <span class="material-symbols-outlined spinning">progress_activity</span>
        </div>
      {:else if error}
        <div class="error-state">
          <span class="material-symbols-outlined">error</span>
          <p>{error}</p>
        </div>
      {:else}
        <SvelteFlow
          {nodes}
          edges={displayEdges}
          {nodeTypes}
          fitView
          panOnScroll
          zoomOnScroll={false}
          onnodeclick={handleNodeClick}
          onnodepointerenter={({ node }) => hoveredNodeId = node.id}
          onnodepointerleave={() => hoveredNodeId = null}
        >
          <Controls />
          <Background variant={BackgroundVariant.Dots} gap={20} size={1} color="rgba(255,255,255,0.08)" />
        </SvelteFlow>
      {/if}
    </div>
  </div>
</div>


<style>
  .automation-widget {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    background: color-mix(in srgb, var(--color-black) 85%, transparent);
    backdrop-filter: blur(20px);
    border: 1px solid color-mix(in srgb, var(--color-white) 20%, transparent);
    border-radius: 12px;
    overflow: hidden;
    box-shadow: 0 8px 32px color-mix(in srgb, var(--color-black) 40%, transparent);
  }

  .widget-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    border-bottom: 1px solid color-mix(in srgb, var(--color-white) 15%, transparent);
    flex-shrink: 0;
  }

  .header-left {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .header-icon {
    font-size: 22px;
    color: var(--color-interactive);
  }

  .header-title {
    font-family: var(--font-title);
    font-size: 11px;
    font-weight: 600;
    color: var(--color-neutral-active);
  }

  .active-step-label {
    font-size: 10px;
    color: #F59E0B;
    font-style: italic;
    max-width: 140px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .action-btn {
    background: none;
    border: none;
    padding: 4px;
    cursor: pointer;
    color: var(--color-interactive);
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.2s;
  }

  .action-btn:hover {
    background: color-mix(in srgb, var(--color-interactive) 15%, transparent);
    color: var(--color-neutral-active);
  }

  .run-btn {
    color: #43A047;
  }
  .run-btn:hover {
    background: color-mix(in srgb, #43A047 15%, transparent);
    color: #66BB6A;
  }
  .run-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .run-btn:disabled .material-symbols-outlined {
    animation: spin 1s linear infinite;
  }

  .action-btn .material-symbols-outlined {
    font-size: 18px;
  }

  .close-btn {
    background: none;
    border: none;
    padding: 4px;
    cursor: pointer;
    color: var(--color-interactive);
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.2s;
  }

  .close-btn:hover {
    background: color-mix(in srgb, var(--color-interactive) 15%, transparent);
    color: var(--color-neutral-active);
  }

  .close-btn .material-symbols-outlined {
    font-size: 20px;
  }

  .content-wrapper {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-rows: 1fr;
    transition: grid-template-rows 250ms cubic-bezier(0.4, 0, 0.2, 1);
    overflow: hidden;
  }

  .minimized .content-wrapper {
    grid-template-rows: 0fr;
  }

  .content-wrapper > .widget-content {
    min-height: 0;
  }

  .widget-content {
    height: 100%;
    overflow: hidden;
    position: relative;
  }

  .loading {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--color-neutral);
    opacity: 0.5;
  }

  .loading .material-symbols-outlined {
    font-size: 32px;
  }

  .spinning {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  .error-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 8px;
    color: var(--color-error, #ef4444);
    font-size: 13px;
    padding: 24px;
    text-align: center;
  }

  .error-state .material-symbols-outlined {
    font-size: 32px;
  }


  :global(.svelte-flow) {
    background: transparent !important;
    --xy-edge-label-background-color: #000000;
    --xy-edge-label-color: #f1f5f9;
  }

  :global(.svelte-flow .svelte-flow__node) {
    font-family: var(--font-title, sans-serif);
  }

  :global(.svelte-flow__edge.edge-highlighted .svelte-flow__edge-path) {
    stroke: #3b82f6 !important;
    stroke-width: 2.5px !important;
    opacity: 1 !important;
  }
</style>
