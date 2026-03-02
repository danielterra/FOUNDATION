<script>
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import InspectorWidget from './InspectorWidget.svelte';

  let widgets = $state([]);
  let unlisteners = [];
  let draggedWidget = $state(null);
  let dragOffset = $state({ x: 0, y: 0 });
  let topZIndex = $state(100);
  let viewportWidth = $state(0);
  let viewportHeight = $state(0);
  let widgetOffset = $state(0);

  function constrainToBounds(position, size) {
    const minX = 0;
    const minY = 0;
    // Chat area takes 30% of viewport with min 500px, so exclude that from the right side
    const chatWidth = Math.max(viewportWidth * 0.3, 500);
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

    // Reposition all widgets to ensure they're within bounds
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
        zIndex: 100 + index
      }));
      topZIndex = Math.max(100, ...widgets.map(w => w.zIndex));

      // Constrain widgets to viewport after loading
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

    // Update position locally for smooth dragging with bounds constraint
    widgets = widgets.map(w =>
      w.id === draggedWidget.id
        ? { ...w, position: constrainToBounds({ x: newX, y: newY }, w.size) }
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
    // Initialize viewport size
    updateViewportSize();

    // Load widgets initially
    await loadWidgets();

    // Listen for widget events
    const unlistenAdded = await listen('widget-added', (event) => {
      // If widget already exists (e.g. duplicate entity-created event), bring it to front
      const existingIdx = widgets.findIndex(w => w.id === event.payload.id);
      if (existingIdx >= 0) {
        bringToFront(event.payload.id);
        return;
      }

      topZIndex++;
      const newWidget = { ...event.payload, zIndex: topZIndex };

      // Add cascade offset (30px each direction)
      const cascadeOffset = 30;
      widgetOffset = (widgetOffset + 1) % 10; // Reset after 10 widgets
      newWidget.position = {
        x: newWidget.position.x + (widgetOffset * cascadeOffset),
        y: newWidget.position.y + (widgetOffset * cascadeOffset)
      };

      // Ensure new widget is within bounds
      newWidget.position = constrainToBounds(newWidget.position, newWidget.size);
      widgets = [...widgets, newWidget];
    });

    const unlistenRemoved = await listen('widget-removed', (event) => {
      widgets = widgets.filter(w => w.id !== event.payload);
    });

    const unlistenCleared = await listen('widgets-cleared', () => {
      widgets = [];
    });

    // Listen for entity-created events to auto-open inspector
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

    // Add global mouse event listeners for dragging
    document.addEventListener('mousemove', onDrag);
    document.addEventListener('mouseup', stopDrag);

    // Add window resize listener to keep widgets in bounds
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
    in:fly={{ x: -viewportWidth, duration: 1500, opacity: 1, easing: cubicOut }}
    out:fly={{ x: -viewportWidth, duration: 1500, opacity: 1, easing: cubicOut }}
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
