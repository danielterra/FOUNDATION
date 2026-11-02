<script lang="ts">
  import type { Snippet } from 'svelte'
  import { convertFileSrc, invoke } from '@tauri-apps/api/core';
  import { Button } from '$lib/components/ui/button';

  type CopyState = 'nominal' | 'copied' | 'error';

  let {
    icon = 'widgets',
    iconSrc = null,
    title = '',
    showTitle = true,
    windowState = 'normal',
    onWindowStateChange,
    onClose,
    canExpand = true,
    entityId,
    canOpenInspector = true,
    canCopyId = true,
    conversationIri = null,
    headerActions,
    headerExtra,
    headerSubtitle,
    headerTitle,
    children,
  }: {
    icon?: string
    iconSrc?: string | null
    title?: string
    showTitle?: boolean
    windowState?: string
    onWindowStateChange?: (state: string) => void
    onClose?: () => void
    canExpand?: boolean
    entityId?: string
    canOpenInspector?: boolean
    canCopyId?: boolean
    conversationIri?: string | null
    headerActions?: Snippet
    headerExtra?: Snippet
    headerSubtitle?: Snippet
    headerTitle?: Snippet
    children?: Snippet
  } = $props();

  let copyState = $state<CopyState>('nominal');
  let copyTimer: ReturnType<typeof setTimeout> | null = null;

  function openInspector() {
    if (!entityId) return;
    invoke('widget_blackboard__add_widget', {
      widgetType: 'inspector',
      entityId,
      position: null,
      size: null,
      conversationId: conversationIri ?? null,
    }).catch(() => {});
  }

  async function copyEntityId() {
    if (!entityId) return;
    if (copyTimer !== null) {
      clearTimeout(copyTimer);
      copyTimer = null;
    }
    try {
      await navigator.clipboard.writeText(entityId);
      copyState = 'copied';
    } catch {
      copyState = 'error';
    }
    copyTimer = setTimeout(() => {
      copyState = 'nominal';
      copyTimer = null;
    }, 1500);
  }

  function toggleMinimize() {
    onWindowStateChange?.(windowState === 'minimized' ? 'normal' : 'minimized');
  }

  function toggleExpanded() {
    onWindowStateChange?.(windowState === 'maximized' ? 'normal' : 'maximized');
  }

  function resolveIconSrc(src: string): string {
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
          {#if showTitle}
            {#if headerTitle}
              {@render headerTitle()}
            {:else}
              <span class="header-title">{title}</span>
            {/if}
          {/if}
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
        {#if entityId && canCopyId}
          <Button variant="ghost" size="icon-touch"
            aria-label={copyState === 'copied' ? 'ID copiado!' : copyState === 'error' ? 'Não foi possível copiar o ID' : 'Copiar ID'}
            title={copyState === 'copied' ? 'ID copiado!' : copyState === 'error' ? 'Não foi possível copiar o ID' : 'Copiar ID'}
            class={copyState === 'copied' ? 'copy-id-btn copy-id-btn--success' : copyState === 'error' ? 'copy-id-btn copy-id-btn--error' : 'copy-id-btn'}
            onclick={copyEntityId}
          ><span class="material-symbols-outlined">{copyState === 'copied' ? 'done' : copyState === 'error' ? 'error' : 'content_copy'}</span></Button>
          <span class="sr-only" aria-live="polite">{copyState === 'copied' ? 'ID copiado' : copyState === 'error' ? 'Não foi possível copiar o ID' : ''}</span>
        {/if}
        {#if entityId && canOpenInspector}
          <Button variant="ghost" size="icon-touch"
            aria-label="Abrir no inspetor"
            title="Abrir no inspetor"
            onclick={openInspector}
          ><span class="material-symbols-outlined">info</span></Button>
        {/if}
        <Button variant="ghost" size="icon-touch"
          title={windowState === 'minimized' ? 'Restore' : 'Minimize'}
          onclick={toggleMinimize}
        ><span class="material-symbols-outlined">{windowState === 'minimized' ? 'filter_none' : 'horizontal_rule'}</span></Button>
        {#if canExpand}
          <Button variant="ghost" size="icon-touch"
            title={windowState === 'maximized' ? 'Restore' : 'Maximize'}
            onclick={toggleExpanded}
          ><span class="material-symbols-outlined">{windowState === 'maximized' ? 'filter_none' : 'check_box_outline_blank'}</span></Button>
        {/if}
        <Button variant="ghost" size="icon-touch" title="Close" onclick={onClose}><span class="material-symbols-outlined">close</span></Button>
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
    position: relative;
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    background: color-mix(
      in srgb,
      color-mix(
        in srgb,
        var(--color-widget-surface-front) calc(100% - var(--widget-background-darkness)),
        var(--color-widget-surface-back) var(--widget-background-darkness)
      ) var(--opacity-widget-surface),
      transparent
    );
    backdrop-filter: blur(var(--blur-widget-surface));
    border: 0;
    border-radius: 0;
    overflow: hidden;
    box-shadow: none;
  }

  .widget::before {
    content: '';
    position: absolute;
    inset: 0;
    z-index: 3;
    pointer-events: none;
    background:
      radial-gradient(circle, var(--widget-selection-accent, var(--color-hud-point)) 0 1.5px, transparent 2px) left top / var(--size-hud-marker) var(--size-hud-marker) no-repeat,
      radial-gradient(circle, var(--color-hud-point) 0 1.5px, transparent 2px) right top / var(--size-hud-marker) var(--size-hud-marker) no-repeat,
      radial-gradient(circle, var(--color-hud-point) 0 1.5px, transparent 2px) left bottom / var(--size-hud-marker) var(--size-hud-marker) no-repeat,
      radial-gradient(circle, var(--color-hud-point) 0 1.5px, transparent 2px) right bottom / var(--size-hud-marker) var(--size-hud-marker) no-repeat,
      linear-gradient(var(--color-hud-line-subtle) 0 0) top / 100% var(--size-hud-frame-line) no-repeat,
      linear-gradient(var(--color-hud-line-subtle) 0 0) bottom / 100% var(--size-hud-frame-line) no-repeat,
      linear-gradient(var(--color-hud-line-subtle) 0 0) left / var(--size-hud-frame-line) 100% no-repeat,
      linear-gradient(var(--color-hud-line-subtle) 0 0) right / var(--size-hud-frame-line) 100% no-repeat;
    filter: drop-shadow(0 0 var(--size-hud-glow-blur) var(--color-hud-glow));
  }

  .widget-header {
    display: flex;
    flex-direction: column;
    flex-shrink: 0;
    border-bottom: 1px solid var(--color-hud-line-subtle);
  }

  .header-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--spacing-widget-header-block) var(--spacing-widget-header-inline);
    gap: var(--spacing-inline-related);
  }

  .header-left {
    display: flex;
    align-items: center;
    gap: var(--spacing-control-internal);
    min-width: 0;
    overflow: hidden;
    flex: 1;
  }

  .header-extra {
    display: flex;
    align-items: center;
    gap: var(--spacing-inline-tight);
    flex-shrink: 0;
  }

  .header-controls {
    display: flex;
    align-items: center;
    gap: var(--spacing-inline-tight);
    flex-shrink: 0;
  }

  .header-controls :global(.material-symbols-outlined) {
    font-size: var(--type-icon-medium);
    line-height: 1;
  }

  :global([data-slot="button"].copy-id-btn--success .material-symbols-outlined) {
    color: var(--color-success);
  }

  :global([data-slot="button"].copy-id-btn--error .material-symbols-outlined) {
    color: var(--color-error);
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

  .header-icon-wrap {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .header-icon {
    font-size: var(--type-display);
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
    gap: var(--spacing-inline-tight);
    min-width: 0;
    overflow: hidden;
  }

  .header-title {
    font-family: var(--font-title);
    font-size: var(--type-body);
    color: var(--text-primary);
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
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--spacing-inline-tight);
    padding: var(--spacing-inline-tight) 0 var(--spacing-inline-tight) var(--spacing-inline-tight);
    margin: 0 var(--spacing-widget-header-inline) var(--spacing-control-internal);
    background: transparent;
    border: 1px solid color-mix(in srgb, var(--color-border) 55%, transparent);
    border-radius: var(--radius);
  }

  .header-actions :global(.material-symbols-outlined) {
    font-size: var(--type-icon-medium);
  }

  .header-actions :global(.action-btn--danger:hover) {
    background: color-mix(in srgb, var(--destructive) 15%, transparent);
    color: var(--destructive);
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
