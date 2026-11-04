<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { isIconUrl, resolveIconSrc } from '$lib/utils/icon';

  // Icons whose image failed to load fall back to the default ligature
  // instead of leaving an empty slot in the row.
  let brokenIcons = $state(new Set<string>());
  let sidebarElement: HTMLElement;
  let isExpanded = $state(false);
  let isPointerInside = $state(false);
  let isFocusInside = $state(false);
  let lastPointerType = '';
  let togglePointerType = '';

  interface SidebarEntry {
    id: string;
    icon: string;
    title: string;
    isFocused: boolean;
    isMinimized: boolean;
  }

  let {
    entries,
    onSelect,
    onMinimizeAll,
    onRestoreAll,
    hasOpenWidgets = false,
    hasMinimizedWidgets = false
  }: {
    entries: SidebarEntry[];
    onSelect: (id: string) => void;
    onMinimizeAll?: () => void;
    onRestoreAll?: () => void;
    hasOpenWidgets?: boolean;
    hasMinimizedWidgets?: boolean;
  } = $props();

  function handlePointerDown(event: PointerEvent) {
    lastPointerType = event.pointerType;
    isFocusInside = false;
  }

  function handleTogglePointerDown(event: PointerEvent) {
    togglePointerType = event.pointerType;
  }

  function handlePointerUp() {
    lastPointerType = '';
  }

  function handlePointerEnter(event: PointerEvent) {
    if (event.pointerType === 'touch') return;
    isPointerInside = true;
    isExpanded = true;
  }

  function handlePointerLeave(event: PointerEvent) {
    if (event.pointerType === 'touch') return;
    isPointerInside = false;
    if (!isFocusInside) isExpanded = false;
  }

  function handleFocusIn(event: FocusEvent) {
    if (lastPointerType === 'mouse' || lastPointerType === 'touch') {
      isFocusInside = false;
      return;
    }
    isFocusInside = true;
    const target = event.target;
    if (target instanceof Element && target.closest('.sidebar-toggle')) return;
    isExpanded = true;
  }

  function handleFocusOut(event: FocusEvent) {
    const nextTarget = event.relatedTarget;
    if (nextTarget instanceof Node && sidebarElement.contains(nextTarget)) return;
    isFocusInside = false;
    if (!isPointerInside) isExpanded = false;
  }

  function handleToggleClick(event: MouseEvent) {
    if (togglePointerType === 'touch' || event.detail === 0) {
      isExpanded = !isExpanded;
    }
    togglePointerType = '';
  }
</script>

<nav
  bind:this={sidebarElement}
  class="widget-sidebar"
  class:is-expanded={isExpanded}
  aria-label="Widgets abertos"
  onpointerdown={handlePointerDown}
  onpointerup={handlePointerUp}
  onpointerenter={handlePointerEnter}
  onpointerleave={handlePointerLeave}
  onfocusin={handleFocusIn}
  onfocusout={handleFocusOut}
