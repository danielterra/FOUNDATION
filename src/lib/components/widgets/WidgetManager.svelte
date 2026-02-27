<script>
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import InspectorWidget from './InspectorWidget.svelte';

  let widgets = $state([]);
  let unlisteners = [];
  let draggedWidget = $state(null);
  let dragOffset = $state({ x: 0, y: 0 });
  let topZIndex = $state(100);

  async function loadWidgets() {
    try {
      const result = await invoke('widget__get_all');
      widgets = result.map((w, index) => ({
        ...w,
        zIndex: 100 + index
      }));
      topZIndex = Math.max(100, ...widgets.map(w => w.zIndex));
    } catch (error) {
      console.error('Failed to load widgets:', error);
    }
  }

  function bringToFront(widgetId) {
    topZIndex++;
    widgets = widgets.map(w =>
      w.id === widgetId ? { ...w, zIndex: topZIndex } : w
    );
  }

  function startDrag(event, widget) {
    // Only drag from header area
    if (!event.target.closest('.widget-header')) return;
    if (event.target.closest('.close-btn')) return;

    draggedWidget = widget;
    const rect = event.currentTarget.getBoundingClientRect();
    dragOffset = {
      x: event.clientX - rect.left,
      y: event.clientY - rect.top
    };
    bringToFront(widget.id);

    event.preventDefault();
  }

  function onDrag(event) {
    if (!draggedWidget) return;

    const newX = event.clientX - dragOffset.x;
    const newY = event.clientY - dragOffset.y;

    // Update position locally for smooth dragging
    widgets = widgets.map(w =>
      w.id === draggedWidget.id
        ? { ...w, position: { x: newX, y: newY } }
        : w
    );
  }

  async function stopDrag() {
    if (!draggedWidget) return;

    const widget = widgets.find(w => w.id === draggedWidget.id);
    if (widget) {
      try {
        await invoke('widget__update_position', {
          widgetId: widget.id,
          position: widget.position
        });
      } catch (error) {
        console.error('Failed to update widget position:', error);
      }
    }

    draggedWidget = null;
  }

  onMount(async () => {
    // Load widgets initially
    await loadWidgets();

    // Listen for widget events
    const unlistenAdded = await listen('widget-added', (event) => {
      console.log('Widget added:', event.payload);
      topZIndex++;
      widgets = [...widgets, { ...event.payload, zIndex: topZIndex }];
    });

    const unlistenRemoved = await listen('widget-removed', (event) => {
      console.log('Widget removed:', event.payload);
      widgets = widgets.filter(w => w.id !== event.payload);
    });

    const unlistenCleared = await listen('widgets-cleared', () => {
      console.log('Widgets cleared');
      widgets = [];
    });

    unlisteners = [unlistenAdded, unlistenRemoved, unlistenCleared];

    // Add global mouse event listeners for dragging
    document.addEventListener('mousemove', onDrag);
    document.addEventListener('mouseup', stopDrag);
  });

  onDestroy(() => {
    unlisteners.forEach(unlisten => unlisten());
    document.removeEventListener('mousemove', onDrag);
    document.removeEventListener('mouseup', stopDrag);
  });
</script>

<svelte:window />

{#each widgets as widget (widget.id)}
  <div
    class="widget-container"
    class:dragging={draggedWidget?.id === widget.id}
    style:left="{widget.position.x}px"
    style:top="{widget.position.y}px"
    style:width="{widget.size.width}px"
    style:height="{widget.size.height}px"
    style:z-index={widget.zIndex}
    onmousedown={(e) => startDrag(e, widget)}
    onclick={() => bringToFront(widget.id)}
    role="dialog"
    aria-label="Widget"
  >
    {#if widget.widget_type === 'inspector'}
      <InspectorWidget entityId={widget.entity_id} widgetId={widget.id} />
    {/if}
  </div>
{/each}

<style>
  .widget-container {
    position: absolute;
    transition: box-shadow 0.2s;
  }

  .widget-container.dragging {
    cursor: grabbing;
    box-shadow: 0 12px 48px rgba(0, 0, 0, 0.6);
  }

  .widget-container :global(.widget-header) {
    cursor: grab;
  }

  .widget-container.dragging :global(.widget-header) {
    cursor: grabbing;
  }
</style>
