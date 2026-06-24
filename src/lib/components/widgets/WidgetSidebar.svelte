<script lang="ts">
  import { Button } from '$lib/components/ui/button';

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
        size="icon-sm"
        disabled={!hasOpenWidgets}
        onclick={() => onMinimizeAll?.()}
        title="Minimizar todos os widgets"
        aria-label="Minimizar todos os widgets"
      ><span class="material-symbols-outlined">vertical_align_bottom</span></Button>
      <Button
        variant="ghost"
        size="icon-sm"
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
        {#if entry.icon.startsWith('http://') || entry.icon.startsWith('https://') || entry.icon.startsWith('data:')}
          <img src={entry.icon} alt="" class="entry-icon-img" />
        {:else}
          <span class="material-symbols-outlined entry-icon">{entry.icon}</span>
        {/if}
        <span class="entry-title">{entry.title}</span>
      </Button>
    {/each}
  </div>
</nav>

<style>
  .widget-sidebar {
    display: flex;
    flex-direction: column;
    background: var(--card);
    border-radius: var(--radius);
    overflow: hidden;
    flex-shrink: 0;
    width: 270px;
  }

  .sidebar-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    padding: 4px 6px 4px 10px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  .sidebar-header-label {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    color: var(--muted-foreground);
    text-transform: uppercase;
  }

  .sidebar-header-actions {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .sidebar-list {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 6px 4px;
    overflow-y: auto;
    overflow-x: hidden;
    flex: 1;
    min-height: 0;
  }

  :global([data-slot="button"].sidebar-entry) {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-radius: var(--radius);
    background: transparent;
    color: var(--muted-foreground);
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
    background: color-mix(in srgb, var(--primary) 20%, transparent);
    color: var(--primary);
  }

  :global([data-slot="button"].sidebar-entry.is-focused:hover) {
    background: color-mix(in srgb, var(--primary) 30%, transparent);
    color: var(--primary);
  }

  :global([data-slot="button"].sidebar-entry.is-minimized) {
    opacity: 0.5;
    font-style: italic;
  }

  :global([data-slot="button"].sidebar-entry.is-minimized:hover) {
    opacity: 0.8;
  }

  .entry-icon {
    font-size: 18px;
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
    font-size: 12px;
    line-height: 1.2;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
    text-align: left;
  }
</style>
