<script>
  import { onDestroy, untrack } from 'svelte';

  import { invoke } from '@tauri-apps/api/core';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { createEntitySubscription } from '$lib/realtime/subscriptions';
  import { deleteConfirm } from '$lib/stores/deleteConfirm';
  import FilePreview from './inspector/FilePreview.svelte';
  import MetaFields from './inspector/MetaFields.svelte';
  import PropertyList from './inspector/PropertyList.svelte';
  import InspectorClassView from './inspector/InspectorClassView.svelte';
  import WidgetContainer from './WidgetContainer.svelte';
  import Button from '../Button.svelte';

  let {
    entityId, widgetId, refreshKey = 0, windowState = 'normal',
    onWindowStateChange, conversationIri = null
  } = $props();

  let entityData = $state(null);
  let loading = $state(true);
  let error = $state(null);
  let loadPending = false;
  let reloadWhenDone = false;
  let loadDebounceTimer = null;
  let widgetDefinitions = $state([]);
  const entitySub = createEntitySubscription((event) => {
    const { type, entityId: updatedId } = event;
    if (type === 'updated') {
      if (updatedId === entityId) { scheduleLoad(); return; }
      if (entityData?.properties?.some(p => p.value === updatedId)) scheduleLoad();
      return;
    }
    if (type === 'referenced') {
      if (updatedId === entityId) { scheduleLoad(); return; }
      if (entityData?.types?.some(t => t.iri === updatedId)) reloadAutomations();
      return;
    }
    if (type === 'deleted') {
      if (updatedId === entityId) { closeWidget(); return; }
      if (entityData?.properties?.some(p => p.value === updatedId)) scheduleLoad();
    }
  });
  let applicableAutomations = $state([]);
  let runningAutomationIri = $state(null);
  let togglingLock = $state(false);
  let showStatusPicker = $state(false);
  let statusBadgeWrapperEl = $state(null);
  let deleteSuccess = $state(false);
  let showAddPropertyForm = $state(false);
  let savingClassProperty = $state(false);
  let removeConfirmProp = $state(null);
  let removeConfirmCount = $state(0);
  let removeConfirmExamples = $state([]);
  let checkingUsage = $state(false);
  let showAddDisjointForm = $state(false);
  let savingDisjoint = $state(false);
  let disjointError = $state(null);
  let removeDisjointConfirm = $state(null);

  async function reloadAutomations() {
    const typeIris = (entityData?.types ?? []).map(t => t.iri).filter(Boolean);
    if (!typeIris.length) return;
    try {
      const result = await invoke('automation__find_for_types', { typeIris });
      applicableAutomations = JSON.parse(result);
    } catch (err) {
      console.error('Failed to reload automations:', err);
    }
  }

  function scheduleLoad() {
    if (loadDebounceTimer !== null) clearTimeout(loadDebounceTimer);
    loadDebounceTimer = setTimeout(() => {
      loadDebounceTimer = null;
      loadEntity();
    }, 300);
  }

  async function loadEntity() {
    if (loadPending) {
      reloadWhenDone = true;
      console.debug(`[INSPECTOR] ${entityId} queued (loadPending=true)`);
      return;
    }
    loadPending = true;
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
      {
        const watchProps = ['foundation:result', 'foundation:startedAt'];
        for (const iri of watchProps) {
          const p = (entityData?.properties ?? []).find(p => p.property === iri);
          if (p) console.debug(`[INSPECTOR] ${entityId} prop ${iri}: value="${p.value?.slice(0,60)}" is_empty=${p.is_empty}`);
          else console.debug(`[INSPECTOR] ${entityId} prop ${iri}: NOT IN properties array`);
        }
      }

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
        ? defsResult.value.filter(d => !['inspector', 'notification_center', 'ai_call_history'].includes(d.widget_type))
        : [];

      applicableAutomations = automationsResult.status === 'fulfilled'
        ? JSON.parse(automationsResult.value)
        : [];
    } catch (err) {
      error = `Failed to load entity: ${entityId}`;
      console.error('Failed to load entity:', err);
    } finally {
      loadPending = false;
      loading = false;
      if (reloadWhenDone) {
        reloadWhenDone = false;
        console.debug(`[INSPECTOR] ${entityId} reloadWhenDone → scheduling next`);
        scheduleLoad();
      }
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

  async function initiateDelete() {
    if (!entityData?.id) return;
    try {
      const raw = await invoke('widget_inspector__get_delete_impact', { entityId: entityData.id });
      const impact = JSON.parse(raw);
      deleteConfirm.set({
        entityId: entityData.id,
        entityLabel: entityData.label ?? entityData.id,
        cascade_items: impact.cascade_items ?? [],
        backlink_count: impact.backlink_count,
        onConfirm: () => {
          deleteConfirm.set(null);
          deleteIndividual();
        },
        onCancel: () => deleteConfirm.set(null),
      });
    } catch (err) {
      console.error('Failed to get delete impact:', err);
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

  async function clearProperty(propertyIri) {
    try {
      await invoke('widget_inspector__clear_property', { entityId, propertyIri });
    } catch (err) {
      console.error('Failed to clear property:', err);
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

  async function addDisjoint(disjointIri) {
    savingDisjoint = true;
    disjointError = null;
    try {
      await invoke('widget_inspector__add_class_disjoint', {
        classIri: entityId,
        disjointIri,
      });
      showAddDisjointForm = false;
    } catch (err) {
      disjointError = String(err);
    } finally {
      savingDisjoint = false;
    }
  }

  async function removeDisjointPair(disjointIri) {
    disjointError = null;
    try {
      await invoke('widget_inspector__remove_disjoint_pair', {
        classIri: entityId,
        disjointIri,
      });
    } catch (err) {
      disjointError = String(err);
    }
  }

  async function retractDisjointSet(groupId) {
    disjointError = null;
    try {
      await invoke('widget_inspector__retract_disjoint_set', { groupId });
      removeDisjointConfirm = null;
    } catch (err) {
      disjointError = String(err);
    }
  }

  function requestRemoveDisjoint(group) {
    if (group.kind === 'all') {
      removeDisjointConfirm = group;
    } else {
      removeDisjointPair(group.members[0].iri);
    }
  }

  async function addRelatedProcess(processIri) {
    try {
      await invoke('widget_inspector__add_property_value', {
        entityId: processIri,
        propertyIri: 'foundation:hasRelatedClass',
        valueIri: entityId,
      });
      scheduleLoad();
    } catch (err) {
      console.error('Failed to add related process:', err);
    }
  }

  async function removeRelatedProcess(processIri) {
    try {
      await invoke('widget_inspector__remove_property_value', {
        entityId: processIri,
        propertyIri: 'foundation:hasRelatedClass',
        valueIri: entityId,
      });
      scheduleLoad();
    } catch (err) {
      console.error('Failed to remove related process:', err);
    }
  }

  async function addInputAutomation(automationIri) {
    try {
      await invoke('widget_inspector__add_property_value', {
        entityId: automationIri,
        propertyIri: 'foundation:inputClass',
        valueIri: entityId,
      });
      scheduleLoad();
    } catch (err) {
      console.error('Failed to add input automation:', err);
    }
  }

  async function removeInputAutomation(automationIri) {
    try {
      await invoke('widget_inspector__remove_property_value', {
        entityId: automationIri,
        propertyIri: 'foundation:inputClass',
        valueIri: entityId,
      });
      scheduleLoad();
    } catch (err) {
      console.error('Failed to remove input automation:', err);
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

  async function saveQueryConfig(propertyIri, queryConfigJson) {
    await invoke('widget_inspector__set_property_query_config', {
      classId: entityId,
      propertyIri,
      queryConfigJson,
    });
  }

  async function loadMoreBacklinks(predicate, sourceClassIri, offset) {
    const raw = await invoke('inspector__get_backlink_page', {
      entityIri: entityId,
      predicate,
      sourceClass: sourceClassIri,
      offset,
    });
    return JSON.parse(raw);
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
        widgetType: '',
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

  function isIconUrl(icon) {
    if (!icon) return false;
    return icon.startsWith('http://') || icon.startsWith('https://') ||
           icon.startsWith('data:') || icon.startsWith('file://') || icon.startsWith('/');
  }

  function getIconSrc(icon) {
    if (!icon) return null;
    if (icon.startsWith('http://') || icon.startsWith('https://') || icon.startsWith('data:')) return icon;
    if (icon.startsWith('file://')) return convertFileSrc(icon.replace(/^file:\/\//, ''));
    if (icon.startsWith('/')) return convertFileSrc(icon);
    return null;
  }

  const isAutomationWithoutInputClass = $derived(
    entityData?.types?.some(t => t.iri === 'foundation:Automation') &&
    !entityData?.properties?.some(p => p.property === 'foundation:inputClass')
  );

  async function runAutomation(automationIri, inputIri = null, label = null) {
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
    untrack(() => scheduleLoad());
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

  // The inspected entity plus every IRI it renders (property values, types) — reloading
  // when any of them changes mirrors the previous self + property-value + type watches.
  $effect(() => {
    const iris = new Set();
    if (entityId) iris.add(entityId);
    for (const p of entityData?.properties ?? []) {
      if (p.value) iris.add(p.value);
    }
    for (const t of entityData?.types ?? []) {
      if (t.iri) iris.add(t.iri);
    }
    entitySub.setIris(iris);
  });

  onDestroy(() => {
    entitySub.destroy();
  });
</script>

<div class="inspector-wrapper">
  <WidgetContainer
    icon={entityData?.icon && !isIconUrl(entityData.icon) ? entityData.icon : 'info'}
    iconSrc={entityData?.icon && isIconUrl(entityData.icon) ? getIconSrc(entityData.icon) : null}
    title={entityData?.label ?? 'Inspector'}
    {windowState}
    {onWindowStateChange}
    onClose={closeWidget}
  >
    {#snippet headerSubtitle()}
      {#if entityData?.types?.length > 0}
        <div class="header-types">
          {#each entityData.types as type, idx}
            {#if idx > 0}<span class="type-separator">·</span>{/if}
            <button class="type-link" onclick={() => openEntityInspector(type.iri)}>{type.label}</button>
          {/each}
        </div>
      {/if}
    {/snippet}

    {#snippet headerActions()}
      {#each widgetDefinitions as def}
        <Button
          icon={def.icon || 'open_in_new'}
          title={def.description}
          onclick={() => openWidgetForEntity(def.widget_type)}
        />
      {/each}
      {#if entityData}
        <Button
          icon={isLocked ? 'lock' : 'lock_open'}
          title={isLocked ? 'Unlock entity' : 'Lock entity'}
          disabled={togglingLock}
          onclick={toggleSystemLock}
        />
      {/if}
      {#if entityData && !entityData.isClass && !isLocked}
        <Button
          variant="danger"
          icon="delete_forever"
          title="Delete"
          onclick={initiateDelete}
        />
      {/if}
      <Button icon="content_copy" title="Copy IRI" onclick={copyEntityIri} />
      {#if entityData?.status || (entityData?.allowedStatuses?.length > 0 && !entityData?.isClass && !isLocked)}
        <div class="status-badge-wrapper" bind:this={statusBadgeWrapperEl}>
          <button
            class="status-badge"
            class:clickable={entityData.allowedStatuses?.length > 0 && !isLocked}
            style="--status-color: {entityData.status?.color || 'var(--color-neutral)'}"
            title={isLocked ? 'Entity is system-locked' : (entityData.status?.iri ?? 'Definir status')}
            onclick={() => {
              if (entityData.allowedStatuses?.length > 0 && !isLocked) showStatusPicker = !showStatusPicker;
            }}
          >
            {#if entityData.status}
              <span class="material-symbols-outlined status-badge-icon">{entityData.status.icon || 'radio_button_checked'}</span>
              <span class="status-badge-label">{entityData.status.label}</span>
            {:else}
              <span class="material-symbols-outlined status-badge-icon">radio_button_unchecked</span>
              <span class="status-badge-label">Status</span>
            {/if}
            {#if entityData.allowedStatuses?.length > 0}
              <span class="material-symbols-outlined status-badge-chevron">expand_more</span>
            {/if}
          </button>
          {#if showStatusPicker}
            <div class="status-picker" role="listbox">
              {#each entityData.allowedStatuses as s}
                <button
                  class="status-picker-item"
                  class:active={s.iri === entityData.status?.iri}
                  style="--status-color: {s.color || 'var(--color-neutral)'}"
                  role="option"
                  aria-selected={s.iri === entityData.status?.iri}
                  onclick={() => updateStatus(s.iri)}
                >
                  <span class="material-symbols-outlined status-badge-icon">{s.icon || 'radio_button_checked'}</span>
                  <span class="status-badge-label">{s.label}</span>
                </button>
              {/each}
            </div>
          {/if}
        </div>
      {/if}
    {/snippet}

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
          {openEntityInspector}
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
          bind:showAddDisjointForm
          {savingDisjoint}
          {disjointError}
          {removeDisjointConfirm}
          onAddDisjoint={addDisjoint}
          onRequestRemoveDisjoint={requestRemoveDisjoint}
          onConfirmRemoveDisjoint={() => retractDisjointSet(removeDisjointConfirm?.groupId)}
          onCancelRemoveDisjoint={() => removeDisjointConfirm = null}
          onClearDisjointError={() => disjointError = null}
          onAddRelatedProcess={isLocked ? null : addRelatedProcess}
          onRemoveRelatedProcess={isLocked ? null : removeRelatedProcess}
          onAddInputAutomation={isLocked ? null : addInputAutomation}
          onRemoveInputAutomation={isLocked ? null : removeInputAutomation}
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
          onClearProperty={entityData.isClass || isLocked ? null : clearProperty}
          onRemoveProperty={entityData.isClass && !isLocked ? initiateRemoveProperty : null}
          onSaveCardinality={entityData.isClass && !isLocked ? saveCardinality : null}
          onSaveQueryConfig={entityData.isClass && !isLocked ? saveQueryConfig : null}
          onLoadMoreBacklinks={entityData.isClass ? null : loadMoreBacklinks}
        />
      </div>

      {#if !entityData.isClass && (isAutomationWithoutInputClass || applicableAutomations.length > 0)}
        <div class="actions-bar">
          {#if isAutomationWithoutInputClass}
            {@const isRunning = runningAutomationIri === entityId}
            <button
              class="action-bar-btn"
              onclick={() => runAutomation(entityId, null, entityData?.label)}
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
              onclick={() => runAutomation(auto.iri, entityId, auto.label)}
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
  </WidgetContainer>

  {#if deleteSuccess}
    <div class="delete-toast">
      <span class="material-symbols-outlined">check_circle</span>
      "{entityData?.label}" deleted
    </div>
  {/if}
</div>

<style>
  .inspector-wrapper {
    width: 100%;
    height: 100%;
    position: relative;
  }

  .header-types {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
  }

  .type-link {
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    font-family: var(--font-body);
    font-size: 11px;
    color: var(--color-neutral-active);
    opacity: 0.7;
  }

  .type-separator {
    color: var(--color-neutral);
    opacity: 0.4;
    font-size: 11px;
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
    border: none;
    cursor: default;
  }

  .status-badge.clickable {
    cursor: pointer;
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
    right: 0;
    z-index: 1000;
    background: color-mix(in srgb, var(--color-black) 90%, transparent);
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
    cursor: pointer;
    background: transparent;
    border: none;
    width: 100%;
    text-align: left;
  }

  .status-picker-item.active {
    background: color-mix(in srgb, var(--status-color) 30%, transparent);
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
    flex-direction: row;
    gap: 6px;
    padding: 0px;
    background: color-mix(in srgb, var(--color-black) 60%, transparent);
    flex-wrap: 1;
  }

  .action-bar-btn {
    gap: 6px;
    padding: 8px 12px;
    background: color-mix(in srgb, var(--color-interactive) 12%, transparent);
    border: none;
    color: var(--color-interactive);
    font-family: var(--font-body);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.15s;
    text-overflow: ellipsis;
    white-space: nowrap;
    flex-shrink: 0;
    overflow: hidden;
  }

  .action-bar-btn .material-symbols-outlined {
    font-size: 16px;
    flex-shrink: 0;
  }

  .action-bar-btn:hover {
    background: color-mix(in srgb, var(--color-interactive) 22%, transparent);
    color: var(--color-neutral-active);
  }

  .action-bar-btn:disabled {
    opacity: 0.6;
    cursor: default;
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
