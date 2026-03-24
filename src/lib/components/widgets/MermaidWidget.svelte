<script>
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import mermaid from 'mermaid';

  let { widgetId, entityId = '', windowState = 'normal', onWindowStateChange } = $props();

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
  let expanded = $state(windowState === 'maximized');
  let modalRenderContainer = $state(null);
  let scale = $state(1);
  let translateX = $state(0);
  let translateY = $state(0);
  let isDragging = $state(false);
  let dragStart = { x: 0, y: 0 };
  let dragMoved = false;
  let unlistenContentUpdated = null;
  let unlistenEntityUpdated = null;
  let renderCount = 0;

  function portal(node) {
    const target = document.querySelector('.canvas-area') ?? document.body;
    target.appendChild(node);
    return {
      destroy() {
        node.remove();
      }
    };
  }

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

    // Fix 2: Move SVG <style> to the HTML container.
    // WebKit does not apply CSS from <style> elements embedded inside SVG when the SVG
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

  async function renderModalDiagram() {
    if (!modalRenderContainer) return;
    try {
      const id = `mermaid-modal-${widgetId.replace(/[^a-zA-Z0-9]/g, '_')}_${++renderCount}`;
      const { svg } = await mermaid.render(id, content);
      modalRenderContainer.innerHTML = svg;
      fixSvgForWebkit(modalRenderContainer);
    } catch {
      modalRenderContainer.innerHTML = '';
    }
  }

  function fitToView() {
    if (!modalRenderContainer) return;
    const svg = modalRenderContainer.querySelector('svg');
    if (!svg) return;
    const canvas = modalRenderContainer.closest('.modal-canvas');
    if (!canvas) return;
    const svgRect = svg.getBoundingClientRect();
    const canvasRect = canvas.getBoundingClientRect();
    if (svgRect.width === 0 || svgRect.height === 0) return;
    const scaleX = (canvasRect.width * 0.88) / svgRect.width;
    const scaleY = (canvasRect.height * 0.88) / svgRect.height;
    scale = Math.min(scaleX, scaleY);
    translateX = 0;
    translateY = 0;
  }

  function openExpanded() {
    scale = 1;
    translateX = 0;
    translateY = 0;
    expanded = true;
    onWindowStateChange?.('maximized');
  }

  function handleWheel(e) {
    e.preventDefault();
    if (e.ctrlKey) {
      // Pinch-to-zoom (Mac trackpad) or Ctrl+scroll
      const factor = e.deltaY > 0 ? 0.97 : 1.03;
      scale = Math.max(0.1, Math.min(50, scale * factor));
    } else {
      // Two-finger pan (Mac trackpad) or mouse wheel scroll
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

  function resetView() {
    fitToView();
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
      conversationId: null,
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

  $effect(() => {
    if (expanded && modalRenderContainer) {
      renderModalDiagram().then(() => fitToView());
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
      if (event.payload === widgetId) {
        renderDiagram();
        if (expanded) renderModalDiagram();
      }
    });

    if (entityId) {
      unlistenEntityUpdated = await listen('entity-updated', async (event) => {
        if (event.payload.entityId !== entityId) return;
        try {
          const resultStr = await invoke('inspector__get_entity', { entityId });
          const data = JSON.parse(resultStr);
          const source = data?.properties?.find(p => p.property === 'foundation:diagramSource')?.value;
          if (source && source !== content) {
            content = source;
            draftContent = source;
            await renderDiagram();
            if (expanded) await renderModalDiagram();
          }
        } catch {
          // ignore
        }
      });
    }
  });

  onDestroy(() => {
    if (unlistenContentUpdated) unlistenContentUpdated();
    if (unlistenEntityUpdated) unlistenEntityUpdated();
  });
</script>

<svelte:window onkeydown={(e) => { if (e.key === 'Escape' && expanded) { expanded = false; onWindowStateChange?.('normal'); } }} />

<div class="mermaid-widget">
  <div class="widget-header">
    <div class="header-left">
      <span class="material-symbols-outlined header-icon">account_tree</span>
      <span class="header-title">{entityLabel || 'Mermaid Diagram'}</span>
    </div>
    <div class="header-actions">
      {#if editMode}
        <button class="action-btn confirm-btn" onclick={saveContent} title="Save">
          <span class="material-symbols-outlined">check</span>
        </button>
        <button class="action-btn" onclick={cancelEdit} title="Cancel">
          <span class="material-symbols-outlined">close</span>
        </button>
      {:else}
        <button class="action-btn" onclick={openExpanded} title="Expand">
          <span class="material-symbols-outlined">open_in_full</span>
        </button>
        <button class="action-btn" onclick={() => { draftContent = content; editMode = true; }} title="Edit diagram">
          <span class="material-symbols-outlined">edit</span>
        </button>
        <button class="action-btn" onclick={openInspector} title="Open inspector">
          <span class="material-symbols-outlined">info</span>
        </button>
        <button class="action-btn" onclick={() => onWindowStateChange?.(windowState === 'minimized' ? 'normal' : 'minimized')} title={windowState === 'minimized' ? 'Expand' : 'Minimize'}>
          <span class="material-symbols-outlined">{windowState === 'minimized' ? 'expand_more' : 'expand_less'}</span>
        </button>
        <button class="close-btn" onclick={closeWidget}>
          <span class="material-symbols-outlined">close</span>
        </button>
      {/if}
    </div>
  </div>

  <div class="widget-content">
    {#if entityLoading}
      <div class="loading">
        <span class="material-symbols-outlined spinning">progress_activity</span>
      </div>
    {:else if editMode}
      <textarea
        class="diagram-editor"
        bind:value={draftContent}
        spellcheck="false"
        placeholder="Enter Mermaid diagram source..."
      ></textarea>
    {:else}
      <div class="diagram-view">
        {#if renderError}
          <div class="render-error">
            <span class="material-symbols-outlined">error</span>
            <p>Diagram error: {renderError}</p>
          </div>
        {/if}
        <div bind:this={renderContainer} class="diagram-container"></div>
      </div>
    {/if}
  </div>
</div>

{#if expanded}
  <div
    use:portal
    class="modal-overlay"
    role="button"
    tabindex="-1"
    onclick={() => { expanded = false; onWindowStateChange?.('normal'); }}
    onkeydown={(e) => { if (e.key === 'Escape') { expanded = false; onWindowStateChange?.('normal'); } }}
  >
    <div
      class="modal-panel"
      role="dialog"
      aria-modal="true"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <div class="modal-header">
        <div class="header-left">
          <span class="material-symbols-outlined header-icon">account_tree</span>
          <span class="header-title">{entityLabel || 'Mermaid Diagram'}</span>
        </div>
        <div class="header-actions">
          <span class="zoom-label">{Math.round(scale * 100)}%</span>
          <button class="action-btn" onclick={resetView} title="Reset view">
            <span class="material-symbols-outlined">center_focus_strong</span>
          </button>
          <button class="action-btn" onclick={() => { expanded = false; onWindowStateChange?.('normal'); }} title="Close">
            <span class="material-symbols-outlined">close_fullscreen</span>
          </button>
        </div>
      </div>
      <div
        class="modal-canvas"
        onwheel={handleWheel}
        onmousedown={handleMouseDown}
        onmousemove={handleMouseMove}
        onmouseup={handleMouseUp}
        onmouseleave={handleMouseUp}
        onclick={handleCanvasClick}
        onkeydown={(e) => {
          if (e.key === 'ArrowLeft') translateX += 20;
          else if (e.key === 'ArrowRight') translateX -= 20;
          else if (e.key === 'ArrowUp') translateY += 20;
          else if (e.key === 'ArrowDown') translateY -= 20;
          else if (e.key === '+' || e.key === '=') scale = Math.min(50, scale * 1.1);
          else if (e.key === '-') scale = Math.max(0.1, scale * 0.9);
          else if (e.key === '0') resetView();
        }}
        role="application"
        tabindex="-1"
        aria-label="Mermaid diagram"
        style="cursor: {isDragging ? 'grabbing' : 'grab'};"
      >
        <div
          class="modal-diagram"
          style="transform: translate(calc(-50% + {translateX}px), calc(-50% + {translateY}px)) scale({scale}); transform-origin: center center;"
          bind:this={modalRenderContainer}
        ></div>
      </div>
      <div class="modal-hint">Scroll to zoom · Drag to pan · Press Esc to close</div>
    </div>
  </div>
{/if}

<style>
  .mermaid-widget {
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

  .widget-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    border-bottom: 1px solid color-mix(in srgb, var(--color-white) 15%, transparent);
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

  .confirm-btn {
    color: var(--color-success, #22c55e);
  }

  .confirm-btn:hover {
    background: color-mix(in srgb, var(--color-success, #22c55e) 15%, transparent);
    color: var(--color-success, #22c55e);
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

  .widget-content {
    flex: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
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

  .diagram-editor {
    flex: 1;
    width: 100%;
    height: 100%;
    background: transparent;
    border: none;
    outline: none;
    resize: none;
    padding: 16px;
    font-family: var(--font-mono, monospace);
    font-size: 13px;
    line-height: 1.6;
    color: var(--color-neutral-active);
    box-sizing: border-box;
  }

  .diagram-view {
    flex: 1;
    overflow: auto;
    padding: 16px;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
  }

  .diagram-container {
    width: 100%;
    display: flex;
    justify-content: center;
  }

  .diagram-container :global(svg) {
    max-width: 100%;
    height: auto;
  }

  .render-error {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 12px;
    margin-bottom: 12px;
    background: color-mix(in srgb, var(--color-error, #ef4444) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-error, #ef4444) 30%, transparent);
    border-radius: 6px;
    color: var(--color-error, #ef4444);
    font-size: 12px;
    width: 100%;
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

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    border-bottom: 1px solid color-mix(in srgb, var(--color-white) 15%, transparent);
    flex-shrink: 0;
  }

  .zoom-label {
    font-size: 11px;
    color: color-mix(in srgb, var(--color-white) 50%, transparent);
    font-family: var(--font-mono, monospace);
    min-width: 38px;
    text-align: right;
  }

  .modal-canvas {
    flex: 1;
    overflow: hidden;
    position: relative;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .modal-diagram {
    position: absolute;
    top: 50%;
    left: 50%;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .modal-diagram :global(svg) {
    max-width: none !important;
    height: auto;
    display: block;
  }

  .modal-diagram :global(g.node),
  .modal-diagram :global(g[data-node="true"]) {
    cursor: pointer;
  }

  .modal-diagram :global(g.node:hover rect),
  .modal-diagram :global(g.node:hover circle),
  .modal-diagram :global(g.node:hover polygon),
  .modal-diagram :global(g[data-node="true"]:hover rect),
  .modal-diagram :global(g[data-node="true"]:hover circle),
  .modal-diagram :global(g[data-node="true"]:hover polygon) {
    filter: brightness(1.3);
  }

  .modal-hint {
    padding: 8px 16px;
    font-size: 11px;
    color: color-mix(in srgb, var(--color-white) 35%, transparent);
    text-align: center;
    border-top: 1px solid color-mix(in srgb, var(--color-white) 10%, transparent);
    flex-shrink: 0;
  }
</style>
