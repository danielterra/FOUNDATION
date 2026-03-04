<script>
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { fly } from 'svelte/transition';
  import { cubicOut, cubicIn } from 'svelte/easing';
  import InspectorWidget from './InspectorWidget.svelte';

  const BASE_WIDGET_Z_INDEX = 100;
  const CHAT_PANEL_WIDTH_RATIO = 0.3;
  const CHAT_PANEL_MIN_WIDTH = 500;
  const WIDGET_CASCADE_STEPS = 10;
  const WIDGET_CASCADE_OFFSET = 30;
  const WIDGET_FLY_DURATION = 600;

  let widgets = $state([]);
  let unlisteners = [];
  let draggedWidget = $state(null);
  let dragOffset = $state({ x: 0, y: 0 });
  let topZIndex = $state(BASE_WIDGET_Z_INDEX);
  let viewportWidth = $state(0);
  let viewportHeight = $state(0);
  let widgetOffset = $state(0);

  function constrainToBounds(position, size) {
    const minX = 0;
    const minY = 0;
    const chatWidth = Math.max(viewportWidth * CHAT_PANEL_WIDTH_RATIO, CHAT_PANEL_MIN_WIDTH);
    const maxX = viewportWidth - chatWidth - size.width;
    const maxY = viewportHeight - size.height;

    return {
      x: Math.max(minX, Math.min(maxX, position.x)),
      y: Math.max(minY, Math.min(maxY, position.y))
    };
  }

  function updateViewportSize() {
    viewportWidth = window.innerWidth;
    viewportHeight = window.innerHeight;

    widgets = widgets.map(w => ({
      ...w,
      position: constrainToBounds(w.position, w.size)
    }));
  }

  async function loadWidgets() {
    try {
      const result = await invoke('widget__get_all');
      widgets = result.map((w, index) => ({
        ...w,
        zIndex: BASE_WIDGET_Z_INDEX + index
      }));
      topZIndex = Math.max(BASE_WIDGET_Z_INDEX, ...widgets.map(w => w.zIndex));

      updateViewportSize();
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

    widgets = widgets.map(w =>
      w.id === draggedWidget.id
        ? { ...w, position: constrainToBounds({ x: newX, y: newY }, w.size) }
        : w
    );
  }

  function stopDrag() {
    if (!draggedWidget) return;

    const widget = widgets.find(w => w.id === draggedWidget.id);
    draggedWidget = null;

    if (widget) {
      invoke('widget__update_position', {
        widgetId: widget.id,
        position: widget.position
      }).catch(error => {
        console.error('Failed to update widget position:', error);
      });
    }
  }

  onMount(async () => {
    updateViewportSize();

    await loadWidgets();

    const unlistenAdded = await listen('widget-added', (event) => {
      // If widget already exists (e.g. duplicate entity-created event), bring it to front
      const existingIdx = widgets.findIndex(w => w.id === event.payload.id);
      if (existingIdx >= 0) {
        bringToFront(event.payload.id);
        return;
      }

      topZIndex++;
      const newWidget = { ...event.payload, zIndex: topZIndex };

      widgetOffset = (widgetOffset + 1) % WIDGET_CASCADE_STEPS;
      newWidget.position = {
        x: newWidget.position.x + (widgetOffset * WIDGET_CASCADE_OFFSET),
        y: newWidget.position.y + (widgetOffset * WIDGET_CASCADE_OFFSET)
      };

      newWidget.position = constrainToBounds(newWidget.position, newWidget.size);
      widgets = [...widgets, newWidget];
    });

    const unlistenRemoved = await listen('widget-removed', (event) => {
      widgets = widgets.filter(w => w.id !== event.payload);
    });

    const unlistenCleared = await listen('widgets-cleared', () => {
      widgets = [];
    });

    const unlistenEntityCreated = await listen('entity-created', async (event) => {
      try {
        await invoke('widget__add', {
          widgetType: 'inspector',
          entityId: event.payload.entityId,
          position: null,
          size: null
        });
      } catch (error) {
        console.error('Failed to open inspector for new entity:', error);
      }
    });

    unlisteners = [unlistenAdded, unlistenRemoved, unlistenCleared, unlistenEntityCreated];

    document.addEventListener('mousemove', onDrag);
    document.addEventListener('mouseup', stopDrag);

    window.addEventListener('resize', updateViewportSize);
  });

  onDestroy(() => {
    unlisteners.forEach(unlisten => unlisten());
    document.removeEventListener('mousemove', onDrag);
    document.removeEventListener('mouseup', stopDrag);
    window.removeEventListener('resize', updateViewportSize);
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
    tabindex="0"
    aria-label="Widget"
    in:fly={{ x: -viewportWidth, duration: WIDGET_FLY_DURATION, opacity: 1, easing: cubicOut }}
    out:fly={{ x: -viewportWidth, duration: WIDGET_FLY_DURATION, opacity: 1, easing: cubicIn }}
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
    box-shadow: 0 12px 48px color-mix(in srgb, var(--color-black) 60%, transparent);
  }

  .widget-container :global(.widget-header) {
    cursor: grab;
  }

  .widget-container.dragging :global(.widget-header) {
    cursor: grabbing;
  }
</style>
