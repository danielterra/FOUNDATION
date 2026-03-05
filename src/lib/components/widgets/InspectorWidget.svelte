<script>
  import { onMount, onDestroy, untrack } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { marked } from 'marked';
  import { listen } from '@tauri-apps/api/event';
  import FilePreview from './inspector/FilePreview.svelte';
  import PropertyList from './inspector/PropertyList.svelte';
  import BacklinkList from './inspector/BacklinkList.svelte';

  let { entityId, widgetId, refreshKey = 0 } = $props();

  let entityData = $state(null);
  let loading = $state(true);
  let error = $state(null);
  let unlistenEntityUpdated = $state(null);

  async function loadEntity() {
    loading = true;
    error = null;

    try {
      const resultStr = await invoke('entity__get', { entityId });
      entityData = JSON.parse(resultStr);
    } catch (err) {
      error = `Failed to load entity: ${entityId}`;
      console.error('Failed to load entity:', err);
    } finally {
      loading = false;
    }
  }

  async function closeWidget() {
    try {
      await invoke('widget__remove', { widgetId });
    } catch (err) {
      console.error('Failed to remove widget:', err);
    }
  }

  async function copyEntityIri() {
    if (!entityData?.id) return;
    try {
      await navigator.clipboard.writeText(entityData.id);
    } catch (err) {
      console.error('Failed to copy IRI:', err);
    }
  }

  async function openEntityInspector(entityIri) {
    try {
      await invoke('widget__add', {
        widgetType: 'inspector',
        entityId: entityIri,
        position: null,
        size: null
      });
    } catch (err) {
      console.error('Failed to open inspector:', err);
    }
  }

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

  $effect(() => {
    refreshKey;
    untrack(() => loadEntity());
  });

  onMount(async () => {
    unlistenEntityUpdated = await listen('entity-updated', (event) => {
      const updatedId = event.payload.entityId;
      if (updatedId === entityId) {
        loadEntity();
        return;
      }
      if (entityData) {
        const inBacklinks = entityData.backlinks?.some(b => b.value === updatedId);
        const inProperties = entityData.properties?.some(p => p.value === updatedId);
        if (inBacklinks || inProperties) {
          loadEntity();
        }
      }
    });
  });

  onDestroy(() => {
    if (unlistenEntityUpdated) {
      unlistenEntityUpdated();
    }
  });
</script>

