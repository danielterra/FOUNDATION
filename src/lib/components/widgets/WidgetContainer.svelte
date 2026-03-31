<script>
  let {
    icon = 'widgets',
    title = '',
    windowState = 'normal',
    onWindowStateChange,
    onClose,
    canExpand = false,
    headerExtra,
    headerActions,
    overrideActions,
    children,
  } = $props();

  function toggleMinimize() {
    onWindowStateChange?.(windowState === 'minimized' ? 'normal' : 'minimized');
  }

  function toggleExpanded() {
    onWindowStateChange?.(windowState === 'maximized' ? 'normal' : 'maximized');
  }
</script>

<div class="widget" class:minimized={windowState === 'minimized'}>
  <div class="widget-header">
    <div class="header-left">
      <span class="material-symbols-outlined header-icon">{icon}</span>
      <span class="header-title">{title}</span>
      {@render headerExtra?.()}
    </div>
    <div class="header-actions">
      {#if overrideActions}
        {@render overrideActions()}
      {:else}
        {@render headerActions?.()}
        {#if canExpand}
          <button class="action-btn" onclick={toggleExpanded} title={windowState === 'maximized' ? 'Restore' : 'Expand'}>
            <span class="material-symbols-outlined">{windowState === 'maximized' ? 'close_fullscreen' : 'open_in_full'}</span>
          </button>
        {/if}
        <button class="action-btn" onclick={toggleMinimize} title={windowState === 'minimized' ? 'Expand' : 'Minimize'}>
          <span class="material-symbols-outlined">{windowState === 'minimized' ? 'expand_more' : 'expand_less'}</span>
        </button>
        <button class="close-btn" onclick={onClose}>
          <span class="material-symbols-outlined">close</span>
        </button>
      {/if}
    </div>
  </div>
  <div class="content-wrapper">
    <div class="widget-content">
      {@render children?.()}
    </div>
  </div>
</div>

<style>
  .widget {
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
    flex-shrink: 0;
  }

  .widget.minimized .widget-header {
    border-bottom: none;
  }

  .header-left {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    overflow: hidden;
  }

  .header-icon {
    font-size: 22px;
    color: var(--color-interactive);
    flex-shrink: 0;
  }

  .header-title {
    font-family: var(--font-title);
    font-size: 11px;
    font-weight: 600;
    color: var(--color-neutral-active);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
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

  .action-btn :global(.material-symbols-outlined) {
    font-size: 18px;
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

  .close-btn :global(.material-symbols-outlined) {
    font-size: 20px;
  }

  /* Applies to snippet-provided buttons inside the header actions area */
  .header-actions :global(.action-btn) {
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

  .header-actions :global(.action-btn:hover) {
    background: color-mix(in srgb, var(--color-interactive) 15%, transparent);
    color: var(--color-neutral-active);
  }

  .header-actions :global(.action-btn .material-symbols-outlined) {
    font-size: 18px;
  }

  .content-wrapper {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-rows: 1fr;
    transition: grid-template-rows 250ms cubic-bezier(0.4, 0, 0.2, 1);
    overflow: hidden;
  }

  .widget.minimized .content-wrapper {
    grid-template-rows: 0fr;
  }

  .content-wrapper > .widget-content {
    min-height: 0;
    overflow: hidden;
  }

  .widget-content {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
</style>
