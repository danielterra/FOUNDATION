<script>
  import { onMount, onDestroy } from 'svelte'
  import { invoke } from '@tauri-apps/api/core'
  import { listen } from '@tauri-apps/api/event'
  import { SvelteFlow, Controls, Background, BackgroundVariant } from '@xyflow/svelte'
  import '@xyflow/svelte/dist/style.css'
  import { applyDagreLayout } from './meta-process/layout.js'
  import NodeStartEvent from './meta-process/NodeStartEvent.svelte'
  import NodeEndEvent from './meta-process/NodeEndEvent.svelte'
  import NodeSystemTask from './meta-process/NodeSystemTask.svelte'
  import NodeUserTask from './meta-process/NodeUserTask.svelte'
  import NodeSubProcess from './meta-process/NodeSubProcess.svelte'
  import NodeGatewayExclusive from './meta-process/NodeGatewayExclusive.svelte'
  import NodeGatewayParallel from './meta-process/NodeGatewayParallel.svelte'
  import NodeGatewayEventBased from './meta-process/NodeGatewayEventBased.svelte'
  import NodeIntermediateEvent from './meta-process/NodeIntermediateEvent.svelte'
  import NodeGatewayCondition from './meta-process/NodeGatewayCondition.svelte'
  import NodeGatewayInclusive from './meta-process/NodeGatewayInclusive.svelte'
  import NodeBoundaryEvent from './meta-process/NodeBoundaryEvent.svelte'
  import NodeBoundaryCondition from './meta-process/NodeBoundaryCondition.svelte'

  let { widgetId, entityId, windowState = 'normal', onWindowStateChange } = $props()

  const nodeTypes = {
    MetaStartEvent:          NodeStartEvent,
    MetaEndEvent:            NodeEndEvent,
    MetaSystemTask:          NodeSystemTask,
    MetaUserTask:            NodeUserTask,
    MetaSubProcess:          NodeSubProcess,
    MetaExclusiveGateway:    NodeGatewayExclusive,
    MetaParallelGateway:     NodeGatewayParallel,
    MetaEventBasedGateway:   NodeGatewayEventBased,
    MetaIntermediateEvent:   NodeIntermediateEvent,
    MetaGatewayCondition:    NodeGatewayCondition,
    MetaInclusiveGateway:    NodeGatewayInclusive,
    MetaBoundaryEvent:       NodeBoundaryEvent,
    MetaBoundaryCondition:   NodeBoundaryCondition,
  }


  let processLabel = $state('')
  let nodes = $state([])
  let edges = $state([])
  let loading = $state(true)
  let error = $state(null)
  let expanded = $state(windowState === 'maximized')
  let watchedIris = new Set()
  let unlisten = null

  function portal(node) {
    const target = document.querySelector('.canvas-area') ?? document.body
    target.appendChild(node)
    return {
      destroy() { node.remove() }
    }
  }

  async function loadGraph() {
    loading = true
    error = null
    try {
      const raw = await invoke('meta_process__get_graph', { processIri: entityId })
      const data = JSON.parse(raw)
      processLabel = data.process_label

      const flowNodes = data.nodes.map(n => ({
        id: n.id,
        type: n.type,
        data: { label: n.label, nodeType: n.type, invokesProcess: n.invokes_process ?? null, status: n.status ?? null, conditionOperator: n.condition_operator ?? null, conditionValue: n.condition_value ?? null, eventType: n.event_type ?? null, triggerType: n.trigger_type ?? null, rendersComponent: n.renders_component ?? null },
        position: { x: 0, y: 0 },
      }))

      const flowEdges = data.edges.map(e => ({
        id: e.id,
        source: e.source,
        target: e.target,
        type: 'default',
        animated: true,
        label: e.label ?? undefined,
        labelStyle: 'background:#000;color:#f1f5f9;padding:3px 7px;border-radius:4px;border:1px solid #475569;font-size:11px;font-weight:600;',
      }))

      nodes = applyDagreLayout(flowNodes, flowEdges)
      edges = flowEdges
      watchedIris = new Set([entityId, ...data.nodes.map(n => n.id)])
    } catch (e) {
      error = String(e)
    } finally {
      loading = false
    }
  }

  async function handleNodeClick({ node }) {
    if (node.data.invokesProcess) {
      await invoke('widget_blackboard__add_widget', {
        widgetType: 'meta_process',
        entityId: node.data.invokesProcess,
        position: null,
        size: null,
      }).catch(e => console.error('Failed to open sub-process widget:', e))
    } else {
      await invoke('widget_blackboard__add_widget', {
        widgetType: 'inspector',
        entityId: node.id,
        position: null,
        size: null,
      }).catch(e => console.error('Failed to open inspector:', e))
    }
  }

  function toggleMinimize() {
    onWindowStateChange?.(windowState === 'minimized' ? 'normal' : 'minimized')
  }

  function openExpanded() {
    expanded = true
    onWindowStateChange?.('maximized')
  }

  function closeExpanded() {
    expanded = false
    onWindowStateChange?.('normal')
  }

  async function closeWidget() {
    await invoke('widget_blackboard__remove_widget', { widgetId }).catch(() => {})
  }

  onMount(async () => {
    await loadGraph()
    unlisten = await listen('entity-updated', async (event) => {
      if (watchedIris.has(event.payload.entityId)) await loadGraph()
    })
  })

  onDestroy(() => {
    if (unlisten) unlisten()
  })
