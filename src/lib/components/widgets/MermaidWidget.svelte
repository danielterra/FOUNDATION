<script>
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { createEntitySubscription } from '$lib/realtime/subscriptions';
  import mermaid from 'mermaid';
  import WidgetContainer from './WidgetContainer.svelte';
  import { Button } from '$lib/components/ui/button';
  import { Textarea } from '$lib/components/ui/textarea';

  let { widgetId, entityId = '', conversationIri = null, windowState = 'normal', onWindowStateChange } = $props();

  const DEFAULT_DIAGRAM = `graph TD
    A[Start] --> B{Decision}
    B -->|Yes| C[Result A]
    B -->|No| D[Result B]`;

  let content = $state(DEFAULT_DIAGRAM);
  let entityLabel = $state('');
  let entityLoading = $state(true);
  let editMode = $state(false);
  let draftContent = $state('');
  $effect(() => { if (!editMode) draftContent = content; });
  let renderContainer = $state(null);
  let renderError = $state(null);
  let scale = $state(1);
  let translateX = $state(0);
  let translateY = $state(0);
  let isDragging = $state(false);
  let dragStart = { x: 0, y: 0 };
  let dragMoved = false;
  let unlistenContentUpdated = null;
  const entitySub = createEntitySubscription(async (event) => {
    if (event.type !== 'updated') return;
    try {
      const resultStr = await invoke('inspector__get_entity', { entityId });
      const data = JSON.parse(resultStr);
      const source = data?.properties?.find(p => p.property === 'foundation:diagramSource')?.value;
      if (source && source !== content) {
        content = source;
        draftContent = source;
        await renderDiagram();
      }
    } catch {
      // ignore
    }
  });
  let renderCount = 0;

  mermaid.initialize({
    startOnLoad: false,
    theme: 'dark',
    themeVariables: {
      background: 'transparent',
      mainBkg: '#1e293b',
      primaryColor: '#2563eb',
      primaryTextColor: '#f8fafc',
      primaryBorderColor: '#60a5fa',
      lineColor: '#94a3b8',
      secondaryColor: '#1e3a5f',
      tertiaryColor: '#0f2027',
      nodeBorder: '#60a5fa',
      clusterBkg: '#1e293b',
      clusterBorder: '#475569',
      titleColor: '#f1f5f9',
      edgeLabelBackground: '#1e293b',
      nodeTextColor: '#f8fafc',
      fontFamily: 'ui-sans-serif, system-ui, sans-serif',
    },
  });

  function fixSvgForWebkit(container) {
    const svgEl = container?.querySelector('svg');
    if (!svgEl) return;

    // Fix 1: Set explicit pixel dimensions.
    // WebKit (Tauri WKWebView) cannot compute SVG viewport height when width="100%"
    // and no height is set, resulting in a zero-height viewport that clips all drawing elements.
    const viewBox = svgEl.getAttribute('viewBox');
    if (viewBox) {
      const parts = viewBox.trim().split(/[\s,]+/);
      if (parts.length === 4) {
        const vbWidth = parseFloat(parts[2]);
        const vbHeight = parseFloat(parts[3]);
        if (vbWidth > 0 && vbHeight > 0) {
          svgEl.setAttribute('width', String(vbWidth));
          svgEl.setAttribute('height', String(vbHeight));
          svgEl.style.removeProperty('max-width');
        }
      }
    }

    // Fix 2: Move SVG style element to the HTML container.
    // WebKit does not apply CSS from style elements embedded inside SVG when the SVG
    // is inserted via innerHTML. SVG elements fall back to default styles (black fill,
    // no stroke) making paths and rects invisible. Moving the style into the HTML scope
    // ensures the selectors are evaluated correctly.
    const svgStyle = svgEl.querySelector(':scope > style');
    if (svgStyle) {
      const htmlStyle = document.createElement('style');
      htmlStyle.textContent = svgStyle.textContent;
      container.insertBefore(htmlStyle, svgEl);
      svgStyle.remove();
    }
  }

  async function renderDiagram() {
    if (!renderContainer) return;
    renderError = null;
    try {
      const id = `mermaid-${widgetId.replace(/[^a-zA-Z0-9]/g, '_')}_${++renderCount}`;
      const { svg } = await mermaid.render(id, content);
      renderContainer.innerHTML = svg;
      fixSvgForWebkit(renderContainer);
    } catch (err) {
      renderError = err?.message ?? String(err);
      renderContainer.innerHTML = '';
    }
  }

  function resetView() {
    scale = 1;
    translateX = 0;
    translateY = 0;
  }

  function handleKeydown(e) {
    if (e.key === 'ArrowLeft') translateX += 20;
    else if (e.key === 'ArrowRight') translateX -= 20;
    else if (e.key === 'ArrowUp') translateY += 20;
    else if (e.key === 'ArrowDown') translateY -= 20;
    else if (e.key === '+' || e.key === '=') scale = Math.min(50, scale * 1.1);
    else if (e.key === '-') scale = Math.max(0.1, scale * 0.9);
    else if (e.key === '0') resetView();
  }

  function handleWheel(e) {
    e.preventDefault();
    if (e.ctrlKey) {
      const factor = e.deltaY > 0 ? 0.97 : 1.03;
      scale = Math.max(0.1, Math.min(50, scale * factor));
    } else {
      translateX -= e.deltaX;
      translateY -= e.deltaY;
    }
  }

  function handleMouseDown(e) {
    isDragging = true;
    dragMoved = false;
    dragStart = { x: e.clientX - translateX, y: e.clientY - translateY };
  }

  function handleMouseMove(e) {
    if (!isDragging) return;
    const newX = e.clientX - dragStart.x;
    const newY = e.clientY - dragStart.y;
    if (Math.abs(newX - translateX) > 3 || Math.abs(newY - translateY) > 3) dragMoved = true;
    translateX = newX;
    translateY = newY;
  }

  function handleMouseUp() {
    isDragging = false;
  }

  function handleCanvasClick(e) {
    if (dragMoved) return;
    let el = e.target;
    while (el && el !== e.currentTarget) {
      if (el.tagName === 'g' && (el.classList.contains('node') || el.dataset?.node === 'true')) {
        const label =
          el.querySelector('foreignObject')?.textContent?.trim() ||
          el.querySelector('.label')?.textContent?.trim() ||
          el.querySelector('text')?.textContent?.trim();
        if (label) {
          document.dispatchEvent(new CustomEvent('chat-inject', {
            detail: { text: `[diagram:${widgetId}] "${label}"` }
          }));
        }
        return;
      }
      el = el.parentElement;
    }
  }

  async function saveContent() {
    try {
      await invoke('widget_blackboard__update_widget_content', { widgetId, content: draftContent });
      content = draftContent;
      editMode = false;
      await renderDiagram();
    } catch (err) {
      console.error('Failed to save mermaid content:', err);
    }
  }

  function cancelEdit() {
    draftContent = content;
    editMode = false;
  }

  async function openInspector() {
    await invoke('widget_blackboard__add_widget', {
      widgetType: 'inspector',
      entityId,
      position: null,
      size: null,
      conversationId: conversationIri,
    }).catch(() => {});
  }

  async function closeWidget() {
    try {
      await invoke('widget_blackboard__remove_widget', { widgetId });
    } catch (err) {
      console.error('Failed to remove widget:', err);
    }
  }

  $effect(() => {
    if (!editMode && renderContainer && !entityLoading) {
      renderDiagram();
    }
  });

  onMount(async () => {
    if (entityId) {
      try {
        const resultStr = await invoke('inspector__get_entity', { entityId });
        const data = JSON.parse(resultStr);
        entityLabel = data?.label ?? '';
        const source = data?.properties?.find(p => p.property === 'foundation:diagramSource')?.value;
        if (source) {
          content = source;
          draftContent = source;
        }
      } catch {
        // keep defaults
      }
    }
    entityLoading = false;

    unlistenContentUpdated = await listen('widget-content-updated', (event) => {
      if (event.payload === widgetId) renderDiagram();
    });

  });

  onDestroy(() => {
    if (unlistenContentUpdated) unlistenContentUpdated();
    entitySub.destroy();
  });

  $effect(() => {
    entitySub.setIris(entityId ? [entityId] : []);
  });
