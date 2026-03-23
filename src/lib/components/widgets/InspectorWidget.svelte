<script>
  import { onMount, onDestroy, untrack } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import FilePreview from './inspector/FilePreview.svelte';
  import MetaFields from './inspector/MetaFields.svelte';
  import PropertyList from './inspector/PropertyList.svelte';
  import BacklinkList from './inspector/BacklinkList.svelte';
  import ClassPropertyForm from './inspector/ClassPropertyForm.svelte';
  import { sticky } from '$lib/utils/actions';

  let {
    entityId, widgetId, refreshKey = 0, windowState = 'normal',
    onWindowStateChange, conversationIri = null
  } = $props();

  function toggleMinimize() {
    onWindowStateChange?.(windowState === 'minimized' ? 'normal' : 'minimized');
  }


  let entityData = $state(null);
  let loading = $state(true);
  let error = $state(null);
  let widgetDefinitions = $state([]);
  let unlistenEntityUpdated = $state(null);
  let unlistenEntityReferenced = $state(null);
  let unlistenEntityDeleted = $state(null);
  let applicableAutomations = $state([]);
  let runningAutomationIri = $state(null);
  let togglingLock = $state(false);
  let showStatusPicker = $state(false);
  let statusBadgeWrapperEl = $state(null);
  let showDeleteConfirm = $state(false);
  let deleteSuccess = $state(false);
  let showAddPropertyForm = $state(false);
  let savingClassProperty = $state(false);
  let removeConfirmProp = $state(null);
  let removeConfirmCount = $state(0);
  let removeConfirmExamples = $state([]);
  let checkingUsage = $state(false);

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

      const typeIris = (entityData?.types ?? []).map(t => t.iri).filter(Boolean);
      if (typeIris.length > 0) {
        try {
          const raw = await invoke('automation__find_for_types', { typeIris });
          applicableAutomations = JSON.parse(raw);
        } catch {
          applicableAutomations = [];
        }
      } else {
        applicableAutomations = [];
      }
    } catch (err) {
      error = `Failed to load entity: ${entityId}`;
      console.error('Failed to load entity:', err);
    } finally {
      loading = false;
    }
  }

  async function deleteIndividual() {
    if (!entityData?.id) return;
    try {
      await invoke('widget_inspector__delete_individual', { entityId: entityData.id });
      deleteSuccess = true;
      setTimeout(() => closeWidget(), 1500);
    } catch (err) {
      console.error('Failed to delete individual:', err);
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

  async function saveProperty(propertyIri, value, datatype = null) {
    try {
      await invoke('widget_inspector__update_property', {
        entityId, propertyIri, value, datatype,
      });
    } catch (err) {
      console.error('Failed to update property:', err);
    }
  }

  async function saveReferences(propertyIri, iris) {
    try {
      await invoke('widget_inspector__set_references', { entityId, propertyIri, iris });
    } catch (err) {
      console.error('Failed to update references:', err);
    }
  }

  async function defineClassProperty(propertyIri, vals) {
    savingClassProperty = true;
    try {
      await invoke('widget_inspector__define_class_property', {
        classIri: entityId,
        propertyIri: propertyIri ?? null,
        label: vals.label,
        propertyType: vals.propertyType,
        range: vals.range,
        unit: vals.unit ?? null,
        comment: vals.comment ?? null,
      });
      showAddPropertyForm = false;
    } catch (err) {
      console.error('Failed to define class property:', err);
    } finally {
      savingClassProperty = false;
    }
  }

  async function saveCardinality(propertyIri, minCount, maxCount) {
    await invoke('widget_inspector__set_property_cardinality', {
      classId: entityId,
      propertyIri,
      minCount,
      maxCount,
    });
  }

  async function initiateRemoveProperty(propertyIri, propertyLabel) {
    checkingUsage = true;
    removeConfirmProp = null;
    try {
      const raw = await invoke('widget_inspector__check_property_usage', {
        propertyIri,
        classIri: entityId,
      });
      const result = JSON.parse(raw);
      if (result.count === 0) {
        await invoke('widget_inspector__remove_class_property', { propertyIri, classIri: entityId });
      } else {
        removeConfirmProp = { iri: propertyIri, label: propertyLabel };
        removeConfirmCount = result.count;
        removeConfirmExamples = result.examples ?? [];
      }
    } catch (err) {
      console.error('Failed to check property usage:', err);
    } finally {
      checkingUsage = false;
    }
  }

  async function confirmRemoveProperty() {
    if (!removeConfirmProp) return;
    try {
      await invoke('widget_inspector__remove_class_property', {
        propertyIri: removeConfirmProp.iri,
        classIri: entityId,
      });
    } catch (err) {
      console.error('Failed to remove property:', err);
    } finally {
      removeConfirmProp = null;
    }
  }

  async function updateStatus(statusIri) {
    showStatusPicker = false;
    try {
      await invoke('widget_inspector__update_status', { entityId, statusIri });
    } catch (err) {
      console.error('Failed to update status:', err);
    }
  }

  async function openEntityInspector(entityIri) {
    try {
      await invoke('widget_blackboard__add_widget', {
        widgetType: 'inspector',
        entityId: entityIri,
        position: null,
        size: null,
        conversationId: conversationIri
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
        size: null,
        conversationId: conversationIri
      });
    } catch (err) {
      console.error(`Failed to open ${widgetType} widget:`, err);
    }
  }

  const isLocked = $derived(entityData?.isLocked === true);

  async function toggleSystemLock() {
    if (!entityData?.id || togglingLock) return;
    togglingLock = true;
    try {
      await invoke('widget_inspector__set_system_locked', { entityId: entityData.id, locked: !isLocked });
    } catch (err) {
      console.error('Failed to toggle system lock:', err);
    } finally {
      togglingLock = false;
    }
  }

  const isAutomationWithoutInputClass = $derived(
    entityData?.types?.some(t => t.iri === 'foundation:Automation') &&
    !entityData?.properties?.some(p => p.property === 'foundation:inputClass')
  );

  async function runAutomation(automationIri, inputIri = null) {
    runningAutomationIri = automationIri;
    try {
      await invoke('automation__run', { automationIri, inputIri });
    } catch (err) {
      console.error('Failed to run automation:', err);
    } finally {
      runningAutomationIri = null;
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

  $effect(() => {
    if (!showStatusPicker) return;
    function handleDocClick(e) {
      if (statusBadgeWrapperEl && !statusBadgeWrapperEl.contains(e.target)) {
        showStatusPicker = false;
      }
    }
    document.addEventListener('click', handleDocClick, true);
    return () => document.removeEventListener('click', handleDocClick, true);
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

    // entity-referenced fires when a write creates a link TO an entity (new backlink).
    // Only reload if this inspector is showing the exact entity that gained a new backlink.
    unlistenEntityReferenced = await listen('entity-referenced', (event) => {
      if (event.payload.entityId === entityId) {
        loadEntity();
      }
    });

    unlistenEntityDeleted = await listen('entity-deleted', (event) => {
      if (event.payload.entityId === entityId) {
        closeWidget();
      }
    });
  });

  onDestroy(() => {
    if (unlistenEntityUpdated) unlistenEntityUpdated();
    if (unlistenEntityReferenced) unlistenEntityReferenced();
    if (unlistenEntityDeleted) unlistenEntityDeleted();
  });
</script>

<div class="inspector-widget" class:minimized={windowState === 'minimized'}>
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
          {#each widgetDefinitions as def}
            {@const defIcon = WIDGET_TYPE_ICONS[def.widget_type] ?? 'open_in_new'}
            <button
              class="action-btn"
              onclick={() => openWidgetForEntity(def.widget_type)}
              title={def.description}
            >
              <span class="material-symbols-outlined">{defIcon}</span>
            </button>
          {/each}
          {#if entityData}
            <button
              class="action-btn"
              class:action-btn--locked={isLocked}
              onclick={toggleSystemLock}
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
              onclick={() => showDeleteConfirm = true}
              title="Delete"
            >
              <span class="material-symbols-outlined">delete_forever</span>
            </button>
          {/if}
          <button class="action-btn" onclick={copyEntityIri} title="Copy IRI">
            <span class="material-symbols-outlined">content_copy</span>
          </button>
          <button
            class="action-btn"
            onclick={toggleMinimize}
            title={windowState === 'minimized' ? 'Expand' : 'Minimize'}
          >
            <span class="material-symbols-outlined">
              {windowState === 'minimized' ? 'expand_more' : 'expand_less'}
            </span>
          </button>
          <button class="close-btn" onclick={closeWidget}>
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
                    onclick={() => updateStatus(s.iri)}
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

  <div class="content-wrapper">
    <div class="widget-content">
    {#if loading && !entityData}
      <div class="loading">
        <span class="material-symbols-outlined spinning">progress_activity</span>
        <p>Loading...</p>
      </div>
    {:else if error && !entityData}
      <div class="error">
        <span class="material-symbols-outlined">error</span>
        <p>{error}</p>
      </div>
    {:else if entityData}
      <div class="content-scroll">
        {#if isLocked}
          <div class="locked-banner">
            <span class="material-symbols-outlined locked-banner-icon">lock</span>
            <div class="locked-banner-body">
              <span class="locked-banner-title">System Locked</span>
              <span class="locked-banner-desc">This entity is protected. Use the lock icon to unlock it.</span>
            </div>
          </div>
        {/if}
        <MetaFields
          label={entityData?.label}
          comment={entityData?.comment}
          onSave={isLocked ? null : saveProperty}
        />

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

        {#if entityData.isClass}
          <div class="class-props-header">
            <span class="class-props-title">Properties</span>
            {#if !isLocked}
              <button
                class="add-property-btn"
                onclick={() => showAddPropertyForm = !showAddPropertyForm}
                title="Add property"
              >
                <span class="material-symbols-outlined">{showAddPropertyForm ? 'close' : 'add'}</span>
                {showAddPropertyForm ? 'Cancel' : 'Add property'}
              </button>
            {/if}
          </div>
          {#if showAddPropertyForm}
            <ClassPropertyForm
              mode="add"
              saving={savingClassProperty}
              onsave={(vals) => defineClassProperty(null, vals)}
              oncancel={() => showAddPropertyForm = false}
            />
          {/if}
          {#if removeConfirmProp}
            <div class="remove-confirm-dialog">
              <div class="remove-confirm-icon">
                <span class="material-symbols-outlined">warning</span>
              </div>
              <div class="remove-confirm-body">
                <p class="remove-confirm-msg">
                  <strong>{removeConfirmCount}</strong> individual{removeConfirmCount !== 1 ? 's' : ''} of this class
                  {removeConfirmCount !== 1 ? 'have' : 'has'} a value for <em>{removeConfirmProp.label}</em>.
                  Removing it will hide the property from the schema but existing values will be preserved.
                </p>
                {#if removeConfirmExamples.length > 0}
                  <p class="remove-confirm-examples">{removeConfirmExamples.join(', ')}{removeConfirmCount > removeConfirmExamples.length ? '…' : ''}</p>
                {/if}
                <div class="remove-confirm-actions">
                  <button class="remove-confirm-proceed" onclick={confirmRemoveProperty}>Remove anyway</button>
                  <button class="remove-confirm-cancel" onclick={() => removeConfirmProp = null}>Cancel</button>
                </div>
              </div>
            </div>
          {/if}
        {/if}

        <PropertyList
          properties={entityData.isClass
            ? (entityData.properties ?? []).filter(
                p => p.property !== 'rdf:type' && p.property !== 'rdfs:subClassOf')
            : (entityData.properties ?? []).filter(
                p => p.property !== 'foundation:hasStatus' && p.property !== 'rdf:type')}
          requiredFields={entityData.requiredFields ?? []}
          isClass={entityData.isClass}
          {openEntityInspector}
          onSave={entityData.isClass || isLocked ? null : saveProperty}
          onSaveReference={entityData.isClass || isLocked ? null : saveReferences}
          onRemoveProperty={entityData.isClass && !isLocked ? initiateRemoveProperty : null}
          onSaveCardinality={entityData.isClass && !isLocked ? saveCardinality : null}
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
  </div>

  {#if showDeleteConfirm}
    <div class="delete-overlay" role="dialog" aria-modal="true">
      <div class="delete-dialog">
        <span class="material-symbols-outlined delete-dialog-icon">delete_forever</span>
        <p class="delete-dialog-title">Delete "{entityData?.label}"?</p>
        <p class="delete-dialog-warning">This cannot be undone from the UI.</p>
        <div class="delete-dialog-actions">
          <button class="delete-cancel-btn" onclick={() => showDeleteConfirm = false}>Cancel</button>
          <button class="delete-confirm-btn" onclick={() => { showDeleteConfirm = false; deleteIndividual(); }}>Delete</button>
        </div>
      </div>
    </div>
  {/if}

  {#if deleteSuccess}
    <div class="delete-toast">
      <span class="material-symbols-outlined">check_circle</span>
      "{entityData?.label}" deleted
    </div>
  {/if}

  {#if entityData && !entityData.isClass
    && (isAutomationWithoutInputClass || applicableAutomations.length > 0)}
    <div class="actions-bar">
      {#if isAutomationWithoutInputClass}
        {@const isRunning = runningAutomationIri === entityId}
        <button
          class="action-bar-btn"
          onclick={() => runAutomation(entityId)}
          disabled={isRunning}
        >
          <span class="material-symbols-outlined" class:spinning={isRunning}>
            {isRunning ? 'progress_activity' : 'play_circle'}
          </span>
          Run
        </button>
      {/if}
      {#each applicableAutomations as auto}
        {@const isAutoRunning = runningAutomationIri === auto.iri}
        <button
          class="action-bar-btn"
          onclick={() => runAutomation(auto.iri, entityId)}
          disabled={isAutoRunning}
          title={auto.label}
        >
          <span class="material-symbols-outlined" class:spinning={isAutoRunning}>
            {isAutoRunning ? 'progress_activity' : 'play_circle'}
          </span>
          {auto.label}
        </button>
      {/each}
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
    position: relative;
  }

  .content-wrapper {
    display: grid;
    grid-template-rows: 1fr;
    transition: grid-template-rows 250ms cubic-bezier(0.4, 0, 0.2, 1);
    overflow: hidden;
  }

  .inspector-widget.minimized .content-wrapper {
    grid-template-rows: 0fr;
  }

  .inspector-widget.minimized .header-top {
    padding: 8px 12px;
  }

  .inspector-widget.minimized .header-actions {
    gap: 2px;
  }

  .content-wrapper > .widget-content {
    min-height: 0;
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

  .actions-bar {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 10px 12px;
    border-top: 1px solid color-mix(in srgb, var(--color-white) 10%, transparent);
    background: color-mix(in srgb, var(--color-black) 60%, transparent);
    flex-shrink: 0;
  }

  .action-bar-btn {
    display: flex;
    align-items: center;
    justify-content: flex-start;
    gap: 6px;
    width: 100%;
    padding: 8px 12px;
    background: color-mix(in srgb, var(--color-interactive) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-interactive) 35%, transparent);
    border-radius: 6px;
    color: var(--color-interactive);
    font-family: var(--font-body);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .action-bar-btn .material-symbols-outlined {
    font-size: 16px;
    flex-shrink: 0;
  }

  .action-bar-btn:hover {
    background: color-mix(in srgb, var(--color-interactive) 22%, transparent);
    border-color: color-mix(in srgb, var(--color-interactive) 60%, transparent);
    color: var(--color-neutral-active);
  }

  .action-bar-btn:disabled {
    opacity: 0.6;
    cursor: default;
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
    overflow-y: auto;
  }

  .content-scroll {
    padding: 16px;
  }

  .locked-banner {
    display: flex;
    align-items: flex-start;
    gap: 10px;
    padding: 10px 12px;
    margin-bottom: 14px;
    background: color-mix(in srgb, var(--color-warning, #f59e0b) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-warning, #f59e0b) 35%, transparent);
    border-radius: 8px;
  }

  .locked-banner-icon {
    font-size: 18px;
    color: var(--color-warning, #f59e0b);
    flex-shrink: 0;
    padding-top: 1px;
  }

  .locked-banner-body {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .locked-banner-title {
    font-family: var(--font-body);
    font-size: 12px;
    font-weight: 700;
    color: var(--color-warning, #f59e0b);
  }

  .locked-banner-desc {
    font-family: var(--font-body);
    font-size: 12px;
    color: var(--color-neutral);
    line-height: 1.4;
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

  .class-props-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 4px 0 8px;
    margin-top: 4px;
  }

  .class-props-title {
    font-family: var(--font-body);
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: color-mix(in srgb, var(--color-neutral) 60%, transparent);
  }

  .add-property-btn {
    display: flex;
    align-items: center;
    gap: 3px;
    padding: 3px 8px;
    background: color-mix(in srgb, var(--color-interactive) 15%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-interactive) 30%, transparent);
    border-radius: 5px;
    color: var(--color-interactive);
    font-family: var(--font-body);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    transition: background 0.15s;
  }

  .add-property-btn:hover {
    background: color-mix(in srgb, var(--color-interactive) 25%, transparent);
  }

  .add-property-btn .material-symbols-outlined {
    font-size: 14px;
  }

  .remove-confirm-dialog {
    display: flex;
    gap: 10px;
    padding: 12px;
    background: color-mix(in srgb, var(--color-warning, #f59e0b) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-warning, #f59e0b) 30%, transparent);
    border-radius: 8px;
    margin-bottom: 8px;
  }

  .remove-confirm-icon .material-symbols-outlined {
    font-size: 20px;
    color: var(--color-warning, #f59e0b);
  }

  .remove-confirm-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .remove-confirm-msg {
    font-family: var(--font-body);
    font-size: 12px;
    color: var(--color-neutral-active);
    margin: 0;
    line-height: 1.5;
  }

  .remove-confirm-examples {
    font-family: var(--font-body);
    font-size: 11px;
    color: var(--color-neutral);
    margin: 0;
    font-style: italic;
  }

  .remove-confirm-actions {
    display: flex;
    gap: 6px;
    margin-top: 2px;
  }

  .remove-confirm-proceed {
    padding: 4px 10px;
    background: color-mix(in srgb, var(--color-error, #ef4444) 20%, transparent);
    border: none;
    border-radius: 4px;
    color: var(--color-error, #ef4444);
    font-family: var(--font-body);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    transition: background 0.15s;
  }

  .remove-confirm-proceed:hover {
    background: color-mix(in srgb, var(--color-error, #ef4444) 30%, transparent);
  }

  .remove-confirm-cancel {
    padding: 4px 10px;
    background: color-mix(in srgb, var(--color-neutral) 12%, transparent);
    border: none;
    border-radius: 4px;
    color: var(--color-neutral);
    font-family: var(--font-body);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    transition: background 0.15s;
  }

  .remove-confirm-cancel:hover {
    background: color-mix(in srgb, var(--color-neutral) 22%, transparent);
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

  .delete-overlay {
    position: absolute;
    inset: 0;
    background: color-mix(in srgb, var(--color-black) 75%, transparent);
    backdrop-filter: blur(4px);
    border-radius: 12px;
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 100;
  }

  .delete-dialog {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 10px;
    padding: 24px 20px;
    background: color-mix(in srgb, var(--color-black) 92%, transparent);
    border: 1px solid color-mix(in srgb, #ef4444 40%, transparent);
    border-radius: 10px;
    max-width: 220px;
    text-align: center;
  }

  .delete-dialog-icon {
    font-size: 32px;
    color: #ef4444;
  }

  .delete-dialog-title {
    font-family: var(--font-body);
    font-size: 13px;
    font-weight: 600;
    color: var(--color-neutral-active);
    margin: 0;
    word-break: break-word;
  }

  .delete-dialog-warning {
    font-family: var(--font-body);
    font-size: 11px;
    color: var(--color-neutral);
    margin: 0;
    opacity: 0.7;
  }

  .delete-dialog-actions {
    display: flex;
    gap: 8px;
    margin-top: 4px;
  }

  .delete-cancel-btn {
    padding: 6px 14px;
    background: color-mix(in srgb, var(--color-white) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-white) 20%, transparent);
    border-radius: 6px;
    color: var(--color-neutral-active);
    font-family: var(--font-body);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s;
  }

  .delete-cancel-btn:hover {
    background: color-mix(in srgb, var(--color-white) 15%, transparent);
  }

  .delete-confirm-btn {
    padding: 6px 14px;
    background: color-mix(in srgb, #ef4444 20%, transparent);
    border: 1px solid color-mix(in srgb, #ef4444 50%, transparent);
    border-radius: 6px;
    color: #ef4444;
    font-family: var(--font-body);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s;
  }

  .delete-confirm-btn:hover {
    background: color-mix(in srgb, #ef4444 35%, transparent);
    color: var(--color-neutral-active);
  }

  .delete-toast {
    position: absolute;
    bottom: 12px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 14px;
    background: color-mix(in srgb, #22c55e 20%, var(--color-black));
    border: 1px solid color-mix(in srgb, #22c55e 50%, transparent);
    border-radius: 20px;
    font-family: var(--font-body);
    font-size: 12px;
    font-weight: 600;
    color: #22c55e;
    white-space: nowrap;
    z-index: 101;
    pointer-events: none;
  }

  .delete-toast .material-symbols-outlined {
    font-size: 16px;
  }
</style>
