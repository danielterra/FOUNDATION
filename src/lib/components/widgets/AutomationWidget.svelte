<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { invoke } from '@tauri-apps/api/core'
  import { listen } from '@tauri-apps/api/event'
  import { createEntitySubscription, type EntityEvent } from '$lib/realtime/subscriptions'
  import { openEntity } from '$lib/blackboard'
  import { SvelteFlow, Controls, Background, BackgroundVariant } from '@xyflow/svelte'
  import '@xyflow/svelte/dist/style.css'
  import { applyDagreLayout } from './automation/layout.js'
  import WidgetContainer from './WidgetContainer.svelte'
  import { Button } from '$lib/components/ui/button'
  import ExecutionHistory from './automation/ExecutionHistory.svelte'
  import NodeStartEvent        from './automation/NodeStartEvent.svelte'
  import NodeTimerStartEvent   from './automation/NodeTimerStartEvent.svelte'
  import NodeEndEvent     from './automation/NodeEndEvent.svelte'
  import NodeAgentTask    from './automation/NodeAgentTask.svelte'
  import NodeCodeTask     from './automation/NodeCodeTask.svelte'
  import NodeRequestTask  from './automation/NodeRequestTask.svelte'
  import NodeUserTask     from './automation/NodeUserTask.svelte'
  import NodeGateway      from './automation/NodeGateway.svelte'
  import NodeSubProcess        from './automation/NodeSubProcess.svelte'
  import NodeNOVAMessageTask   from './automation/NodeNOVAMessageTask.svelte'
  import NodeTemplateTask      from './automation/NodeTemplateTask.svelte'

  interface BatchProgress {
    done: number
    failed: number
    total: number | null
  }

  interface AutomationProgressPayload {
    executionIri: string
    stepIri: string
    nodeIri: string
    nodeLabel?: string
    status: 'started' | 'completed' | 'failed' | 'waiting' | 'running' | 'cancelled'
    phase?: 'query-properties' | 'calculated-properties'
    entityIri?: string
    propertyIri?: string
    completedCount?: number
    totalCount?: number
    reason?: string
    error?: string
    iterationOf?: string
    iterationIndex?: number
    iterationTotal?: number
  }

  interface AutomationFinishedPayload {
    executionIri: string
    status: 'completed' | 'failed' | 'cancelled'
    error?: string
  }

  let { widgetId, entityId, windowState = 'normal', onWindowStateChange, conversationIri = null } = $props()

  const nodeTypes = {
    automation_StartEvent:       NodeStartEvent,
    automation_TimerStartEvent:  NodeTimerStartEvent,
    automation_EndEvent:         NodeEndEvent,
    automation_Gateway:          NodeGateway,
    automation_SubProcess:       NodeSubProcess,
    automation_AgentTask:        NodeAgentTask,
    automation_CodeTask:         NodeCodeTask,
    automation_NOVAMessageTask:  NodeNOVAMessageTask,
    automation_RequestTask:      NodeRequestTask,
    automation_UserTask:         NodeUserTask,
    automation_TemplateTask:     NodeTemplateTask,
  }

  let automationLabel = $state('')
  let isExecutable = $state(false)
  let nodes = $state.raw([])
  let edges = $state.raw([])
  let loading = $state(true)
  let error = $state(null)
  let hoveredNodeId = $state(null)
  let running = $state(false)
  let activeTab = $state<'flow' | 'history'>('flow')

  let execNodeStatus = $state(new Map<string, { statusLabel: string; statusColor: string; statusIcon: string }>())
  let batchProgressMap = $state(new Map<string, BatchProgress>())
  let iterationOfToNodeId = new Map<string, string>()
  let activeExecutionIri = $state<string | null>(null)
  let activeStepLabel = $state<string | null>(null)
  let activeStepPhase = $state<'query-properties' | 'calculated-properties' | null>(null)
  let activeMaterializationStatus = $state<'waiting' | 'running' | 'completed' | 'failed' | 'cancelled' | null>(null)
  let activeMaterializationProgress = $state<{ completedCount?: number; totalCount?: number } | null>(null)
  let executionOutcome = $state<'completed' | 'failed' | 'cancelled' | null>(null)
  let activeNodeIri: string | null = null

  const nodesWithExecStatus = $derived(
    execNodeStatus.size === 0 && batchProgressMap.size === 0
      ? nodes
      : nodes.map(n => {
          const s = execNodeStatus.get(n.id)
          const bp = batchProgressMap.get(n.id)
          if (!s && !bp) return n

          let statusOverride = s ? { status: s.statusLabel, statusColor: s.statusColor, statusIcon: s.statusIcon } : {}
          if (bp && bp.total !== null && bp.done + bp.failed >= bp.total) {
            if (bp.failed > 0) {
              statusOverride = { status: 'Failed', statusColor: 'var(--color-danger)', statusIcon: 'error' }
            } else {
              statusOverride = { status: 'Done', statusColor: 'var(--color-success)', statusIcon: 'check_circle' }
            }
          }

          return {
            ...n,
            data: {
              ...n.data,
              ...statusOverride,
              ...(bp ? { batchProgress: bp } : {}),
            }
          }
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

  // Snapshot tx cursor: max(tx) at the last loadGraph call.
  let snapshotTx = 0
  // IRI set for the flat subscription: automation + nodes + sequence flows.
  let watchedIris = new Set()
  // Execution-scoped subscription for StepExecution entities.
  let execSub = null
  let execStepIris = new Set()

  const entitySub = createEntitySubscription(async (event) => {
    if (event.type === 'joined-set') {
      if (event.classIri === 'foundation:automation_FlowNode' ||
          // cover all FlowNode subtypes by checking the predicate
          event.predicate === 'foundation:partOfProcess') {
        // A new node was added to this automation — reload the diagram to pick it up.
        // Layout needs recalculation anyway so a full graph reload is appropriate here.
        if (event.tx != null && event.tx <= snapshotTx) return
        await loadGraph()
      }
      return
    }
    if (event.type === 'updated') {
      // A node or sequence flow in the flat set changed — only reload the graph
      // if the changed entity is part of the current diagram (avoid false reloads
      // when a node of another automation is in the watched set by coincidence).
      if (watchedIris.has(event.entityId)) {
        await loadGraph()
      }
    }
  })
  let unlistenExecStarted = null
  let unlistenStepProgress = null
  let unlistenExecFinished = null

  async function loadGraph() {
    loading = true
    error = null
    try {
      const [raw, snapTx] = await Promise.all([
        invoke('automation__get_graph', { processIri: entityId }),
        invoke('chat__get_conversation_snapshot_tx', { conversationId: entityId }).catch(() => 0),
      ])
      const data = JSON.parse(raw as string)
      automationLabel = data.process_label
      isExecutable = data.is_executable ?? false

      const flowNodes = data.nodes.map((n: Record<string, unknown>) => ({
        id: n.id as string,
        type: n.realType as string,
        data: {
          label: n.label,
          nodeType: n.realType,
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
          isMultiInstance: n.isMultiInstance === true ? true : undefined,
          isSequential: n.isMultiInstance === true ? (n.isSequential !== false) : undefined,
        },
        position: { x: 0, y: 0 },
      }))

      const gatewayIds = new Set(data.nodes.filter(n => n.realType === 'automation_Gateway').map(n => n.id))

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
          style: isErrorEdge ? 'stroke:var(--color-danger);stroke-dasharray:5,4;stroke-width:1.5px;' : undefined,
          label: isErrorEdge ? (e.label ?? 'on error') : (conditionLabel ?? undefined),
          labelStyle: isErrorEdge
            ? 'background:var(--color-surface-2);color:var(--color-danger-hover);padding:2px 7px;border:1px solid var(--color-danger);font-size:11px;font-weight:500;'
            : conditionLabel ? 'background:var(--color-surface-2);color:var(--color-neutral-active);padding:2px 7px;border:1px solid var(--color-surface-3);font-size:11px;font-weight:500;' : undefined,
        }
      })

      nodes = applyDagreLayout(flowNodes, flowEdges)
      edges = flowEdges
      const iris = new Set([entityId, ...data.nodes.map(n => n.id), ...(data.sequence_flow_iris ?? [])])
      for (const n of data.nodes.filter(n => n.invokes_process)) {
        iris.add(n.invokes_process)
        try {
          const subRaw = await invoke('automation__get_graph', { processIri: n.invokes_process })
          const subData = JSON.parse(subRaw as string)
          subData.nodes.forEach(sn => iris.add(sn.id))
        } catch {}
      }
      watchedIris = iris
      entitySub.setIris(iris)
      // Register creation-query for new nodes added to this automation (US6).
      entitySub.setCreationQueries([{
        classIri: 'foundation:automation_FlowNode',
        predicate: 'foundation:partOfProcess',
        objectValue: entityId,
      }])
      snapshotTx = snapTx as number
      entitySub.setSinceTx(snapshotTx)
      entitySub.replayMissed()
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
      await openEntity(node.id, conversationIri).catch(e => console.error('Failed to open entity:', e))
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

  async function closeWidget() {
    await invoke('widget_blackboard__remove_widget', { widgetId }).catch(() => {})
  }

  function materializationPhaseLabel(phase, status) {
    if (status === 'failed') return 'Falha ao atualizar os dados necessários'
    if (status === 'cancelled') return 'Atualização dos dados cancelada'
    if (status === 'completed') return 'Dados atualizados'
    if (phase === 'query-properties') return 'Atualizando propriedades relacionadas'
    return 'Atualizando propriedades calculadas'
  }

  function hasDeterminateProgress(progress) {
    return progress?.completedCount !== undefined
      && progress?.totalCount !== undefined
      && progress.totalCount > 0
      && progress.completedCount >= 0
      && progress.completedCount <= progress.totalCount
  }

  async function loadStepStatus(stepIri) {
    const resultStr = await invoke('inspector__get_entity', { entityId: stepIri })
    const data = JSON.parse(resultStr as string)
    const props = data?.properties ?? []
    const statusProp = props.find(p => p.property === 'foundation:hasStatus')
    const execStepProp = props.find(p => p.property === 'foundation:executesStep')
    const nodeIri = execStepProp?.value ?? null
    if (!nodeIri) return
    const statusIri = statusProp?.value ?? ''
    let statusLabel, statusColor, statusIcon
    if (statusIri.includes('Falhou') || statusIri.includes('1772993026091')) {
      statusLabel = 'Failed'; statusColor = 'var(--color-danger)'; statusIcon = 'error'
    } else if (statusIri === 'foundation:Status_1772570972069') {
      statusLabel = 'Cancelled'; statusColor = 'var(--color-warning)'; statusIcon = 'cancel'
    } else if (statusIri === 'foundation:Completed' || statusIri.includes('Concluído')) {
      statusLabel = 'Done'; statusColor = 'var(--color-success)'; statusIcon = 'check_circle'
    } else {
      statusLabel = 'Running'; statusColor = 'var(--color-warning)'; statusIcon = 'progress_activity'
    }
    const map = new Map(execNodeStatus)
    map.set(nodeIri, { statusLabel, statusColor, statusIcon })
    execNodeStatus = map
  }

  function handleExecutionEntityEvent(event: EntityEvent) {
    if (event.type === 'joined-set' && event.predicate === 'foundation:belongsToExecution') {
      const stepIri = event.entityId
      if (execStepIris.has(stepIri)) return
      execStepIris.add(stepIri)
      execSub.setIris([...execStepIris])
      return
    }
    if (event.type === 'updated') {
      loadStepStatus(event.entityId).catch(() => {})
    }
  }

  function handleExecutionStarted(event: { payload: { processIri: string; executionIri: string } }) {
    if (event.payload.processIri !== entityId) return
    const execIri = event.payload.executionIri
    activeExecutionIri = execIri
    execNodeStatus = new Map()
    batchProgressMap = new Map()
    iterationOfToNodeId = new Map()
    activeStepLabel = null
    activeStepPhase = null
    activeMaterializationStatus = null
    activeMaterializationProgress = null
    executionOutcome = null
    activeNodeIri = null
    running = true
    if (execSub) execSub.destroy()
    execStepIris = new Set()
    execSub = createEntitySubscription(handleExecutionEntityEvent)
    execSub.setCreationQueries([{
      classIri: 'foundation:StepExecution',
      predicate: 'foundation:belongsToExecution',
      objectValue: execIri,
    }])
  }

  function handleStepProgress(event: { payload: AutomationProgressPayload }) {
    const payload = event.payload
    if (payload.executionIri !== activeExecutionIri) return

    if (payload.phase) {
      const nodeIri = payload.nodeIri
      const node = nodes.find(n => n.id === nodeIri)
      activeNodeIri = nodeIri
      activeStepLabel = node?.data?.label ?? activeStepLabel ?? 'Etapa'
      activeStepPhase = payload.phase
      activeMaterializationStatus = payload.status as typeof activeMaterializationStatus
      activeMaterializationProgress = {
        completedCount: payload.completedCount,
        totalCount: payload.totalCount,
      }
      const map = new Map(execNodeStatus)
      if (payload.status === 'failed') {
        map.set(nodeIri, { statusLabel: 'Failed', statusColor: 'var(--color-danger)', statusIcon: 'error' })
      } else if (payload.status === 'cancelled') {
        map.set(nodeIri, { statusLabel: 'Cancelled', statusColor: 'var(--color-warning)', statusIcon: 'cancel' })
      } else {
        map.set(nodeIri, { statusLabel: 'Running', statusColor: 'var(--color-warning)', statusIcon: 'progress_activity' })
      }
      execNodeStatus = map
      return
    }

    const { nodeIri, nodeLabel, status } = payload
    if (payload.iterationOf != null && payload.iterationTotal != null) {
      const iterIri = payload.iterationOf
      const total = payload.iterationTotal
      let nodeId = iterationOfToNodeId.get(iterIri)
      if (nodeId == null) {
        const alreadyMapped = new Set(iterationOfToNodeId.values())
        const candidate = nodes.find(n => n.data?.isMultiInstance && !alreadyMapped.has(n.id))
        nodeId = candidate ? candidate.id : iterIri
        iterationOfToNodeId.set(iterIri, nodeId)
      }
      const prev = batchProgressMap.get(nodeId) ?? { done: 0, failed: 0, total: null }
      const next: BatchProgress = {
        done: prev.done + (status === 'completed' ? 1 : 0),
        failed: prev.failed + (status === 'failed' ? 1 : 0),
        total,
      }
      const bpMap = new Map(batchProgressMap)
      bpMap.set(nodeId, next)
      batchProgressMap = bpMap
      const statusMap = new Map(execNodeStatus)
      if (!statusMap.has(nodeId)) {
        statusMap.set(nodeId, { statusLabel: 'Running', statusColor: 'var(--color-warning)', statusIcon: 'progress_activity' })
        execNodeStatus = statusMap
      }
      return
    }

    const map = new Map(execNodeStatus)
    activeNodeIri = nodeIri
    if (status === 'started') {
      map.set(nodeIri, { statusLabel: 'Running', statusColor: 'var(--color-warning)', statusIcon: 'progress_activity' })
      activeStepLabel = nodeLabel ?? 'Etapa'
      activeStepPhase = null
      activeMaterializationStatus = null
      activeMaterializationProgress = null
    } else if (status === 'completed') {
      map.set(nodeIri, { statusLabel: 'Done', statusColor: 'var(--color-success)', statusIcon: 'check_circle' })
      activeStepLabel = null
      activeNodeIri = null
      activeStepPhase = null
      activeMaterializationStatus = null
      activeMaterializationProgress = null
    } else if (status === 'failed') {
      map.set(nodeIri, { statusLabel: 'Failed', statusColor: 'var(--color-danger)', statusIcon: 'error' })
      activeStepLabel = nodeLabel ?? 'Etapa'
      activeStepPhase = null
      activeMaterializationStatus = null
      activeMaterializationProgress = null
    }
    execNodeStatus = map
  }

  function handleExecutionFinished(event: { payload: AutomationFinishedPayload }) {
    const payload = event.payload
    if (payload.executionIri !== activeExecutionIri) return
    running = false
    activeStepLabel = null
    activeStepPhase = null
    activeMaterializationStatus = payload.status === 'cancelled' ? 'cancelled' : null
    executionOutcome = payload.status
    if (payload.status === 'cancelled' && activeNodeIri) {
      const map = new Map(execNodeStatus)
      map.set(activeNodeIri, { statusLabel: 'Cancelled', statusColor: 'var(--color-warning)', statusIcon: 'cancel' })
      execNodeStatus = map
    }
  }

  async function mountWidget() {
    await loadGraph()
    unlistenExecStarted = await listen('automation-execution-started', handleExecutionStarted)
    unlistenStepProgress = await listen('automation-step-progress', handleStepProgress)
    unlistenExecFinished = await listen('automation-execution-finished', handleExecutionFinished)
  }

  function destroyWidget() {
    entitySub.destroy()
    if (execSub) execSub.destroy()
    if (unlistenExecStarted) unlistenExecStarted()
    if (unlistenStepProgress) unlistenStepProgress()
    if (unlistenExecFinished) unlistenExecFinished()
  }

  onMount(mountWidget)
  onDestroy(destroyWidget)
</script>

<WidgetContainer
  icon="bolt"
  title={automationLabel || 'Automation'}
  {windowState}
  {onWindowStateChange}
  onClose={closeWidget}
  {entityId}
  {conversationIri}
>
  {#snippet headerExtra()}
    {#if activeStepLabel}
      <span class="active-step-label" aria-live="polite">{activeStepLabel}</span>
    {/if}
    {#if activeStepPhase}
      <span class="active-step-phase">
        <span role="status" aria-live="polite">{materializationPhaseLabel(activeStepPhase, activeMaterializationStatus)}</span>
        {#if activeMaterializationStatus === 'waiting' || activeMaterializationStatus === 'running'}
          {#if hasDeterminateProgress(activeMaterializationProgress)}
            <span>{activeMaterializationProgress.completedCount}/{activeMaterializationProgress.totalCount}</span>
          {:else}
            <progress aria-label="Atualizando os dados necessários"></progress>
          {/if}
        {/if}
      </span>
    {:else if executionOutcome === 'cancelled'}
      <span class="execution-outcome cancelled" role="status">Automação cancelada</span>
    {:else if executionOutcome === 'failed'}
      <span class="execution-outcome failed" role="status">Automação falhou</span>
    {/if}
    {#if isExecutable}
      <Button variant="ghost" size="icon-sm" class="run-btn-automation" onclick={runAutomation} disabled={running} title="Run automation">
        <span class="material-symbols-outlined">{running ? 'progress_activity' : 'play_arrow'}</span>
      </Button>
    {/if}
  {/snippet}

  <div class="widget-content">
    <div class="automation-tabs" role="tablist" aria-label="Visualização da automação">
      <button class:active={activeTab === 'flow'} id="automation-flow-tab" role="tab" aria-selected={activeTab === 'flow'} aria-controls="automation-flow-panel" onclick={() => activeTab = 'flow'}>Fluxo</button>
      <button class:active={activeTab === 'history'} id="automation-history-tab" role="tab" aria-selected={activeTab === 'history'} aria-controls="automation-history-panel" onclick={() => activeTab = 'history'}>Execuções</button>
    </div>
    {#if activeTab === 'history'}
      <div class="history-panel" id="automation-history-panel" role="tabpanel" aria-labelledby="automation-history-tab">
        <ExecutionHistory automationIri={entityId} />
      </div>
    {:else}
      <div class="flow-panel" id="automation-flow-panel" role="tabpanel" aria-labelledby="automation-flow-tab">
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
        nodes={nodesWithExecStatus}
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
        <Background variant={BackgroundVariant.Dots} gap={20} size={1} />
      </SvelteFlow>
    {/if}
      </div>
    {/if}
  </div>
</WidgetContainer>


<style>
  .active-step-label {
    font-size: 10px;
    color: var(--color-warning);
    font-style: italic;
    max-width: 140px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .active-step-phase,
  .execution-outcome {
    display: inline-flex;
    align-items: center;
    gap: var(--spacing-inline-tight);
    font-size: var(--type-caption-compact);
    max-width: var(--size-toast-max-width);
  }

  .active-step-phase {
    color: var(--color-transition);
  }

  .active-step-phase progress {
    width: var(--size-toast-recalc-min-width);
    height: var(--size-startup-progress-height);
    accent-color: var(--color-transition);
  }

  .execution-outcome.cancelled {
    color: var(--color-warning);
  }

  .execution-outcome.failed {
    color: var(--color-danger);
  }

  :global(.run-btn-automation) {
    color: var(--color-success) !important;
  }

  :global(.run-btn-automation:hover:not(:disabled)) {
    background: color-mix(in srgb, var(--color-success) 15%, transparent) !important;
    color: var(--color-success) !important;
  }

  :global(.run-btn-automation:disabled .material-symbols-outlined) {
    animation: spin 1s linear infinite;
  }

  :global(.run-btn-automation .material-symbols-outlined) {
    font-size: 18px !important;
  }

  .widget-content {
    height: 100%;
    overflow: hidden;
    position: relative;
    display: flex;
    flex-direction: column;
  }

  .automation-tabs {
    flex: 0 0 auto;
    display: flex;
    gap: var(--spacing-field-group);
    padding: 0 var(--spacing-widget-inset);
    border-bottom: var(--size-hud-frame-line) solid var(--color-hud-line-subtle);
  }

  .automation-tabs button {
    min-height: var(--size-touch-target);
    padding: var(--space-2) var(--space-1);
    border: 0;
    border-bottom: var(--size-toast-edge) solid transparent;
    background: transparent;
    color: var(--text-secondary);
    font: inherit;
    font-size: var(--type-body-compact);
    cursor: pointer;
  }

  .automation-tabs button.active {
    border-bottom-color: var(--color-selection);
    color: var(--color-selection);
  }

  .automation-tabs button:focus-visible {
    outline: var(--size-toast-edge) solid var(--color-selection);
    outline-offset: var(--size-hud-frame-line);
  }

  .history-panel, .flow-panel {
    flex: 1 1 auto;
    min-height: 0;
    overflow: hidden;
  }

  .flow-panel :global(.svelte-flow) {
    height: 100%;
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
    color: var(--color-error);
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