</script>

{#snippet editActions()}
  <Button variant="ghost" size="icon" class="confirm-action-btn" onclick={saveContent} title="Save"><span class="material-symbols-outlined">check</span></Button>
  <Button variant="ghost" size="icon" onclick={cancelEdit} title="Cancel"><span class="material-symbols-outlined">close</span></Button>
{/snippet}

{#snippet normalActions()}
  <Button variant="ghost" size="icon" onclick={resetView} title="Reset view"><span class="material-symbols-outlined">center_focus_strong</span></Button>
  <Button variant="ghost" size="icon" onclick={() => { draftContent = content; editMode = true; }} title="Edit diagram"><span class="material-symbols-outlined">edit</span></Button>
  <Button variant="ghost" size="icon" onclick={openInspector} title="Open inspector"><span class="material-symbols-outlined">info</span></Button>
{/snippet}

<WidgetContainer
  icon="account_tree"
  title={entityLabel || 'Mermaid Diagram'}
  {windowState}
  {onWindowStateChange}
  onClose={closeWidget}
  overrideActions={editMode ? editActions : undefined}
  headerActions={editMode ? undefined : normalActions}
>
  {#if entityLoading}
    <div class="loading">
      <span class="material-symbols-outlined spinning">progress_activity</span>
    </div>
  {:else if editMode}
    <Textarea
      class="diagram-editor"
      bind:value={draftContent}
      spellcheck={false}
      placeholder="Enter Mermaid diagram source..."
    />
  {:else}
    <div class="diagram-view">
      {#if renderError}
        <div class="render-error">
          <span class="material-symbols-outlined">error</span>
          <p>Diagram error: {renderError}</p>
        </div>
      {/if}
      <div
        class="diagram-canvas"
        role="button"
        tabindex="0"
        aria-label="Mermaid diagram. Scroll to zoom, drag to pan."
        onwheel={handleWheel}
        onmousedown={handleMouseDown}
        onmousemove={handleMouseMove}
        onmouseup={handleMouseUp}
        onmouseleave={handleMouseUp}
        onclick={handleCanvasClick}
        onkeydown={handleKeydown}
        style="cursor: {isDragging ? 'grabbing' : 'grab'};"
      >
        <div
          class="diagram-container"
          style="transform: translate(calc(-50% + {translateX}px), calc(-50% + {translateY}px)) scale({scale}); transform-origin: center center;"
          bind:this={renderContainer}
        ></div>
      </div>
    </div>
  {/if}
</WidgetContainer>

<style>
  :global(.confirm-action-btn) {
    color: var(--color-success) !important;
  }

  :global(.confirm-action-btn:hover) {
    background: color-mix(in srgb, var(--color-success) 15%, transparent) !important;
    color: var(--color-success) !important;
  }

  .loading {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
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

  :global([data-slot="textarea"].diagram-editor) {
    flex: 1;
    width: 100%;
    height: 100%;
    background: transparent;
    border: none;
    box-shadow: none;
    outline: none;
    resize: none;
    padding: 16px;
    font-family: var(--font-mono, monospace);
    font-size: 14px;
    line-height: 1.6;
    color: var(--color-neutral-active);
    box-sizing: border-box;
    min-height: unset;
    field-sizing: unset;
  }

  .diagram-view {
    flex: 1;
    overflow: hidden;
    position: relative;
  }

  .diagram-canvas {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: none;
    border: none;
    padding: 0;
  }

  .diagram-container {
    position: absolute;
    top: 50%;
    left: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .diagram-container :global(svg) {
    max-width: none !important;
    height: auto;
    display: block;
  }

  .diagram-container :global(g.node),
  .diagram-container :global(g[data-node="true"]) {
    cursor: pointer;
  }

  .diagram-container :global(g.node:hover rect),
  .diagram-container :global(g.node:hover circle),
  .diagram-container :global(g.node:hover polygon),
  .diagram-container :global(g[data-node="true"]:hover rect),
  .diagram-container :global(g[data-node="true"]:hover circle),
  .diagram-container :global(g[data-node="true"]:hover polygon) {
    filter: brightness(1.3);
  }

  .render-error {
    position: absolute;
    top: 12px;
    left: 12px;
    right: 12px;
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 12px;
    background: color-mix(in srgb, var(--color-error) 10%, transparent);
    color: var(--color-error);
    font-size: 12px;
    z-index: 1;
  }

  .render-error .material-symbols-outlined {
    font-size: 16px;
    flex-shrink: 0;
    margin-top: 1px;
  }

  .render-error p {
    margin: 0;
    word-break: break-word;
  }
</style>
