<script>
  import { convertFileSrc } from '@tauri-apps/api/core';
  import Button from '../Button.svelte';

  let {
    icon = 'widgets',
    iconSrc = null,
    title = '',
    windowState = 'normal',
    onWindowStateChange,
    onClose,
    canExpand = true,
    headerActions,
    headerExtra,
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
      {#if headerExtra}
        <div class="header-extra">
          {@render headerExtra()}
        </div>
      {/if}
      <div class="header-controls">
        <Button
          icon={windowState === 'minimized' ? 'filter_none' : 'horizontal_rule'}
          title={windowState === 'minimized' ? 'Restore' : 'Minimize'}
          onclick={toggleMinimize}
        />
        {#if canExpand}
          <Button
            icon={windowState === 'maximized' ? 'filter_none' : 'check_box_outline_blank'}
            title={windowState === 'maximized' ? 'Restore' : 'Maximize'}
            onclick={toggleExpanded}
          />
        {/if}
        <Button icon="close" title="Close" onclick={onClose} />
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
    background: var(--color-surface-1);
    border-radius: var(--radius);
    overflow: hidden;
    box-shadow: 0 8px 32px color-mix(in srgb, var(--color-black) 40%, transparent);
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

  .header-extra {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
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
    color: var(--color-neutral-active);
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
    color: var(--color-neutral-active);
  }

  .header-actions {
    background-color: rgba(255, 140, 66, 0.09);
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 4px;
    padding: 0px;
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
