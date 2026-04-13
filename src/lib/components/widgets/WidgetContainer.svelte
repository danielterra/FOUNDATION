<script>
  import { convertFileSrc } from '@tauri-apps/api/core';

  let {
    icon = 'widgets',
    iconSrc = null,
    title = '',
    windowState = 'normal',
    onWindowStateChange,
    onClose,
    canExpand = false,
    headerActions,
    headerSubtitle,
    children,
  } = $props();

  function toggleMinimize() {
    onWindowStateChange?.(windowState === 'minimized' ? 'normal' : 'minimized');
  }

  function toggleExpanded() {
    onWindowStateChange?.(windowState === 'maximized' ? 'normal' : 'maximized');
  }

  function resolveIconSrc(src) {
    if (!src) return '';
    if (src.startsWith('http://') || src.startsWith('https://') || src.startsWith('data:')) return src;
    if (src.startsWith('file://')) return convertFileSrc(src.replace(/^file:\/\//, ''));
    if (src.startsWith('/')) return convertFileSrc(src);
    return src;
  }
</script>

<div class="widget" class:minimized={windowState === 'minimized'}>
  <div class="widget-header">
    <div class="header-top">
      <div class="header-left">
        <div class="header-icon-wrap">
          {#if iconSrc}
            <img src={resolveIconSrc(iconSrc)} alt="" class="header-icon-img" />
          {:else}
            <span class="material-symbols-outlined header-icon">{icon}</span>
          {/if}
        </div>
        <div class="header-title-block">
          <span class="header-title">{title}</span>
          {#if headerSubtitle}
            <div class="header-subtitle">{@render headerSubtitle()}</div>
          {/if}
        </div>
      </div>
      <div class="header-controls">
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
      </div>
    </div>
    {#if headerActions}
      <div class="header-actions">
        {@render headerActions()}
      </div>
    {/if}
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
    background: rgba(0, 0, 0, 0.5);
    overflow: hidden;
    box-shadow: 0 8px 32px color-mix(in srgb, var(--color-black) 40%, transparent);
    backdrop-filter: blur(5px);
  }

  .widget-header {
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
  }

  .header-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 16px;
    gap: 8px;
  }

  .header-left {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    overflow: hidden;
    flex: 1;
  }

  .header-controls {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }

  .header-icon-wrap {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .header-icon {
    font-size: 34px;
    color: var(--color-interactive);
  }

  .header-icon-img {
    width: 34px;
    height: 34px;
    object-fit: cover;
  }

  .header-title-block {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    overflow: hidden;
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

  .header-subtitle {
    display: flex;
    align-items: center;
  }

  .header-actions {
    background-color: rgba(255, 140, 66, 0.09);
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 4px;
    padding: 0px;
  }

  .action-btn {
    background: none;
    border: none;
    padding: 4px;
    cursor: pointer;
    color: var(--color-interactive);
    display: flex;
    align-items: center;
    justify-content: center;
    transition: color 0.15s, background 0.15s;
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
    display: flex;
    align-items: center;
    justify-content: center;
    transition: color 0.15s, background 0.15s;
  }

  .close-btn:hover {
    background: color-mix(in srgb, var(--color-interactive) 15%, transparent);
    color: var(--color-neutral-active);
  }

  .close-btn :global(.material-symbols-outlined) {
    font-size: 20px;
  }

  /* Styles for action buttons provided via headerActions snippet */
  .header-actions :global(.action-btn) {
    background: none;
    border: none;
    padding: 4px;
    cursor: pointer;
    color: var(--color-interactive);
    display: flex;
    align-items: center;
    justify-content: center;
    transition: color 0.15s, background 0.15s;
  }

  .header-actions :global(.action-btn:hover) {
    background: color-mix(in srgb, var(--color-interactive) 15%, transparent);
    color: var(--color-neutral-active);
  }

  .header-actions :global(.action-btn .material-symbols-outlined) {
    font-size: 18px;
  }

  .header-actions :global(.action-btn--locked) {
    color: var(--color-warning, #f59e0b);
  }

  .header-actions :global(.action-btn--locked:hover) {
    background: color-mix(in srgb, var(--color-warning, #f59e0b) 15%, transparent);
    color: var(--color-warning, #f59e0b);
  }

  .header-actions :global(.action-btn--danger) {
    color: var(--color-danger, #ef4444);
  }

  .header-actions :global(.action-btn--danger:hover) {
    background: color-mix(in srgb, var(--color-danger, #ef4444) 15%, transparent);
    color: var(--color-danger, #ef4444);
  }

  .content-wrapper {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-rows: 1fr;
    transition: grid-template-rows 250ms cubic-bezier(0.4, 0, 0.2, 1);
    overflow: hidden;
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
