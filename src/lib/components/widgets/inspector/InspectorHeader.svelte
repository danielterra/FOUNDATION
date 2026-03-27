<script>
  import { convertFileSrc } from '@tauri-apps/api/core';

  let {
    entityData,
    widgetDefinitions,
    windowState,
    isLocked,
    togglingLock,
    showStatusPicker = $bindable(),
    statusBadgeWrapperEl = $bindable(),
    onToggleMinimize,
    onClose,
    onDelete,
    onCopyIri,
    onToggleLock,
    onOpenEntityInspector,
    onOpenWidget,
    onUpdateStatus,
  } = $props();

  const WIDGET_TYPE_ICONS = {
    mermaid: 'account_tree',
  };

  function isIconUrl(icon) {
    if (!icon) return false;
    return icon.startsWith('http://') || icon.startsWith('https://') ||
           icon.startsWith('data:') || icon.startsWith('file://') || icon.startsWith('/');
  }

  function getIconUrl(icon) {
    if (!icon) return '';
    if (icon.startsWith('http://') || icon.startsWith('https://') || icon.startsWith('data:'))
      return icon;
    if (icon.startsWith('file://')) return convertFileSrc(icon.replace(/^file:\/\//, ''));
    if (icon.startsWith('/')) return convertFileSrc(icon);
    return icon;
  }
</script>

<div class="widget-header">
  <div class="header-top">
    <div class="widget-title-wrapper">
      <div class="widget-icon-container">
        {#if entityData?.icon}
          {#if isIconUrl(entityData.icon)}
            <img src={getIconUrl(entityData.icon)} alt="" class="entity-icon-image" />
          {:else}
            <span class="material-symbols-outlined entity-icon-symbol">{entityData.icon}</span>
          {/if}
        {:else}
          <span class="material-symbols-outlined entity-icon-symbol">info</span>
        {/if}
      </div>
      <div class="widget-title-info">
        <div class="widget-title">
          <span>{entityData?.label || 'Inspector'}</span>
        </div>
        {#if entityData?.types?.length > 0}
          <div class="header-types">
            {#each entityData.types as type, idx}
              {#if idx > 0}<span class="type-separator">·</span>{/if}
              <button class="type-link" onclick={() => onOpenEntityInspector(type.iri)}>
                {type.label}
              </button>
            {/each}
          </div>
        {/if}
      </div>
    </div>
    <div class="header-actions">
      <div class="header-action-buttons">
        {#each widgetDefinitions as def}
          {@const defIcon = WIDGET_TYPE_ICONS[def.widget_type] ?? 'open_in_new'}
          <button
            class="action-btn"
            onclick={() => onOpenWidget(def.widget_type)}
            title={def.description}
          >
            <span class="material-symbols-outlined">{defIcon}</span>
          </button>
        {/each}
        {#if entityData}
          <button
            class="action-btn"
            class:action-btn--locked={isLocked}
            onclick={onToggleLock}
            disabled={togglingLock}
            title={isLocked ? 'Unlock entity' : 'Lock entity'}
          >
            <span class="material-symbols-outlined">
              {isLocked ? 'lock' : 'lock_open'}
            </span>
          </button>
        {/if}
        {#if entityData && !entityData.isClass && !isLocked}
          <button
            class="action-btn action-btn--danger"
            onclick={onDelete}
            title="Delete"
          >
            <span class="material-symbols-outlined">delete_forever</span>
          </button>
        {/if}
        <button class="action-btn" onclick={onCopyIri} title="Copy IRI">
          <span class="material-symbols-outlined">content_copy</span>
        </button>
        <button
          class="action-btn"
          onclick={onToggleMinimize}
          title={windowState === 'minimized' ? 'Expand' : 'Minimize'}
        >
          <span class="material-symbols-outlined">
            {windowState === 'minimized' ? 'expand_more' : 'expand_less'}
          </span>
        </button>
        <button class="close-btn" onclick={onClose}>
          <span class="material-symbols-outlined">close</span>
        </button>
      </div>
      {#if entityData?.status}
        {@const statusIcon = entityData.status.icon || 'radio_button_checked'}
        <div class="status-badge-wrapper" bind:this={statusBadgeWrapperEl}>
          <button
            class="status-badge"
            class:clickable={entityData.allowedStatuses?.length > 0 && !isLocked}
            style="--status-color: {entityData.status.color || 'var(--color-neutral)'}"
            title={isLocked ? 'Entity is system-locked' : entityData.status.iri}
            onclick={() => {
              if (entityData.allowedStatuses?.length > 0 && !isLocked) showStatusPicker = !showStatusPicker;
            }}
          >
            <span class="material-symbols-outlined status-badge-icon">{statusIcon}</span>
            <span class="status-badge-label">{entityData.status.label}</span>
            {#if entityData.allowedStatuses?.length > 0}
              <span class="material-symbols-outlined status-badge-chevron">expand_more</span>
            {/if}
          </button>
          {#if showStatusPicker}
            <div class="status-picker" role="listbox">
              {#each entityData.allowedStatuses as s}
                {@const pickerIcon = s.icon || 'radio_button_checked'}
                <button
                  class="status-picker-item"
                  class:active={s.iri === entityData.status.iri}
                  style="--status-color: {s.color || 'var(--color-neutral)'}"
                  role="option"
                  aria-selected={s.iri === entityData.status.iri}
                  onclick={() => onUpdateStatus(s.iri)}
                >
                  <span class="material-symbols-outlined status-badge-icon">{pickerIcon}</span>
                  <span class="status-badge-label">{s.label}</span>
                </button>
              {/each}
            </div>
          {/if}
        </div>
      {/if}
    </div>
  </div>
</div>

<style>
  .widget-header {
    display: flex;
    flex-direction: column;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    border-bottom: 1px solid color-mix(in srgb, var(--color-white) 15%, transparent);
  }

  .header-top {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 16px;
  }

  .widget-title-wrapper {
    display: flex;
    flex-direction: row;
    gap: 12px;
    align-items: flex-start;
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }

  .widget-icon-container {
    flex-shrink: 0;
    padding-top: 2px;
  }

  .entity-icon-symbol {
    font-size: 28px;
    color: var(--color-neutral-active);
    display: flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
  }

  .entity-icon-image {
    width: 36px;
    height: 36px;
    border-radius: 6px;
    object-fit: cover;
  }

  .widget-title-info {
    display: flex;
    flex-direction: column;
    gap: 1px;
    flex: 1;
    min-width: 0;
  }

  .widget-title {
    font-family: var(--font-title);
    font-size: 11px;
    font-weight: 600;
    color: var(--color-neutral-active);
    overflow: hidden;
    text-align: left;
  }

  .widget-title span {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
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

  .header-actions {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 6px;
    flex-shrink: 0;
  }

  .header-action-buttons {
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

  .action-btn--locked {
    color: var(--color-warning, #f59e0b);
  }

  .action-btn--locked:hover {
    background: color-mix(in srgb, var(--color-warning, #f59e0b) 15%, transparent);
    color: var(--color-warning, #f59e0b);
  }

  .action-btn--danger {
    color: var(--color-danger, #ef4444);
  }

  .action-btn--danger:hover {
    background: color-mix(in srgb, var(--color-danger, #ef4444) 15%, transparent);
    color: var(--color-danger, #ef4444);
  }

  .header-types {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .type-link {
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    font-family: var(--font-body);
    font-size: 12px;
    color: var(--color-interactive);
    transition: all 0.2s;
    text-decoration: none;
  }

  .type-link:hover {
    color: var(--color-neutral-active);
    text-decoration: underline;
  }

  .type-separator {
    color: var(--color-neutral);
    opacity: 0.5;
    font-size: 12px;
  }

  .status-badge-wrapper {
    position: relative;
  }

  .status-badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 8px 3px 5px;
    background: color-mix(in srgb, var(--status-color) 18%, transparent);
    border: 1px solid color-mix(in srgb, var(--status-color) 40%, transparent);
    border-radius: 20px;
    cursor: default;
  }

  .status-badge.clickable {
    cursor: pointer;
  }

  .status-badge.clickable:hover {
    background: color-mix(in srgb, var(--status-color) 28%, transparent);
  }

  .status-badge-icon {
    font-size: 14px;
    color: var(--status-color);
  }

  .status-badge-label {
    font-family: var(--font-body);
    font-size: 11px;
    font-weight: 600;
    color: var(--status-color);
    white-space: nowrap;
  }

  .status-badge-chevron {
    font-size: 14px;
    color: var(--status-color);
    opacity: 0.7;
  }

  .status-picker {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    z-index: 1000;
    background: color-mix(in srgb, var(--color-black) 90%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-white) 15%, transparent);
    border-radius: 8px;
    padding: 4px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 160px;
    box-shadow: 0 6px 20px rgba(0, 0, 0, 0.5);
  }

  .status-picker-item {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 8px;
    border-radius: 6px;
    cursor: pointer;
    background: transparent;
    border: none;
    width: 100%;
    text-align: left;
  }

  .status-picker-item:hover {
    background: color-mix(in srgb, var(--status-color) 25%, transparent);
  }

  .status-picker-item.active {
    background: color-mix(in srgb, var(--status-color) 30%, transparent);
  }
</style>
