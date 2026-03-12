<script>
  import { onMount, onDestroy, untrack } from 'svelte';
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import { invoke } from '@tauri-apps/api/core';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { marked } from 'marked';
  import { listen } from '@tauri-apps/api/event';
  import FilePreview from './inspector/FilePreview.svelte';
  import PropertyList from './inspector/PropertyList.svelte';
  import BacklinkList from './inspector/BacklinkList.svelte';

  let { entityId, widgetId, refreshKey = 0, onResize } = $props();

  let minimized = $state(false);
  let storedHeight = $state(null);
  let widgetEl = $state(null);
  let headerEl = $state(null);

  async function toggleMinimize() {
    if (!minimized) {
      storedHeight = widgetEl?.offsetHeight ?? 500;
      const headerHeight = headerEl?.offsetHeight ?? 70;
      const width = widgetEl?.offsetWidth ?? 320;
      minimized = true;
      await new Promise(r => setTimeout(r, 260));
      onResize?.(width, headerHeight);
    } else {
      const width = widgetEl?.offsetWidth ?? 320;
      onResize?.(width, storedHeight ?? 500);
      minimized = false;
    }
  }

  function sticky(node, { top = 0 } = {}) {
    let scroller, section, nodeTop, sectionTop;

    function findScroller() {
      let el = node.parentElement;
      while (el) {
        const ov = getComputedStyle(el).overflowY;
        if (ov === 'auto' || ov === 'scroll') return el;
        el = el.parentElement;
      }
      return null;
    }

    function computeOffsets() {
      const saved = node.style.transform;
      node.style.transform = 'none';
      const scrollerRect = scroller.getBoundingClientRect();
      const nr = node.getBoundingClientRect();
      const sr = section.getBoundingClientRect();
      node.style.transform = saved;
      nodeTop = nr.top - scrollerRect.top + scroller.scrollTop;
      sectionTop = sr.top - scrollerRect.top + scroller.scrollTop;
    }

    function onScroll() {
      const scrollTop = scroller.scrollTop;
      const sectionHeight = section.offsetHeight;
      const nodeHeight = node.offsetHeight;
      if (scrollTop + top > nodeTop) {
        const shift = Math.max(0, Math.min(
          scrollTop + top - nodeTop,
          sectionTop + sectionHeight - nodeTop - nodeHeight
        ));
        node.style.transform = `translateY(${shift}px)`;
      } else {
        node.style.transform = '';
      }
    }

    requestAnimationFrame(() => {
      scroller = findScroller();
      section = node.parentElement;
      if (!scroller) return;
      computeOffsets();
      scroller.addEventListener('scroll', onScroll, { passive: true });
      onScroll();
    });

    return {
      destroy() {
        if (scroller) scroller.removeEventListener('scroll', onScroll);
      }
    };
  }

  let entityData = $state(null);
  let loading = $state(true);
  let error = $state(null);
  let widgetDefinitions = $state([]);
  let unlistenEntityUpdated = $state(null);

  async function loadEntity() {
    loading = true;
    error = null;

    try {
      const resultStr = await invoke('inspector__get_entity', { entityId });
      entityData = JSON.parse(resultStr);

      const conceptIri = entityData?.types?.[0]?.iri ?? null;
      if (conceptIri) {
        try {
          const defs = await invoke('widget_blackboard__list_widget_definitions', { conceptIri });
          widgetDefinitions = defs.filter(d => d.widget_type !== 'inspector');
        } catch {
          widgetDefinitions = [];
        }
      } else {
        widgetDefinitions = [];
      }
    } catch (err) {
      error = `Failed to load entity: ${entityId}`;
      console.error('Failed to load entity:', err);
    } finally {
      loading = false;
    }
  }

  async function closeWidget() {
    try {
      await invoke('widget_blackboard__remove_widget', { widgetId });
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

  async function saveProperty(propertyIri, value) {
    try {
      await invoke('widget_inspector__update_property', { entityId, propertyIri, value });
    } catch (err) {
      console.error('Failed to update property:', err);
    }
  }

  async function openEntityInspector(entityIri) {
    try {
      await invoke('widget_blackboard__add_widget', {
        widgetType: 'inspector',
        entityId: entityIri,
        position: null,
        size: null
      });
    } catch (err) {
      console.error('Failed to open inspector:', err);
    }
  }

  async function openWidgetForEntity(widgetType) {
    try {
      await invoke('widget_blackboard__add_widget', {
        widgetType,
        entityId,
        position: null,
        size: null
      });
    } catch (err) {
      console.error(`Failed to open ${widgetType} widget:`, err);
    }
  }

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

<div class="inspector-widget" bind:this={widgetEl}>
  <div class="widget-header" bind:this={headerEl}>
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
          {#each widgetDefinitions as def}
            <button class="action-btn" onclick={() => openWidgetForEntity(def.widget_type)} title={def.description}>
              <span class="material-symbols-outlined">{WIDGET_TYPE_ICONS[def.widget_type] ?? 'open_in_new'}</span>
            </button>
          {/each}
          <button class="action-btn" onclick={copyEntityIri} title="Copy IRI">
            <span class="material-symbols-outlined">content_copy</span>
          </button>
          <button class="action-btn" onclick={toggleMinimize} title={minimized ? 'Expand' : 'Minimize'}>
            <span class="material-symbols-outlined">{minimized ? 'expand_more' : 'expand_less'}</span>
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

  {#if !minimized}
    <div class="widget-content" transition:slide={{ duration: 250, easing: cubicOut }}>
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
          <div class="section-group">
            <div class="section-label" use:sticky={{ top: 0 }}>Parent Classes</div>
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
          </div>
        {/if}

        {#if entityData.subClasses?.length > 0}
          <div class="section-group">
            <div class="section-label" use:sticky={{ top: 0 }}>Child Classes</div>
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
          </div>
        {/if}

        {#if entityData.isClass && entityData.allowedStatuses?.length > 0}
          <div class="section-group">
            <div class="section-label" use:sticky={{ top: 0 }}>Allowed Statuses</div>
            <div class="thing-list">
              {#each entityData.allowedStatuses as status}
                <div
                  class="thing-item clickable status-item"
                  style="--status-color: {status.color || 'var(--color-neutral)'}"
                  role="button"
                  tabindex="0"
                  onclick={() => openEntityInspector(status.iri)}
                  onkeydown={(e) => e.key === 'Enter' && openEntityInspector(status.iri)}
                >
                  {#if status.icon}
                    {#if isIconUrl(status.icon)}
                      <img src={getIconUrl(status.icon)} alt="" class="thing-icon-image" />
                    {:else}
                      <span class="material-symbols-outlined status-dot">{status.icon}</span>
                    {/if}
                  {:else}
                    <span class="material-symbols-outlined status-dot">radio_button_checked</span>
                  {/if}
                  <span class="thing-label">{status.label}</span>
                </div>
              {/each}
            </div>
          </div>
        {/if}

        <PropertyList
          properties={entityData.isClass
            ? (entityData.properties ?? []).filter(p => p.property !== 'rdf:type' && p.property !== 'rdfs:subClassOf')
            : (entityData.properties ?? []).filter(p => p.property !== 'foundation:hasStatus' && p.property !== 'rdf:type')}
          requiredFields={entityData.requiredFields ?? []}
          isClass={entityData.isClass}
          {openEntityInspector}
          onSave={entityData.isClass ? null : saveProperty}
        />

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
  {/if}
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

  .section-group {
    display: flex;
    flex-direction: column;
  }

  .section-label {
    font-family: var(--font-body);
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: color-mix(in srgb, var(--color-neutral) 60%, transparent);
    margin-bottom: 6px;
    z-index: 2;
    padding: 6px 0 4px;
    background: color-mix(in srgb, var(--color-black) 97%, transparent);
    backdrop-filter: blur(12px);
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

  .thing-item.status-item {
    border-left: 3px solid var(--status-color);
  }

  .status-dot {
    font-size: 16px;
    color: var(--status-color);
    flex-shrink: 0;
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
