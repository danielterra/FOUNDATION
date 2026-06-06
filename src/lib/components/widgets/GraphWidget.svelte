<script>
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { createEntitySubscription } from '$lib/realtime/subscriptions';
  import GraphVisualization from '../graph/GraphVisualization.svelte';
  import WidgetContainer from './WidgetContainer.svelte';

  let { widgetId, entityId = '', conversationIri = null, windowState = 'normal', onWindowStateChange } = $props();

  let label = $state('');
  let graphData = $state(null);
  let graphComponent = $state();
  // Snapshot tx cursor: max(tx) at the last loadGraph call.
  let snapshotTx = 0;
  // Flat set of entity IRIs currently in the graph.
  let graphIris = new Set();

  const entitySub = createEntitySubscription((event) => {
    if (event.type === 'joined-set') {
      // A new entity joined the graph via a relation — reload to pick it up.
      if (event.tx != null && event.tx <= snapshotTx) return;
      loadGraph();
      return;
    }
    if (event.type === 'updated' && graphIris.has(event.entityId)) {
      // An entity in the graph was updated — reload to reflect the change.
      loadGraph();
    }
  });

  async function closeWidget() {
    try {
      await invoke('widget_blackboard__remove_widget', { widgetId });
    } catch (err) {
      console.error('Failed to remove widget:', err);
    }
  }

  async function openInspector() {
    try {
      await invoke('widget_blackboard__add_widget', {
        widgetType: 'inspector',
        entityId,
        position: null,
        size: null,
        conversationId: conversationIri,
      });
    } catch (err) {
      console.error('Failed to open inspector:', err);
    }
  }

  async function resolveIris() {
    if (!entityId) return { iris: [], entityLabel: '' };
    try {
      const resultStr = await invoke('inspector__get_entity', { entityId });
      const data = JSON.parse(resultStr);
      const graphEntities = data?.properties
        ?.filter(p => p.property === 'foundation:graphEntities')
        ?.map(p => p.value) ?? [];
      const iris = graphEntities.length > 0 ? graphEntities : [entityId];
      return { iris, entityLabel: data?.label ?? '' };
    } catch {
      return { iris: [entityId], entityLabel: '' };
    }
  }

  async function loadGraph() {
    const { iris, entityLabel } = await resolveIris();
    if (iris.length === 0) return;
    try {
      const [json, snapTx] = await Promise.all([
        invoke('widget_blackboard__get_graph_data', { entityIris: iris }),
        invoke('chat__get_conversation_snapshot_tx', { conversationId: iris[0] ?? entityId }).catch(() => 0),
      ]);
      const data = JSON.parse(json);
      label = entityLabel || (iris.length === 1 ? (data.nodes.find(n => n.id === iris[0])?.label ?? iris[0]) : `${iris.length} entities`);
      graphData = { nodes: data.nodes, links: data.links };
      // Declare the flat set of entity IRIs for atomic updates.
      const nodeIris = (data.nodes ?? []).map(n => n.id).filter(Boolean);
      graphIris = new Set([entityId, ...iris, ...nodeIris]);
      entitySub.setIris([...graphIris]);
      snapshotTx = /** @type {number} */ (snapTx);
      entitySub.setSinceTx(snapshotTx);
      entitySub.replayMissed();
    } catch (err) {
      console.error('Failed to load graph data:', err);
    }
  }

  async function navigateTo(nodeId) {
    if (!nodeId) return;
    try {
      const json = await invoke('widget_blackboard__get_graph_data', { entityIris: [nodeId] });
      const data = JSON.parse(json);
      graphData = { nodes: data.nodes, links: data.links };
    } catch (err) {
      console.error('Failed to load graph data:', err);
    }
  }

  function handleNodeClick(nodeId) {
    navigateTo(nodeId);
  }

  onMount(async () => {
    await loadGraph();
  });

  onDestroy(() => {
    entitySub.destroy();
  });

  // Seed the anchor IRI before the first load so events during initialization aren't dropped.
  $effect(() => {
    if (entityId) entitySub.setIris([entityId]);
  });
</script>

<WidgetContainer
  icon="account_tree"
  title={label || entityId}
  {windowState}
  {onWindowStateChange}
  onClose={closeWidget}
>
  {#snippet headerActions()}
    <button class="action-btn" onclick={openInspector} title="Open inspector">
      <span class="material-symbols-outlined">info</span>
    </button>
  {/snippet}

  <div class="widget-content">
    {#if graphData}
      <GraphVisualization
        bind:this={graphComponent}
        {graphData}
        onNodeClick={handleNodeClick}
      />
    {:else}
      <div class="loading">
        <span class="material-symbols-outlined">progress_activity</span>
      </div>
    {/if}
  </div>
</WidgetContainer>

<style>
  .widget-content {
    flex: 1;
    position: relative;
    overflow: hidden;
  }

  .loading {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    color: var(--color-neutral);
  }

  .loading .material-symbols-outlined {
    font-size: 24px;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }
</style>
