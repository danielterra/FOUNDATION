<script>
  import { onMount, onDestroy } from 'svelte'
  import { invoke } from '@tauri-apps/api/core'
  import { listen } from '@tauri-apps/api/event'
  import { SvelteFlow, Controls, Background, BackgroundVariant } from '@xyflow/svelte'
  import '@xyflow/svelte/dist/style.css'
  import { applyDagreLayout } from './automation/layout.js'
  import WidgetContainer from './WidgetContainer.svelte'
  import Button from '../Button.svelte'
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
  import NodeTask              from './automation/NodeTask.svelte'

  let { widgetId, entityId, windowState = 'normal', onWindowStateChange, conversationIri = null } = $props()

  const nodeTypes = {
    bpmn_StartEvent:      NodeStartEvent,
    bpmn_EndEvent:        NodeEndEvent,
    bpmn_Gateway:         NodeGateway,
    bpmn_Task:            NodeTask,
    bpmn_SubProcess:      NodeSubProcess,
    // automation-specific subtypes resolved via bpmn_* traversal but kept for
    // backward compat with any cached widget state that still carries the old type
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
  let isExecutable = $state(false)
  let nodes = $state.raw([])
  let edges = $state.raw([])
  let loading = $state(true)
  let error = $state(null)
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
      const raw = await invoke('automation__get_graph', { processIri: entityId })
      const data = JSON.parse(raw)
      automationLabel = data.process_label
      isExecutable = data.is_executable ?? false

      const flowNodes = data.nodes.map(n => ({
        id: n.id,
        type: n.type,
        data: {
          label: n.label,
          nodeType: n.type,
          assignedAgent: n.assigned_agent ?? null,
          assignedAgentIri: n.assigned_agent_iri ?? null,
          invokesProcess: n.invokes_process ?? null,
          status: n.status ?? null,
          statusColor: n.status_color ?? null,
          statusIcon: n.status_icon ?? null,
          inputClassLabel: n.input_class_label ?? null,
          inputClassIcon: n.input_class_icon ?? null,
          outputClassLabel: n.output_class_label ?? null,
          outputClassIcon: n.output_class_icon ?? null,
          messagePayload: n.message_payload ?? null,
          usesTools: n.uses_tools ?? [],
          assignedToRoles: n.assigned_to_roles ?? [],
          assignedToUsers: n.assigned_to_users ?? [],
          outputClasses: n.output_classes ?? [],
          timerCycle: n.timer_cycle ?? null,
        },
        position: { x: 0, y: 0 },
      }))

      const gatewayIds = new Set(data.nodes.filter(n => n.type === 'bpmn_Gateway' || n.type === 'automation_Gateway').map(n => n.id))

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
            ? 'background:#450a0a;color:#fca5a5;padding:2px 7px;border:1px solid #ef4444;font-size:11px;font-weight:500;'
            : conditionLabel ? 'background:#1e293b;color:#f1f5f9;padding:2px 7px;border:1px solid #475569;font-size:11px;font-weight:500;' : undefined,
        }
      })

      nodes = applyDagreLayout(flowNodes, flowEdges)
      edges = flowEdges
      const iris = new Set([entityId, ...data.nodes.map(n => n.id), ...(data.sequence_flow_iris ?? [])])
      for (const n of data.nodes.filter(n => n.invokes_process)) {
        iris.add(n.invokes_process)
        try {
          const subRaw = await invoke('automation__get_graph', { processIri: n.invokes_process })
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

<WidgetContainer
  icon="bolt"
  title={automationLabel || 'Automation'}
  {windowState}
  {onWindowStateChange}
  onClose={closeWidget}
  canExpand={true}
>
  {#snippet headerExtra()}
    {#if activeStepLabel}
      <span class="active-step-label">{activeStepLabel}…</span>
    {/if}
    {#if isExecutable}
      <button class="run-btn" onclick={runAutomation} disabled={running} title="Run automation">
        <span class="material-symbols-outlined">{running ? 'progress_activity' : 'play_arrow'}</span>
      </button>
    {/if}
    <Button variant="primary" size="sm" icon="info" title="Abrir inspector" onclick={openInspector} />
  {/snippet}

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
</WidgetContainer>


<style>
  .active-step-label {
    font-size: 10px;
    color: #F59E0B;
    font-style: italic;
    max-width: 140px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .run-btn {
    background: none;
    border: none;
    padding: 4px;
    cursor: pointer;
    color: #43A047;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.2s;
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
  .run-btn .material-symbols-outlined {
    font-size: 18px;
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
    font-size: 14px;
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
