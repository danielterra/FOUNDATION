<script lang="ts">
  import { onMount, onDestroy } from 'svelte'
  import { invoke } from '@tauri-apps/api/core'
  import { listen } from '@tauri-apps/api/event'
  import { createEntitySubscription } from '$lib/realtime/subscriptions'
  import { openEntity } from '$lib/blackboard'
  import { SvelteFlow, Controls, Background, BackgroundVariant } from '@xyflow/svelte'
  import '@xyflow/svelte/dist/style.css'
  import { applyDagreLayout } from './automation/layout.js'
  import WidgetContainer from './WidgetContainer.svelte'
  import { Button } from '$lib/components/ui/button'
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

  let execNodeStatus = $state(new Map<string, { statusLabel: string; statusColor: string; statusIcon: string }>())
  let batchProgressMap = $state(new Map<string, BatchProgress>())
  let iterationOfToNodeId = new Map<string, string>()
  let activeExecutionIri = $state<string | null>(null)
  let activeStepLabel = $state<string | null>(null)

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
    unlistenExecStarted = await listen('automation-execution-started', (event) => {
      if ((event.payload as Record<string, unknown>).processIri !== entityId) return
      const execIri = (event.payload as Record<string, unknown>).executionIri as string
      activeExecutionIri = execIri
      execNodeStatus = new Map()
      batchProgressMap = new Map()
      iterationOfToNodeId = new Map()
      activeStepLabel = null
      running = true
      // Register creation-query for new StepExecution entities belonging to this execution (US5).
      // The stream remains the source of truth for the toast (ephemeral progress).
      // The entity subscription is the source of truth for the persistent status (inspector).
      if (execSub) execSub.destroy()
      const execStepIris = new Set()
      execSub = createEntitySubscription((event) => {
        if (event.type === 'joined-set' && event.predicate === 'foundation:belongsToExecution') {
          // A new step entered this execution — subscribe to its updates.
          const stepIri = event.entityId
          if (execStepIris.has(stepIri)) return
          execStepIris.add(stepIri)
          execSub.setIris([...execStepIris])
          return
        }
        if (event.type === 'updated') {
          // A StepExecution was updated — reflect its persisted status in the node overlay.
          // The stream already showed ephemeral progress; this ensures the final state is durable.
          const stepIri = event.entityId
          invoke('inspector__get_entity', { entityId: stepIri }).then((resultStr) => {
            const data = JSON.parse(resultStr as string)
            const props = data?.properties ?? []
            const statusProp = props.find(p => p.property === 'foundation:hasStatus')
            const execStepProp = props.find(p => p.property === 'foundation:executesStep')
            const nodeIri = execStepProp?.value ?? null
            if (!nodeIri) return
            const statusIri = statusProp?.value ?? ''
            // Map ontology status to display values.
            let statusLabel, statusColor, statusIcon
            if (statusIri.includes('Falhou') || statusIri.includes('1772993026091')) {
              statusLabel = 'Failed'; statusColor = 'var(--color-danger)'; statusIcon = 'error'
            } else if (statusIri === 'foundation:Completed' || statusIri.includes('Concluído')) {
              statusLabel = 'Done'; statusColor = 'var(--color-success)'; statusIcon = 'check_circle'
            } else {
              statusLabel = 'Running'; statusColor = 'var(--color-warning)'; statusIcon = 'progress_activity'
            }
            const map = new Map(execNodeStatus)
            map.set(nodeIri, { statusLabel, statusColor, statusIcon })
            execNodeStatus = map
          }).catch(() => {})
        }
      })
      execSub.setCreationQueries([{
        classIri: 'foundation:StepExecution',
        predicate: 'foundation:belongsToExecution',
        objectValue: execIri,
      }])
    })
    unlistenStepProgress = await listen('automation-step-progress', (event) => {
      const payload = event.payload as {
        executionIri: string
        nodeIri: string
        nodeLabel: string
        status: 'started' | 'completed' | 'failed'
        iterationOf?: string
        iterationIndex?: number
        iterationTotal?: number
      }
      if (payload.executionIri !== activeExecutionIri) return
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
      if (status === 'started') {
        map.set(nodeIri, { statusLabel: 'Running', statusColor: 'var(--color-warning)', statusIcon: 'progress_activity' })
        activeStepLabel = nodeLabel
      } else if (status === 'completed') {
        map.set(nodeIri, { statusLabel: 'Done', statusColor: 'var(--color-success)', statusIcon: 'check_circle' })
        activeStepLabel = null
      } else if (status === 'failed') {
        map.set(nodeIri, { statusLabel: 'Failed', statusColor: 'var(--color-danger)', statusIcon: 'error' })
        activeStepLabel = null
      }
      execNodeStatus = map
    })
    unlistenExecFinished = await listen('automation-execution-finished', (event) => {
      const p = event.payload as { executionIri: string }
      if (p.executionIri !== activeExecutionIri) return
      running = false
      activeStepLabel = null
    })
  })

  onDestroy(() => {
    entitySub.destroy()
    if (execSub) execSub.destroy()
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
>
  {#snippet headerExtra()}
    {#if activeStepLabel}
      <span class="active-step-label">{activeStepLabel}…</span>
    {/if}
    {#if isExecutable}
      <Button variant="ghost" size="icon-sm" class="run-btn-automation" onclick={runAutomation} disabled={running} title="Run automation">
        <span class="material-symbols-outlined">{running ? 'progress_activity' : 'play_arrow'}</span>
      </Button>
    {/if}
    <Button variant="ghost" size="icon-sm" title="Abrir inspector" onclick={openInspector}><span class="material-symbols-outlined">info</span></Button>
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
