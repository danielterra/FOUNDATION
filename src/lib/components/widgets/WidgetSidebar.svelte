<script lang="ts">
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
      <button
        type="button"
        class="header-action-btn"
        disabled={!hasOpenWidgets}
        onclick={() => onMinimizeAll?.()}
        title="Minimizar todos os widgets"
        aria-label="Minimizar todos os widgets"
      >
        <span class="material-symbols-outlined">vertical_align_bottom</span>
      </button>
      <button
        type="button"
        class="header-action-btn"
        disabled={!hasMinimizedWidgets}
        onclick={() => onRestoreAll?.()}
        title="Restaurar todos os widgets"
        aria-label="Restaurar todos os widgets"
      >
        <span class="material-symbols-outlined">vertical_align_top</span>
      </button>
    </div>
  </div>
  <div class="sidebar-list">
    {#each entries as entry (entry.id)}
      <button
        class="sidebar-entry"
        class:is-focused={entry.isFocused}
        class:is-minimized={entry.isMinimized}
        title={entry.isMinimized ? `(minimizado) ${entry.title}` : entry.title}
        onclick={() => onSelect(entry.id)}
      >
        {#if entry.icon.startsWith('http://') || entry.icon.startsWith('https://') || entry.icon.startsWith('data:')}
          <img src={entry.icon} alt="" class="entry-icon-img" />
        {:else}
          <span class="material-symbols-outlined entry-icon">{entry.icon}</span>
        {/if}
        <span class="entry-title">{entry.title}</span>
      </button>
    {/each}
  </div>
</nav>

<style>
  .widget-sidebar {
    display: flex;
    flex-direction: column;
    background: color-mix(in srgb, var(--color-white) 7%, var(--color-black));
    border-radius: var(--radius);
    overflow: hidden;
    flex-shrink: 0;
    width: 180px;
  }

  .sidebar-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
    padding: 4px 6px 4px 10px;
    border-bottom: 1px solid color-mix(in srgb, var(--color-white) 10%, transparent);
    flex-shrink: 0;
  }

  .sidebar-header-label {
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    color: color-mix(in srgb, var(--color-white) 65%, transparent);
    text-transform: uppercase;
  }

  .sidebar-header-actions {
    display: flex;
    align-items: center;
    gap: 2px;
  }

  .header-action-btn {
    width: 28px;
    height: 28px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    border: none;
    border-radius: var(--radius);
    color: color-mix(in srgb, var(--color-white) 70%, transparent);
    cursor: pointer;
    transition: background 0.15s, color 0.15s, opacity 0.15s;
    padding: 0;
  }

  .header-action-btn:hover:not(:disabled) {
    background: color-mix(in srgb, var(--color-white) 10%, transparent);
    color: var(--color-white);
  }

  .header-action-btn:disabled {
    opacity: 0.3;
    cursor: not-allowed;
  }

  .header-action-btn .material-symbols-outlined {
    font-size: 18px;
    line-height: 1;
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

  .sidebar-entry {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border: none;
    border-radius: var(--radius);
    background: transparent;
    color: color-mix(in srgb, var(--color-white) 55%, transparent);
    cursor: pointer;
    min-width: 0;
    width: 100%;
    text-align: left;
    transition: background 0.15s, color 0.15s;
  }

  .sidebar-entry:hover {
    background: color-mix(in srgb, var(--color-white) 8%, transparent);
    color: var(--color-white);
  }

  .sidebar-entry.is-focused {
    background: color-mix(in srgb, var(--color-interactive) 20%, transparent);
    color: var(--color-interactive);
  }

  .sidebar-entry.is-focused:hover {
    background: color-mix(in srgb, var(--color-interactive) 30%, transparent);
  }

  .sidebar-entry.is-minimized {
    opacity: 0.5;
    font-style: italic;
  }

  .sidebar-entry.is-minimized:hover {
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
  }
</style>
