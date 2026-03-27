<script>
  import { onMount, onDestroy, untrack } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import FilePreview from './inspector/FilePreview.svelte';
  import MetaFields from './inspector/MetaFields.svelte';
  import PropertyList from './inspector/PropertyList.svelte';
  import BacklinkList from './inspector/BacklinkList.svelte';
  import InspectorHeader from './inspector/InspectorHeader.svelte';
  import InspectorClassView from './inspector/InspectorClassView.svelte';

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
      const t0 = performance.now();
      const resultStr = await invoke('inspector__get_entity', { entityId });
      const t1 = performance.now();
      entityData = JSON.parse(resultStr);

      const classIri = entityData?.types?.[0]?.iri ?? null;
      const typeIris = (entityData?.types ?? []).map(t => t.iri).filter(Boolean);

      console.debug(`[INSPECTOR] ${entityId} get_entity=${Math.round(t1 - t0)}ms`);

      const t2 = performance.now();
      const [defsResult, automationsResult] = await Promise.allSettled([
        classIri
          ? invoke('widget_blackboard__list_widget_definitions', { classIri })
          : Promise.resolve([]),
        typeIris.length > 0
          ? invoke('automation__find_for_types', { typeIris })
          : Promise.resolve('[]'),
      ]);

      const t3 = performance.now();
      console.debug(`[INSPECTOR] ${entityId} defs+automations=${Math.round(t3 - t2)}ms total=${Math.round(t3 - t0)}ms`);

      widgetDefinitions = defsResult.status === 'fulfilled'
        ? defsResult.value.filter(d => d.widget_type !== 'inspector')
        : [];

      applicableAutomations = automationsResult.status === 'fulfilled'
        ? JSON.parse(automationsResult.value)
        : [];
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
  <InspectorHeader
    {entityData}
    {widgetDefinitions}
    {windowState}
    {isLocked}
    {togglingLock}
    bind:showStatusPicker
    bind:statusBadgeWrapperEl
    onToggleMinimize={toggleMinimize}
    onClose={closeWidget}
    onDelete={() => showDeleteConfirm = true}
    onCopyIri={copyEntityIri}
    onToggleLock={toggleSystemLock}
    onOpenEntityInspector={openEntityInspector}
    onOpenWidget={openWidgetForEntity}
    onUpdateStatus={updateStatus}
  />

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

        <InspectorClassView
          {entityData}
          {isLocked}
          bind:showAddPropertyForm
          {savingClassProperty}
          bind:removeConfirmProp
          {removeConfirmCount}
          {removeConfirmExamples}
          {checkingUsage}
          {openEntityInspector}
          onDefineProperty={defineClassProperty}
          onConfirmRemoveProperty={confirmRemoveProperty}
          onCancelRemoveProperty={() => removeConfirmProp = null}
        />

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
      </div>

      {#if !entityData.isClass && (isAutomationWithoutInputClass || applicableAutomations.length > 0)}
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
    box-shadow: 0 8px 32px color-mix(in srgb, var(--color-black) 40%, transparent);
    position: relative;
  }

  .content-wrapper {
    display: grid;
    grid-template-rows: 1fr;
    transition: grid-template-rows 250ms cubic-bezier(0.4, 0, 0.2, 1);
    overflow: hidden;
    flex: 1;
    min-height: 0;
  }

  .inspector-widget.minimized .content-wrapper {
    grid-template-rows: 0fr;
  }

  .inspector-widget.minimized :global(.widget-header) {
    border-bottom: none;
  }

  .inspector-widget.minimized :global(.header-top) {
    padding: 8px 12px;
  }

  .inspector-widget.minimized :global(.header-actions) {
    gap: 2px;
  }

  .content-wrapper > .widget-content {
    min-height: 0;
  }

  .widget-content {
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .content-scroll {
    flex: 1;
    overflow-y: auto;
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