<div class="inspector-widget">
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
                <button class="type-link" onclick={() => openEntityInspector(type.iri)}>
                  {type.label}
                </button>
              {/each}
            </div>
          {/if}
        </div>
      </div>
      <div class="header-actions">
        <div class="header-action-buttons">
          <button class="action-btn" onclick={copyEntityIri} title="Copy IRI">
            <span class="material-symbols-outlined">content_copy</span>
          </button>
          <button class="close-btn" onclick={closeWidget}>
            <span class="material-symbols-outlined">close</span>
          </button>
        </div>
        {#if entityData?.status}
          <div
            class="status-badge"
            style="--status-color: {entityData.status.color || 'var(--color-neutral)'}"
            title={entityData.status.iri}
          >
            <span class="material-symbols-outlined status-badge-icon">radio_button_checked</span>
            <span class="status-badge-label">{entityData.status.label}</span>
          </div>
        {/if}
      </div>
    </div>
  </div>

  <div class="widget-content">
    {#if loading}
      <div class="loading">
        <span class="material-symbols-outlined spinning">progress_activity</span>
        <p>Loading...</p>
      </div>
    {:else if error}
      <div class="error">
        <span class="material-symbols-outlined">error</span>
        <p>{error}</p>
      </div>
    {:else if entityData}
      <div class="content-scroll">
        {#if entityData?.label}
          <div class="entity-full-name">{entityData.label}</div>
        {/if}

        {#if entityData.comment}
          <div class="description markdown-content">
            {@html marked.parse(entityData.comment)}
          </div>
        {/if}

        <FilePreview {entityData} />

        {#if entityData.superClasses?.length > 0}
          <div class="thing-list">
            {#each entityData.superClasses as superClass}
              <div
                class="thing-item clickable"
                role="button"
                tabindex="0"
                onclick={() => openEntityInspector(superClass.iri)}
                onkeydown={(e) => e.key === 'Enter' && openEntityInspector(superClass.iri)}
              >
                {#if superClass.icon}
                  {#if isIconUrl(superClass.icon)}
                    <img src={getIconUrl(superClass.icon)} alt="" class="thing-icon-image" />
                  {:else}
                    <span class="material-symbols-outlined">{superClass.icon}</span>
                  {/if}
                {/if}
                <span class="thing-label">{superClass.label}</span>
              </div>
            {/each}
          </div>
        {/if}

        {#if entityData.subClasses?.length > 0}
          <div class="thing-list">
            {#each entityData.subClasses as subClass}
              <div
                class="thing-item clickable"
                role="button"
                tabindex="0"
                onclick={() => openEntityInspector(subClass.iri)}
                onkeydown={(e) => e.key === 'Enter' && openEntityInspector(subClass.iri)}
              >
                {#if subClass.icon}
                  {#if isIconUrl(subClass.icon)}
                    <img src={getIconUrl(subClass.icon)} alt="" class="thing-icon-image" />
                  {:else}
                    <span class="material-symbols-outlined">{subClass.icon}</span>
                  {/if}
                {/if}
                <span class="thing-label">{subClass.label}</span>
              </div>
            {/each}
          </div>
        {/if}

        <PropertyList properties={entityData.properties} {openEntityInspector} />

        <BacklinkList backlinks={entityData.backlinks} {openEntityInspector} />

        {#if entityData.instances?.length > 0}
          <div class="thing-list">
            {#each entityData.instances as instance}
              <div
                class="thing-item instance clickable"
                role="button"
                tabindex="0"
                onclick={() => openEntityInspector(instance.iri)}
                onkeydown={(e) => e.key === 'Enter' && openEntityInspector(instance.iri)}
              >
                {#if instance.icon}
                  {#if isIconUrl(instance.icon)}
                    <img src={getIconUrl(instance.icon)} alt="" class="thing-icon-image" />
                  {:else}
                    <span class="material-symbols-outlined">{instance.icon}</span>
                  {/if}
                {/if}
                <span class="thing-label">{instance.label}</span>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .inspector-widget {
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
    align-items: center;
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }

  .widget-icon-container {
    flex-shrink: 0;
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
    display: flex;
    align-items: center;
    font-family: var(--font-title);
    font-size: 11px;
    font-weight: 600;
    color: var(--color-neutral-active);
    overflow: hidden;
  }

  .widget-title span {
    overflow: hidden;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
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

  .widget-content {
    flex: 1;
    overflow-y: auto;
  }

  .content-scroll {
    padding: 16px;
  }

  .loading, .error {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 40px;
    text-align: center;
    color: var(--color-neutral);
  }

  .loading .material-symbols-outlined,
  .error .material-symbols-outlined {
    font-size: 48px;
    opacity: 0.5;
  }

  .spinning {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  .entity-full-name {
    font-family: var(--font-title);
    font-size: 16px;
    font-weight: 700;
    color: var(--color-neutral-active);
    line-height: 1.4;
    margin-bottom: 12px;
    word-break: break-word;
  }

  .description {
    margin: 0 0 16px 0;
    font-size: 14px;
    line-height: 1.6;
    color: var(--color-neutral);
    word-wrap: break-word;
  }

  .thing-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 16px;
  }

  .thing-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    border-radius: 6px;
    transition: all 0.2s;
  }

  .thing-item:hover {
    background: color-mix(in srgb, var(--color-white) 8%, transparent);
  }

  .clickable {
    cursor: pointer;
    user-select: none;
  }

  .clickable:hover {
    background: color-mix(in srgb, var(--color-interactive) 20%, transparent) !important;
    transform: translateX(2px);
  }

  .clickable:active {
    transform: translateX(1px);
  }

  .thing-item.instance {
    border-left: 3px solid var(--color-interactive);
  }

  .thing-item .material-symbols-outlined {
    font-size: 18px;
    color: var(--color-interactive);
  }

  .thing-icon-image {
    width: 28px;
    height: 28px;
    border-radius: 5px;
    object-fit: cover;
  }

  .thing-label {
    font-family: var(--font-body);
    font-size: 13px;
    color: var(--color-neutral-active);
  }

  .status-badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 8px 3px 5px;
    background: color-mix(in srgb, var(--status-color) 18%, transparent);
    border: 1px solid color-mix(in srgb, var(--status-color) 40%, transparent);
    border-radius: 20px;
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
</style>