>
  <Button
    variant="ghost"
    size="icon-touch"
    class="sidebar-toggle"
    onpointerdown={handleTogglePointerDown}
    onclick={handleToggleClick}
    title={`Widgets, ${entries.length} itens`}
    aria-label={`Widgets, ${entries.length} itens`}
    aria-expanded={isExpanded}
    aria-controls="widget-sidebar-content"
  >
    <span class="material-symbols-outlined">widgets</span>
    <span class="sidebar-count">{entries.length}</span>
  </Button>
  <div
    id="widget-sidebar-content"
    class="sidebar-content"
    aria-hidden={!isExpanded}
    inert={!isExpanded}
  >
    <div class="sidebar-header">
      <span class="sidebar-header-label">Widgets</span>
      <div class="sidebar-header-actions">
        <Button
          variant="ghost"
          size="icon-touch"
          disabled={!hasOpenWidgets}
          onclick={() => onMinimizeAll?.()}
          title="Minimizar todos os widgets"
          aria-label="Minimizar todos os widgets"
        ><span class="material-symbols-outlined">vertical_align_bottom</span></Button>
        <Button
          variant="ghost"
          size="icon-touch"
          disabled={!hasMinimizedWidgets}
          onclick={() => onRestoreAll?.()}
          title="Restaurar todos os widgets"
          aria-label="Restaurar todos os widgets"
        ><span class="material-symbols-outlined">vertical_align_top</span></Button>
      </div>
    </div>
    <div class="sidebar-list">
      {#each entries as entry (entry.id)}
        <Button
          variant="ghost"
          class={`sidebar-entry${entry.isFocused ? ' is-focused' : ''}${entry.isMinimized ? ' is-minimized' : ''}`}
          title={entry.isMinimized ? `(minimizado) ${entry.title}` : entry.title}
          onclick={() => onSelect(entry.id)}
        >
          {#if isIconUrl(entry.icon) && !brokenIcons.has(entry.id)}
            <img
              src={resolveIconSrc(entry.icon)}
              alt=""
              class="entry-icon-img"
              onerror={() => { brokenIcons = new Set(brokenIcons).add(entry.id); }}
            />
          {:else}
            <span class="material-symbols-outlined entry-icon">{isIconUrl(entry.icon) ? 'widgets' : entry.icon}</span>
          {/if}
          <span class="entry-title">{entry.title}</span>
        </Button>
      {/each}
    </div>
  </div>
</nav>

<style>
  .widget-sidebar {
    position: absolute;
    bottom: 8px;
    left: 8px;
    z-index: calc(var(--z-widget-max) + 1);
    display: flex;
    flex-direction: column;
    background: color-mix(in srgb, var(--color-surface-1) var(--opacity-widget-surface), transparent);
    backdrop-filter: blur(var(--blur-widget-surface));
    border: 0;
    border-radius: 0;
    overflow: hidden;
    width: var(--size-widget-sidebar-width);
    max-width: var(--size-widget-sidebar-width);
    max-height: calc(100% - 16px);
    box-shadow: 0 16px 40px color-mix(in srgb, var(--color-black) 86%, transparent);
    transition: width 220ms cubic-bezier(0.22, 1, 0.36, 1);
  }

  .widget-sidebar:not(.is-expanded) {
    width: var(--size-touch-target);
  }

  :global([data-slot="button"].sidebar-toggle) {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--spacing-inline-tight);
    flex-shrink: 0;
  }

  :global([data-slot="button"].sidebar-toggle .material-symbols-outlined) {
    font-size: var(--type-icon-medium);
    line-height: 1;
  }

  .sidebar-count {
    color: var(--color-interactive);
    font-size: var(--type-caption-compact);
    line-height: 1;
    text-align: center;
  }

  .sidebar-content {
    display: flex;
    flex-direction: column;
    min-height: 0;
    max-height: 0;
    opacity: 0;
    overflow: hidden;
    visibility: hidden;
    order: 0;
    transition: max-height 220ms cubic-bezier(0.22, 1, 0.36, 1), opacity 140ms ease, visibility 220ms;
  }

  .widget-sidebar.is-expanded .sidebar-content {
    max-height: calc(60vh + var(--size-touch-target));
    opacity: 1;
    visibility: visible;
    transition: max-height 220ms cubic-bezier(0.22, 1, 0.36, 1), opacity 140ms ease, visibility 0ms;
  }

  .sidebar-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--spacing-inline-related);
    padding: var(--spacing-inline-tight) var(--spacing-control-internal);
    border-bottom: 1px solid var(--color-hud-line-subtle);
    flex-shrink: 0;
  }

  .sidebar-header-label {
    font-size: var(--type-label);
    letter-spacing: var(--tracking-hud-label);
    color: var(--text-secondary);
    text-transform: uppercase;
  }

  .sidebar-header-actions {
    display: flex;
    align-items: center;
    gap: var(--spacing-inline-tight);
  }

  .sidebar-header-actions :global(.material-symbols-outlined) {
    font-size: var(--type-icon-medium);
    line-height: 1;
  }

  .sidebar-list {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-inline-tight);
    padding: var(--spacing-control-internal) var(--spacing-inline-tight);
    overflow-y: auto;
    overflow-x: hidden;
    flex: 0 1 auto;
    min-height: 0;
    max-height: min(60vh, var(--size-widget-sidebar-list-max-height));
  }

  :global([data-slot="button"].sidebar-toggle) {
    order: 1;
  }

  :global([data-slot="button"].sidebar-entry) {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: var(--spacing-inline-related);
    padding: var(--spacing-control-internal);
    min-height: var(--size-touch-target);
    border-radius: var(--radius);
    background: transparent;
    color: var(--text-secondary);
    min-width: 0;
    width: 100%;
    height: auto;
    justify-content: flex-start;
    text-align: left;
    font-size: inherit;
    transition: background 0.15s, color 0.15s;
  }

  :global([data-slot="button"].sidebar-entry:hover) {
    background: var(--accent);
    color: var(--accent-foreground);
  }

  :global([data-slot="button"].sidebar-entry.is-focused) {
    background: var(--color-selection-subtle);
    color: var(--color-selection);
  }

  :global([data-slot="button"].sidebar-entry.is-focused:hover) {
    background: var(--color-selection-hover-subtle);
    color: var(--color-selection);
  }

  :global([data-slot="button"].sidebar-entry.is-minimized) {
    opacity: 0.5;
  }

  :global([data-slot="button"].sidebar-entry.is-minimized:hover) {
    opacity: 0.8;
  }

  .entry-icon {
    font-size: var(--type-icon-medium);
    line-height: 1;
    flex-shrink: 0;
  }

  .entry-icon-img {
    width: 18px;
    height: 18px;
    object-fit: contain;
    flex-shrink: 0;
  }

  .entry-title {
    flex: 1;
    font-size: var(--type-caption);
    line-height: 1.2;
    display: -webkit-box;
    -webkit-box-orient: vertical;
    line-clamp: 2;
    -webkit-line-clamp: 2;
    overflow: hidden;
    overflow-wrap: anywhere;
    white-space: normal;
    min-width: 0;
    text-align: left;
  }

  @media (prefers-reduced-motion: reduce) {
    .widget-sidebar,
    .sidebar-content {
      transition: none;
    }
  }
</style>
