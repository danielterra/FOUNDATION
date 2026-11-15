<script>
  import { onMount, onDestroy, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { fly } from 'svelte/transition';
  import { cubicOut, cubicIn } from 'svelte/easing';
  import { Button } from '$lib/components/ui/button';
  import InspectorWidget from './InspectorWidget.svelte';
  import MermaidWidget from './MermaidWidget.svelte';
  import ProcessStatusWidget from './ProcessStatusWidget.svelte';
  import ConnectorCredentialWidget from './ConnectorCredentialWidget.svelte';
  import ConnectorManagerWidget from './ConnectorManagerWidget.svelte';
  import DataSyncManagerWidget from './DataSyncManagerWidget.svelte';
  import DataSyncRawInspectorWidget from './DataSyncRawInspectorWidget.svelte';
  import AutomationWidget from './AutomationWidget.svelte';
  import WorkflowExecutionWidget from './WorkflowExecutionWidget.svelte';
  import GraphWidget from './GraphWidget.svelte';
  import AIConversationWidget from './AIConversationWidget.svelte';
  import NotificationCenterWidget from './NotificationCenterWidget.svelte';
  import AICallHistoryWidget from './AICallHistoryWidget.svelte';
  import EmailViewerWidget from './EmailViewerWidget.svelte';
  import JsonRenderWidget from './JsonRenderWidget.svelte';
  import SemanticJsonRenderWidget from './SemanticJsonRenderWidget.svelte';
  import WidgetSidebar from './WidgetSidebar.svelte';
  import { openEntity } from '$lib/blackboard';

  let { activeBlackboardIri = null, activeConversationIri = null } = $props();

  // Mirrors --z-widget-min and --z-widget-max from app.css
  const BASE_WIDGET_Z_INDEX = 100;
  const MAX_WIDGET_Z_INDEX = 9000;
  const WIDGET_FLY_DURATION = 600;
  const MIN_CANVAS_SCALE = 0.3;
  const MAX_CANVAS_SCALE = 2.5;
  const CANVAS_SCALE_STEP = 0.1;

  let widgets = $state([]);
  let unlisteners = [];
  let widgetLoadRequestId = 0;
  let managerDestroyed = false;
  let draggedWidget = $state(null);
  let dragOffset = $state({ x: 0, y: 0 });
  let panningCanvas = $state(false);
  let panStart = $state({ mouseX: 0, mouseY: 0, cameraX: 0, cameraY: 0 });
  let camera = $state({ x: 0, y: 0, scale: 1 });
  let topZIndex = $state(BASE_WIDGET_Z_INDEX);
  let viewportWidth = $state(0);
  let canvasElement = $state(null);
  let reducedMotion = $state(false);
  let motionPreference = null;
  const pendingEntityOpens = new Set();
  let widgetAnnouncement = $state('');

  const widgetTypeInfo = new Map();

  let entityResolutionByIri = $state(new Map());

  const sidebarEntries = $derived(
    [...widgets]
      .sort((a, b) => b.zIndex - a.zIndex)
      .map((w, i) => {
        const entity = w.entity_id ? entityResolutionByIri.get(w.entity_id) : undefined;
        const typeFallback = widgetTypeInfo.get(w.widget_type);
        return {
          id: w.id,
          icon: entity?.icon || typeFallback?.icon || 'widgets',
          title: entity?.label || typeFallback?.description || w.widget_type,
          isFocused: i === 0 && w.window_state !== 'minimized',
          isMinimized: w.window_state === 'minimized'
        };
      })
  );

  const hasOpenWidgets = $derived(widgets.some(w => w.window_state !== 'minimized'));
  const hasMinimizedWidgets = $derived(widgets.some(w => w.window_state === 'minimized'));

  $effect(() => {
    const entityIds = [...new Set(widgets.map(w => w.entity_id).filter(Boolean))];
    if (entityIds.length === 0) return;
    invoke('entity__resolve_iris', { iris: entityIds })
      .then(json => {
        const parsed = JSON.parse(json);
        const next = new Map();
        for (const [iri, val] of Object.entries(parsed)) {
          next.set(iri, val);
        }
        entityResolutionByIri = next;
      })
      .catch(() => {});
  });

  async function loadWidgets(blackboardIri) {
    const requestId = ++widgetLoadRequestId;
    if (!blackboardIri) {
      widgets = [];
      return;
    }
    try {
      const result = await invoke('widget_blackboard__get_widgets', { blackboardId: blackboardIri });
      if (requestId !== widgetLoadRequestId || blackboardIri !== activeBlackboardIri) return;
      widgets = result.map((w, index) => ({
        ...w,
        zIndex: BASE_WIDGET_Z_INDEX + index
      }));
      topZIndex = Math.max(BASE_WIDGET_Z_INDEX, ...widgets.map(w => w.zIndex));

    } catch (error) {
      if (requestId !== widgetLoadRequestId || blackboardIri !== activeBlackboardIri) return;
      console.error('Failed to load widgets:', error);
    }
  }

  $effect(() => {
    loadWidgets(activeBlackboardIri);
  });

  function bringToFront(widgetId) {
    topZIndex = Math.min(topZIndex + 1, MAX_WIDGET_Z_INDEX);
    widgets = widgets.map(w =>
      w.id === widgetId ? { ...w, zIndex: topZIndex } : w
    );
  }

  async function focusWidget(widgetId) {
    await tick();
    await new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));

    const element = [...(canvasElement?.querySelectorAll('.widget-container') ?? [])]
      .find(candidate => candidate.dataset.widgetId === widgetId);
    const canvasRect = canvasElement?.getBoundingClientRect();
    if (!element || !canvasRect) return;

    const widgetWidth = element.offsetWidth;
    const widgetHeight = element.offsetHeight;
    if (!widgetWidth || !widgetHeight) return;

    const margin = 32;
    const availableWidth = Math.max(1, canvasRect.width - margin * 2);
    const availableHeight = Math.max(1, canvasRect.height - margin * 2);
    const scale = Math.max(
      MIN_CANVAS_SCALE,
      Math.min(MAX_CANVAS_SCALE, availableWidth / widgetWidth, availableHeight / widgetHeight)
    );
    const widget = widgets.find(candidate => candidate.id === widgetId);
    if (!widget) return;

    camera = {
      scale,
      x: canvasRect.width / 2 - (widget.position.x + widgetWidth / 2) * scale,
      y: canvasRect.height / 2 - (widget.position.y + widgetHeight / 2) * scale
    };
  }

  function entityOpenKey(entityIri) {
    return `${activeBlackboardIri ?? 'default'}:${entityIri}`;
  }

  async function openReferencedEntity(entityIri) {
    if (!entityIri) return;

    const key = entityOpenKey(entityIri);
    if (pendingEntityOpens.has(key)) {
      widgetAnnouncement = 'Entidade já está sendo aberta.';
      return;
    }

    pendingEntityOpens.add(key);
    widgetAnnouncement = `Abrindo entidade: ${entityResolutionByIri.get(entityIri)?.label ?? entityIri}`;
    try {
      await openEntity(entityIri, activeConversationIri);
    } catch (error) {
      widgetAnnouncement = 'Não foi possível abrir a entidade.';
      console.error('Failed to open referenced entity:', error);
    } finally {
      pendingEntityOpens.delete(key);
    }
  }

  function startDrag(event, widget) {
    if (event.button !== 0 || !event.target.closest('.widget-header')) return;
    if (event.target.closest('button, a, input, textarea, select, [contenteditable="true"], [role="button"], .header-actions')) return;

    draggedWidget = widget;
    dragOffset = {
      x: (event.clientX - camera.x) / camera.scale - widget.position.x,
      y: (event.clientY - camera.y) / camera.scale - widget.position.y
    };
    bringToFront(widget.id);

    event.preventDefault();
  }

  function onDrag(event) {
    if (panningCanvas) {
      camera = {
        ...camera,
        x: panStart.cameraX + event.clientX - panStart.mouseX,
        y: panStart.cameraY + event.clientY - panStart.mouseY
      };
      return;
    }
    if (!draggedWidget) return;

    const newX = (event.clientX - camera.x) / camera.scale - dragOffset.x;
    const newY = (event.clientY - camera.y) / camera.scale - dragOffset.y;

    widgets = widgets.map(w =>
      w.id === draggedWidget.id
        ? { ...w, position: { x: newX, y: newY } }
        : w
    );
  }

  function stopDrag() {
    if (panningCanvas) {
      panningCanvas = false;
      return;
    }
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

  function moveWidget(widgetId, position) {
    widgets = widgets.map(w =>
      w.id === widgetId ? { ...w, position } : w
    );
    invoke('widget_blackboard__update_widget_position', {
      widgetId,
      position
    }).catch(error => {
      console.error('Failed to update widget position:', error);
    });
  }

  function startCanvasPan(event) {
    if (event.button === 0 && event.target.closest('.canvas-controls')) return;
    if (event.button === 0 && event.target.closest('.widget-container')) return;
    if (event.button !== 0 && event.button !== 1) return;
    panningCanvas = true;
    panStart = { mouseX: event.clientX, mouseY: event.clientY, cameraX: camera.x, cameraY: camera.y };
    event.preventDefault();
  }

  function zoomAt(clientX, clientY, nextScale) {
    const rect = canvasElement?.getBoundingClientRect();
    if (!rect) return;
    const anchorX = clientX - rect.left;
    const anchorY = clientY - rect.top;
    const scale = Math.max(MIN_CANVAS_SCALE, Math.min(MAX_CANVAS_SCALE, nextScale));
    const worldX = (anchorX - camera.x) / camera.scale;
    const worldY = (anchorY - camera.y) / camera.scale;
    camera = { x: anchorX - worldX * scale, y: anchorY - worldY * scale, scale };
  }

  function changeZoom(direction) {
    const rect = canvasElement?.getBoundingClientRect();
    if (!rect) return;
    zoomAt(rect.left + rect.width / 2, rect.top + rect.height / 2, camera.scale + direction * CANVAS_SCALE_STEP);
  }

  function onCanvasWheel(event) {
    event.preventDefault();
    if (event.ctrlKey || event.metaKey) {
      zoomAt(event.clientX, event.clientY, camera.scale * Math.exp(-event.deltaY * 0.002));
      return;
    }
    camera = { ...camera, x: camera.x - event.deltaX, y: camera.y - event.deltaY };
  }

  function onCanvasKeydown(event) {
    if (event.target !== event.currentTarget) return;
    const panStep = event.shiftKey ? 48 : 16;
    const movement = {
      ArrowLeft: { x: panStep, y: 0 }, ArrowRight: { x: -panStep, y: 0 },
      ArrowUp: { x: 0, y: panStep }, ArrowDown: { x: 0, y: -panStep }
    }[event.key];
    if (movement) {
      camera = { ...camera, x: camera.x + movement.x, y: camera.y + movement.y };
      event.preventDefault();
    } else if (event.key === '+' || event.key === '=') {
      changeZoom(1);
      event.preventDefault();
    } else if (event.key === '-') {
      changeZoom(-1);
      event.preventDefault();
    }
  }

  function moveWidgetByKeyboard(event, widget) {
    if (event.target !== event.currentTarget || !event.altKey) return;
    const step = event.shiftKey ? 40 : 10;
    const movement = {
      ArrowLeft: { x: -step, y: 0 }, ArrowRight: { x: step, y: 0 },
      ArrowUp: { x: 0, y: -step }, ArrowDown: { x: 0, y: step }
    }[event.key];
    if (!movement) return;
    event.preventDefault();
    const position = { x: widget.position.x + movement.x, y: widget.position.y + movement.y };
    moveWidget(widget.id, position);
    bringToFront(widget.id);
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
    const widget = widgets.find(w => w.id === widgetId);
    if (windowState === 'normal' && widget?.window_state === 'minimized') {
      restoreFromMinimized(widgetId);
      return;
    }

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

  function restoreFromMinimized(widgetId) {
    const widget = widgets.find(w => w.id === widgetId);
    if (!widget) return;

    widgets = widgets.map(w =>
      w.id === widgetId ? { ...w, window_state: 'normal' } : w
    );
    invoke('widget_blackboard__update_widget_window_state', {
      widgetId,
      windowState: 'normal'
    }).catch(error => {
      console.error('Failed to update widget window state:', error);
    });
    bringToFront(widgetId);
    focusWidget(widgetId);
  }

  function onSidebarSelect(widgetId) {
    const widget = widgets.find(w => w.id === widgetId);
    if (!widget) return;
    if (widget.window_state === 'minimized') {
      restoreFromMinimized(widgetId);
    } else {
      bringToFront(widgetId);
      focusWidget(widgetId);
    }
  }

  function handleWidgetAdded(event) {
    const w = event.payload;
    if (w.blackboard_iri !== activeBlackboardIri) return;

    if (w.entity_id) {
      pendingEntityOpens.delete(entityOpenKey(w.entity_id));
      widgetAnnouncement = `Entidade aberta: ${entityResolutionByIri.get(w.entity_id)?.label ?? w.entity_id}`;
    }

    widgetLoadRequestId += 1;
    const existingIdx = widgets.findIndex(existing => existing.id === w.id);
    if (existingIdx >= 0) {
      const existing = widgets[existingIdx];
      if (existing.window_state === 'minimized') restoreFromMinimized(w.id);
      else {
        bringToFront(w.id);
        focusWidget(w.id);
      }
      return;
    }

    topZIndex = Math.min(topZIndex + 1, MAX_WIDGET_Z_INDEX);
    const newWidget = { ...w, zIndex: topZIndex };
    widgets = [...widgets, newWidget];
    focusWidget(w.id);
  }

  function handleWidgetRemoved(event) {
    widgetLoadRequestId += 1;
    widgets = widgets.filter(w => w.id !== event.payload);
    loadWidgets(activeBlackboardIri);
  }

  function handleWidgetsCleared() {
    widgetLoadRequestId += 1;
    widgets = [];
  }

  function removeListeners(listeners) {
    for (const unlisten of listeners) unlisten();
  }

  function updateMotionPreference() {
    reducedMotion = motionPreference?.matches ?? false;
  }

  async function initializeWidgetManager() {
    motionPreference = window.matchMedia('(prefers-reduced-motion: reduce)');
    updateMotionPreference();
    motionPreference.addEventListener('change', updateMotionPreference);

    const registeredListeners = await Promise.all([
      listen('widget-added', handleWidgetAdded),
      listen('widget-removed', handleWidgetRemoved),
      listen('widgets-cleared', handleWidgetsCleared),
    ]);
    if (managerDestroyed) {
      removeListeners(registeredListeners);
      return;
    }
    unlisteners = registeredListeners;

    try {
      const defs = await invoke('widget_blackboard__list_widget_definitions', { classIri: null });
      for (const definition of defs) {
        widgetTypeInfo.set(definition.widget_type, {
          description: definition.description,
          icon: definition.icon,
        });
      }
    } catch (e) {
      console.error('Failed to load widget definitions:', e);
    }

    if (managerDestroyed) return;

    document.addEventListener('mousemove', onDrag);
    document.addEventListener('mouseup', stopDrag);
    canvasElement?.addEventListener('wheel', onCanvasWheel, { passive: false });
  }

  function cleanupWidgetManager() {
    managerDestroyed = true;
    motionPreference?.removeEventListener('change', updateMotionPreference);
    removeListeners(unlisteners);
    document.removeEventListener('mousemove', onDrag);
    document.removeEventListener('mouseup', stopDrag);
    canvasElement?.removeEventListener('wheel', onCanvasWheel);
  }

  onMount(initializeWidgetManager);
  onDestroy(cleanupWidgetManager);
</script>

<div class="widget-manager-layout">
  {#if widgets.length > 0}
    <WidgetSidebar
      entries={sidebarEntries}
      onSelect={onSidebarSelect}
      onMinimizeAll={minimizeAll}
      onRestoreAll={expandAll}
      {hasOpenWidgets}
      {hasMinimizedWidgets}
    />
  {/if}
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="widget-canvas"
    class:panning={panningCanvas}
    bind:this={canvasElement}
    bind:clientWidth={viewportWidth}
    onmousedown={startCanvasPan}
    onkeydown={onCanvasKeydown}
    style="--canvas-scale: {camera.scale}; --canvas-pan-x: {camera.x}px; --canvas-pan-y: {camera.y}px;"
    role="application"
    tabindex="0"
    aria-label="Lousa. Use as setas para mover e mais ou menos para ajustar o zoom."
  >
    <div class="sr-only" aria-live="polite" aria-atomic="true">{widgetAnnouncement}</div>
    <div class="canvas-controls" aria-label="Controles da lousa">
      <Button variant="ghost" size="icon-touch" class="canvas-control" aria-label="Diminuir zoom" title="Diminuir zoom" disabled={camera.scale <= MIN_CANVAS_SCALE} onclick={() => changeZoom(-1)}>
        <span class="material-symbols-outlined">remove</span>
      </Button>
      <span class="canvas-scale" aria-live="polite">{Math.round(camera.scale * 100)}%</span>
      <Button variant="ghost" size="icon-touch" class="canvas-control" aria-label="Aumentar zoom" title="Aumentar zoom" disabled={camera.scale >= MAX_CANVAS_SCALE} onclick={() => changeZoom(1)}>
        <span class="material-symbols-outlined">add</span>
      </Button>
      <Button variant="ghost" size="icon-touch" class="canvas-control" aria-label="Restaurar zoom" title="Restaurar zoom" onclick={() => { camera = { ...camera, x: 0, y: 0, scale: 1 }; }}>
        <span class="material-symbols-outlined">center_focus_strong</span>
      </Button>
    </div>
    <div class="widget-world" style="transform: translate({camera.x}px, {camera.y}px) scale({camera.scale});">
{#each widgets.filter(w => w.window_state !== 'minimized') as widget (widget.id)}
  <div
    class="widget-container"
    data-widget-id={widget.id}
    class:dragging={draggedWidget?.id === widget.id}
    class:is-front={widget.zIndex === topZIndex}
    style:left="{widget.position.x}px"
    style:top="{widget.position.y}px"
    style:width="{widget.width}px"
    style:z-index={widget.zIndex}
    onmousedown={(e) => startDrag(e, widget)}
    onclick={() => bringToFront(widget.id)}
    onkeydown={(e) => { moveWidgetByKeyboard(e, widget); if (e.key === 'Enter' || e.key === ' ') bringToFront(widget.id); }}
    role="dialog"
    tabindex="0"
    aria-label="Widget"
    in:fly={{ x: -viewportWidth, duration: reducedMotion ? 0 : WIDGET_FLY_DURATION, opacity: 1, easing: cubicOut }}
    out:fly={{ x: -viewportWidth, duration: reducedMotion ? 0 : WIDGET_FLY_DURATION, opacity: 1, easing: cubicIn }}
  >
    {#if widget.widget_type === 'inspector'}
      <InspectorWidget entityId={widget.entity_id} widgetId={widget.id} refreshKey={widget.refreshKey ?? 0} windowState={widget.window_state ?? 'normal'} onWindowStateChange={(state) => updateWidgetWindowState(widget.id, state)} onOpenEntityInspector={openReferencedEntity} conversationIri={activeConversationIri} />
    {:else if widget.widget_type === 'mermaid'}
      <MermaidWidget widgetId={widget.id} entityId={widget.entity_id} conversationIri={activeConversationIri} windowState={widget.window_state ?? 'normal'} onWindowStateChange={(state) => updateWidgetWindowState(widget.id, state)} />
    {:else if widget.widget_type === 'process_status'}
      <ProcessStatusWidget widgetId={widget.id} entityId={widget.entity_id} windowState={widget.window_state ?? 'normal'} onWindowStateChange={(state) => updateWidgetWindowState(widget.id, state)} />
    {:else if widget.widget_type === 'connector_credential'}
      <ConnectorCredentialWidget widgetId={widget.id} entityId={widget.entity_id} windowState={widget.window_state ?? 'normal'} onWindowStateChange={(state) => updateWidgetWindowState(widget.id, state)} />
    {:else if widget.widget_type === 'connector_manager'}
      <ConnectorManagerWidget widgetId={widget.id} entityId={widget.entity_id} conversationIri={activeConversationIri} windowState={widget.window_state ?? 'normal'} onWindowStateChange={(state) => updateWidgetWindowState(widget.id, state)} />
    {:else if widget.widget_type === 'datasync_manager'}
      <DataSyncManagerWidget widgetId={widget.id} conversationIri={activeConversationIri} windowState={widget.window_state ?? 'normal'} onWindowStateChange={(state) => updateWidgetWindowState(widget.id, state)} />
    {:else if widget.widget_type === 'datasync_raw_inspector'}
      <DataSyncRawInspectorWidget widgetId={widget.id} entityId={widget.entity_id} windowState={widget.window_state ?? 'normal'} onWindowStateChange={(state) => updateWidgetWindowState(widget.id, state)} />
    {:else if widget.widget_type === 'automation'}
      <AutomationWidget widgetId={widget.id} entityId={widget.entity_id} windowState={widget.window_state ?? 'normal'} onWindowStateChange={(state) => updateWidgetWindowState(widget.id, state)} conversationIri={activeConversationIri} />
    {:else if widget.widget_type === 'workflow_execution'}
      <WorkflowExecutionWidget widgetId={widget.id} entityId={widget.entity_id} conversationIri={activeConversationIri} windowState={widget.window_state ?? 'normal'} onWindowStateChange={(state) => updateWidgetWindowState(widget.id, state)} />
    {:else if widget.widget_type === 'graph'}
      <GraphWidget widgetId={widget.id} entityId={widget.entity_id} conversationIri={activeConversationIri} windowState={widget.window_state ?? 'normal'} onWindowStateChange={(state) => updateWidgetWindowState(widget.id, state)} />
    {:else if widget.widget_type === 'ai_conversation'}
      <AIConversationWidget widgetId={widget.id} entityId={widget.entity_id} conversationIri={activeConversationIri} windowState={widget.window_state ?? 'normal'} onWindowStateChange={(state) => updateWidgetWindowState(widget.id, state)} />
    {:else if widget.widget_type === 'notification_center'}
      <NotificationCenterWidget widgetId={widget.id} windowState={widget.window_state ?? 'normal'} onWindowStateChange={(state) => updateWidgetWindowState(widget.id, state)} conversationIri={activeConversationIri} />
    {:else if widget.widget_type === 'ai_call_history'}
      <AICallHistoryWidget widgetId={widget.id} windowState={widget.window_state ?? 'normal'} onWindowStateChange={(state) => updateWidgetWindowState(widget.id, state)} />
    {:else if widget.widget_type === 'email_viewer'}
      <EmailViewerWidget widgetId={widget.id} entityId={widget.entity_id} conversationIri={activeConversationIri} windowState={widget.window_state ?? 'normal'} onWindowStateChange={(state) => updateWidgetWindowState(widget.id, state)} />
    {:else if widget.widget_type === 'json_render'}
      <JsonRenderWidget widgetId={widget.id} entityId={widget.entity_id} conversationIri={activeConversationIri} windowState={widget.window_state ?? 'normal'} onWindowStateChange={(state) => updateWidgetWindowState(widget.id, state)} />
    {:else if widget.widget_type === 'semantic_json_render'}
      <SemanticJsonRenderWidget widgetId={widget.id} entityId={widget.entity_id} specTemplateIri={widget.spec_template_iri ?? null} conversationIri={activeConversationIri} windowState={widget.window_state ?? 'normal'} onWindowStateChange={(state) => updateWidgetWindowState(widget.id, state)} />
    {/if}
  </div>
{/each}
    </div>
  </div>
</div>

<style>
  .widget-manager-layout {
    position: relative;
    display: flex;
    flex-direction: row;
    width: 100%;
    height: 100%;
    overflow: hidden;
    gap: 5px;
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }

  .widget-canvas {
    position: relative;
    flex: 1;
    height: 100%;
    min-width: 0;
    background-image: radial-gradient(var(--color-hud-grid-dot) var(--size-hud-grid-dot), transparent var(--size-hud-grid-dot));
    background-size: var(--size-hud-grid) var(--size-hud-grid);
    background-position: var(--canvas-pan-x) var(--canvas-pan-y);
    overflow: hidden;
    cursor: grab;
    touch-action: none;
  }

  .widget-canvas:focus-visible {
    outline: var(--size-hud-frame-line) solid var(--color-selection);
    outline-offset: calc(-1 * var(--size-hud-frame-line));
  }

  .widget-canvas.panning {
    cursor: grabbing;
  }

  .canvas-controls {
    position: absolute;
    z-index: 10000;
    top: var(--spacing-widget-inset);
    right: var(--spacing-widget-inset);
    display: flex;
    align-items: center;
    gap: var(--spacing-inline-tight);
    padding: var(--spacing-inline-tight);
    background: color-mix(in srgb, var(--color-surface-1) 92%, transparent);
    backdrop-filter: blur(var(--blur-widget-surface));
  }

  .canvas-scale {
    min-width: 4ch;
    color: var(--text-metadata);
    font-family: var(--font-mono);
    font-size: var(--type-caption-compact);
    text-align: center;
  }

  :global([data-slot="button"].canvas-control) {
    color: var(--color-interactive);
  }

  :global([data-slot="button"].canvas-control:disabled) {
    color: var(--color-interactive-disabled);
  }

  .widget-world {
    position: absolute;
    inset: 0;
    width: 100%;
    min-height: 100%;
    transform-origin: 0 0;
    will-change: transform;
    pointer-events: none;
  }

  .widget-container {
    position: absolute;
    transition:
      box-shadow 560ms cubic-bezier(0.16, 1, 0.3, 1);
    box-shadow: 0px 0px 10px var(--background);
    pointer-events: auto;
    height: max-content;
    min-height: max-content;
  }

  @media (prefers-reduced-motion: reduce) {
    .widget-container {
      transition: none;
    }
  }

  .widget-container.is-front {
    --widget-selection-accent: var(--color-selection);
    --widget-selection-glow: var(--color-selection-glow);
    box-shadow: 0 16px 40px color-mix(in srgb, var(--color-black) 58%, transparent);
  }

  .widget-container.dragging {
    transition: none;
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
