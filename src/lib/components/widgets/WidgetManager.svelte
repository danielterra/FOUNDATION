<script>
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { fly } from 'svelte/transition';
  import { cubicOut, cubicIn } from 'svelte/easing';
  import InspectorWidget from './InspectorWidget.svelte';
  import MermaidWidget from './MermaidWidget.svelte';
  import ProcessStatusWidget from './ProcessStatusWidget.svelte';
  import ConnectorCredentialWidget from './ConnectorCredentialWidget.svelte';
  import ConnectorManagerWidget from './ConnectorManagerWidget.svelte';
  import MetaProcessWidget from './MetaProcessWidget.svelte';

  const BASE_WIDGET_Z_INDEX = 100;
  const WIDGET_FLY_DURATION = 600;
  const MINIMIZED_HEIGHT = 70;
  const MIN_WIDGET_WIDTH = 200;
  const MIN_WIDGET_HEIGHT = 100;
  const TOP_BAR_HEIGHT = 44;

  let widgets = $state([]);
  let unlisteners = [];
  let draggedWidget = $state(null);
  let dragOffset = $state({ x: 0, y: 0 });
  let resizingWidget = $state(null);
  let resizeStart = $state({ mouseX: 0, mouseY: 0, width: 0, height: 0 });
  let topZIndex = $state(BASE_WIDGET_Z_INDEX);
  let viewportWidth = $state(0);
  let viewportHeight = $state(0);

  function constrainSize(size) {
    return {
      width: Math.min(size.width, Math.max(MIN_WIDGET_WIDTH, viewportWidth)),
      height: Math.min(size.height, Math.max(MIN_WIDGET_HEIGHT, viewportHeight))
    };
  }

  function constrainToBounds(position, size, displayHeight) {
    const minX = 0;
    const minY = TOP_BAR_HEIGHT;
    const maxX = viewportWidth - size.width;
    const maxY = viewportHeight - (displayHeight ?? size.height);

    return {
      x: Math.max(minX, Math.min(maxX, position.x)),
      y: Math.max(minY, Math.min(maxY, position.y))
    };
  }

  function displayHeight(widget) {
    return widget.window_state === 'minimized' ? MINIMIZED_HEIGHT : widget.size.height;
  }

  function updateViewportSize() {
    viewportWidth = window.innerWidth;
    viewportHeight = window.innerHeight;

    widgets = widgets.map(w => {
      const constrainedSize = constrainSize(w.size);
      if (constrainedSize.width !== w.size.width || constrainedSize.height !== w.size.height) {
        invoke('widget_blackboard__update_widget_size', {
          widgetId: w.id,
          size: constrainedSize
        }).catch(error => console.error('Failed to update widget size:', error));
      }
      return {
        ...w,
        size: constrainedSize,
        position: constrainToBounds(w.position, constrainedSize, displayHeight(w))
      };
    });
  }

  async function loadWidgets() {
    try {
      const result = await invoke('widget_blackboard__get_widgets');
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

  function startResize(event, widget) {
    resizingWidget = widget;
    resizeStart = {
      mouseX: event.clientX,
      mouseY: event.clientY,
      width: widget.size.width,
      height: widget.size.height
    };
    bringToFront(widget.id);
    event.preventDefault();
    event.stopPropagation();
  }

  function onResize(event) {
    if (!resizingWidget) return;
    const dx = event.clientX - resizeStart.mouseX;
    const dy = event.clientY - resizeStart.mouseY;
    const newWidth = Math.max(MIN_WIDGET_WIDTH, resizeStart.width + dx);
    const newHeight = Math.max(MIN_WIDGET_HEIGHT, resizeStart.height + dy);
    widgets = widgets.map(w =>
      w.id === resizingWidget.id ? { ...w, size: { width: newWidth, height: newHeight } } : w
    );
  }

  function stopResize() {
    if (!resizingWidget) return;
    const widget = widgets.find(w => w.id === resizingWidget.id);
    resizingWidget = null;
    if (widget) resizeWidget(widget.id, widget.size.width, widget.size.height);
  }

  function startDrag(event, widget) {
    if (!event.target.closest('.widget-header')) return;
    if (event.target.closest('.close-btn')) return;

    draggedWidget = widget;
    dragOffset = {
      x: event.clientX - widget.position.x,
      y: event.clientY - widget.position.y
    };
    bringToFront(widget.id);

    event.preventDefault();
  }

  function onDrag(event) {
    if (resizingWidget) { onResize(event); return; }
    if (!draggedWidget) return;

    const newX = event.clientX - dragOffset.x;
    const newY = event.clientY - dragOffset.y;

    widgets = widgets.map(w =>
      w.id === draggedWidget.id
        ? { ...w, position: constrainToBounds({ x: newX, y: newY }, w.size, displayHeight(w)) }
        : w
    );
  }

  function stopDrag() {
    if (resizingWidget) { stopResize(); return; }
    if (!draggedWidget) return;

    const widget = widgets.find(w => w.id === draggedWidget.id);
    draggedWidget = null;

    if (widget) {
      invoke('widget_blackboard__update_widget_position', {
        widgetId: widget.id,
        position: widget.position
      }).catch(error => {
        console.error('Failed to update widget position:', error);
      });
    }
  }

  function resizeWidget(widgetId, width, height) {
    widgets = widgets.map(w =>
      w.id === widgetId ? { ...w, size: { width, height } } : w
    );
    invoke('widget_blackboard__update_widget_size', {
      widgetId,
      size: { width, height }
    }).catch(error => {
      console.error('Failed to update widget size:', error);
    });
  }

  export function minimizeAll() {
    widgets.forEach(w => {
      if (w.window_state !== 'minimized') updateWidgetWindowState(w.id, 'minimized');
    });
  }

  export function expandAll() {
    widgets.forEach(w => {
      if (w.window_state !== 'normal') updateWidgetWindowState(w.id, 'normal');
    });
  }

  function updateWidgetWindowState(widgetId, windowState) {
    widgets = widgets.map(w =>
      w.id === widgetId ? { ...w, window_state: windowState } : w
    );
    invoke('widget_blackboard__update_widget_window_state', {
      widgetId,
      windowState
    }).catch(error => {
      console.error('Failed to update widget window state:', error);
    });
  }

  onMount(async () => {
    updateViewportSize();

    await loadWidgets();

    const unlistenAdded = await listen('widget-added', (event) => {
      // If widget already exists (e.g. duplicate entity-created event), bring it to front
      const existingIdx = widgets.findIndex(w => w.id === event.payload.id);
      if (existingIdx >= 0) {
        bringToFront(event.payload.id);
        widgets = widgets.map(w =>
          w.id === event.payload.id ? { ...w, refreshKey: (w.refreshKey ?? 0) + 1 } : w
        );
        return;
      }

      topZIndex++;
      const newWidget = { ...event.payload, zIndex: topZIndex };

      newWidget.position = constrainToBounds(newWidget.position, newWidget.size, displayHeight(newWidget));
      widgets = [...widgets, newWidget];
    });

    const unlistenRemoved = await listen('widget-removed', (event) => {
      widgets = widgets.filter(w => w.id !== event.payload);
    });

    const unlistenCleared = await listen('widgets-cleared', () => {
      widgets = [];
    });

    unlisteners = [unlistenAdded, unlistenRemoved, unlistenCleared];

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
    class:resizing={resizingWidget?.id === widget.id}
    style:left="{widget.position.x}px"
    style:top="{widget.position.y}px"
    style:width="{widget.size.width}px"
    style:height="{widget.window_state === 'minimized' ? MINIMIZED_HEIGHT : widget.size.height}px"
    style:z-index={widget.zIndex}
    onmousedown={(e) => startDrag(e, widget)}
    onclick={() => bringToFront(widget.id)}
    onkeydown={(e) => (e.key === 'Enter' || e.key === ' ') && bringToFront(widget.id)}
    role="dialog"
    tabindex="0"
    aria-label="Widget"
    in:fly={{ x: -viewportWidth, duration: WIDGET_FLY_DURATION, opacity: 1, easing: cubicOut }}
    out:fly={{ x: -viewportWidth, duration: WIDGET_FLY_DURATION, opacity: 1, easing: cubicIn }}
  >
    {#if widget.window_state !== 'minimized'}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div class="resize-handle" onmousedown={(e) => startResize(e, widget)}></div>
    {/if}
    {#if widget.widget_type === 'inspector'}
      <InspectorWidget entityId={widget.entity_id} widgetId={widget.id} refreshKey={widget.refreshKey ?? 0} windowState={widget.window_state ?? 'normal'} onWindowStateChange={(state) => updateWidgetWindowState(widget.id, state)} />
    {:else if widget.widget_type === 'mermaid'}
      <MermaidWidget widgetId={widget.id} entityId={widget.entity_id} windowState={widget.window_state ?? 'normal'} onWindowStateChange={(state) => updateWidgetWindowState(widget.id, state)} />
    {:else if widget.widget_type === 'process_status'}
      <ProcessStatusWidget widgetId={widget.id} entityId={widget.entity_id} />
    {:else if widget.widget_type === 'connector_credential'}
      <ConnectorCredentialWidget widgetId={widget.id} entityId={widget.entity_id} />
    {:else if widget.widget_type === 'connector_manager'}
      <ConnectorManagerWidget widgetId={widget.id} entityId={widget.entity_id} />
    {:else if widget.widget_type === 'meta_process'}
      <MetaProcessWidget widgetId={widget.id} entityId={widget.entity_id} windowState={widget.window_state ?? 'normal'} onWindowStateChange={(state) => updateWidgetWindowState(widget.id, state)} />
    {/if}
  </div>
{/each}

<style>
  .widget-container {
    position: absolute;
    transition: height 0.25s cubic-bezier(0.4, 0, 0.2, 1), box-shadow 0.2s;
  }

  .widget-container.dragging,
  .widget-container.resizing {
    transition: none;
  }

  .widget-container.dragging {
    cursor: grabbing;
    box-shadow: 0 12px 48px color-mix(in srgb, var(--color-black) 60%, transparent);
  }

  .resize-handle {
    position: absolute;
    bottom: 0;
    right: 0;
    width: 18px;
    height: 18px;
    cursor: nwse-resize;
    z-index: 10;
  }

  .resize-handle::after {
    content: '';
    position: absolute;
    bottom: 4px;
    right: 4px;
    width: 0;
    height: 0;
    border-style: solid;
    border-width: 0 0 9px 9px;
    border-color: transparent transparent color-mix(in srgb, var(--color-white) 25%, transparent) transparent;
    transition: border-color 0.15s;
  }

  .resize-handle:hover::after {
    border-color: transparent transparent color-mix(in srgb, var(--color-interactive) 70%, transparent) transparent;
  }

  .widget-container :global(.widget-header) {
    cursor: grab;
  }

  .widget-container.dragging :global(.widget-header) {
    cursor: grabbing;
  }

</style>
