<script lang="ts">
  import { Button } from '$lib/components/ui/button';
  import { isIconUrl, resolveIconSrc } from '$lib/utils/icon';

  // Icons whose image failed to load fall back to the default ligature
  // instead of leaving an empty slot in the row.
  let brokenIcons = $state(new Set<string>());

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
</script>

<nav class="widget-sidebar" aria-label="Widgets abertos">
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
    width: 270px;
    max-height: calc(100% - 16px);
    box-shadow: 0 16px 40px color-mix(in srgb, var(--color-black) 86%, transparent);
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
    max-height: min(60vh, 480px);
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
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
    text-align: left;
  }
</style>