</script>

<svelte:window onkeydown={(e) => { if (e.key === 'Escape' && expanded) closeExpanded() }} />

<div class="meta-process-widget" class:minimized={windowState === 'minimized'}>
  <div class="widget-header">
    <div class="header-left">
      <span class="material-symbols-outlined header-icon">schema</span>
      <span class="header-title">{processLabel || 'MetaProcess'}</span>
    </div>
    <div class="header-actions">
      <button class="action-btn" onclick={openExpanded} title="Expand">
        <span class="material-symbols-outlined">open_in_full</span>
      </button>
      <button class="action-btn" onclick={toggleMinimize} title={windowState === 'minimized' ? 'Expand' : 'Minimize'}>
        <span class="material-symbols-outlined">{windowState === 'minimized' ? 'expand_more' : 'expand_less'}</span>
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
          {edges}
          {nodeTypes}
          fitView
          panOnScroll
          zoomOnScroll={false}
          onnodeclick={handleNodeClick}
        >
          <Controls />
          <Background variant={BackgroundVariant.Dots} gap={20} size={1} color="rgba(255,255,255,0.08)" />
        </SvelteFlow>
      {/if}
    </div>
  </div>
</div>

{#if expanded}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div use:portal class="modal-overlay" onclick={closeExpanded}>
    <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
    <div class="modal-panel" onclick={(e) => e.stopPropagation()}>
      <div class="modal-header">
        <div class="header-left">
          <span class="material-symbols-outlined header-icon">schema</span>
          <span class="header-title">{processLabel || 'MetaProcess'}</span>
        </div>
        <div class="header-actions">
          <button class="action-btn" onclick={closeExpanded} title="Close">
            <span class="material-symbols-outlined">close_fullscreen</span>
          </button>
        </div>
      </div>
      <div class="modal-canvas">
        {#if !loading && !error}
          <SvelteFlow
            {nodes}
            {edges}
            {nodeTypes}
            fitView
            panOnScroll
            zoomOnScroll={false}
            onnodeclick={handleNodeClick}
          >
            <Controls />
            <Background variant={BackgroundVariant.Dots} gap={20} size={1} color="rgba(255,255,255,0.08)" />
          </SvelteFlow>
        {/if}
      </div>
      <div class="modal-hint">Scroll to zoom · Drag to pan · Press Esc to close</div>
    </div>
  </div>
{/if}

<style>
  .meta-process-widget {
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

  .widget-header,
  .modal-header {
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

  .modal-overlay {
    position: absolute;
    inset: 0;
    background: color-mix(in srgb, var(--color-black) 80%, transparent);
    backdrop-filter: blur(8px);
    z-index: 9999;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .modal-panel {
    width: 92vw;
    height: 90vh;
    display: flex;
    flex-direction: column;
    background: color-mix(in srgb, var(--color-black) 90%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-white) 20%, transparent);
    border-radius: 16px;
    overflow: hidden;
    box-shadow: 0 24px 64px color-mix(in srgb, var(--color-black) 60%, transparent);
  }

  .modal-canvas {
    flex: 1;
    overflow: hidden;
    position: relative;
  }

  .modal-hint {
    padding: 8px 16px;
    font-size: 11px;
    color: color-mix(in srgb, var(--color-white) 35%, transparent);
    text-align: center;
    border-top: 1px solid color-mix(in srgb, var(--color-white) 10%, transparent);
    flex-shrink: 0;
  }

  :global(.svelte-flow) {
    background: transparent !important;
    --xy-edge-label-background-color: #000000;
    --xy-edge-label-color: #f1f5f9;
  }

  :global(.svelte-flow .svelte-flow__node) {
    font-family: var(--font-title, sans-serif);
  }
</style>
