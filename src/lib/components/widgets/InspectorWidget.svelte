<script>
  import { onDestroy, untrack } from 'svelte';

  import { invoke } from '@tauri-apps/api/core';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { createEntitySubscription } from '$lib/realtime/subscriptions';
  import { openEntity } from '$lib/blackboard';
  import { deleteConfirm } from '$lib/stores/deleteConfirm';
  import FilePreview from './inspector/FilePreview.svelte';
  import MetaFields from './inspector/MetaFields.svelte';
  import PropertyList from './inspector/PropertyList.svelte';
  import InspectorClassView from './inspector/InspectorClassView.svelte';
  import WidgetContainer from './WidgetContainer.svelte';
  import { Button } from '$lib/components/ui/button';
  import * as Popover from '$lib/components/ui/popover';

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
  // Snapshot tx cursor: max(tx) at the last successful loadEntity call.
  let snapshotTx = 0;

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
  let statusPickerOpen = $state(false);
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
      // Fetch entity data and snapshot tx in parallel.
      const [resultStr, snapTx] = await Promise.all([
        invoke('inspector__get_entity', { entityId }),
        invoke('chat__get_conversation_snapshot_tx', { conversationId: entityId }).catch(() => 0),
      ]);
      const t1 = performance.now();
      entityData = JSON.parse(resultStr);

      // Declare the flat set: anchor + IRI-valued properties + backlink sources.
      const objectIris = (entityData?.properties ?? [])
        .filter(p => p.value && p.value.startsWith('foundation:') || (p.value ?? '').includes(':'))
        .map(p => p.value)
        .filter(Boolean);
      const backlinkIris = (entityData?.backlinks ?? []).flatMap(b => b.values?.map(v => v.value) ?? []);
      const flatSet = [entityId, ...objectIris, ...backlinkIris].filter(Boolean);
      snapshotTx = /** @type {number} */ (snapTx);
      entitySub.setIris(flatSet);
      entitySub.setSinceTx(snapshotTx);
      entitySub.replayMissed();

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
    statusPickerOpen = false;
    try {
      await invoke('widget_inspector__update_status', { entityId, statusIri });
    } catch (err) {
      console.error('Failed to update status:', err);
    }
  }

  async function openEntityInspector(entityIri) {
    await openEntity(entityIri, conversationIri).catch(err => console.error('Failed to open entity:', err));
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

  // When entityId changes (widget navigates to another entity), reset the subscription.
  // loadEntity() declares the full flat set; this effect only seeds the anchor before
  // the first load so updates that arrive during the load aren't dropped.
  $effect(() => {
    if (entityId) entitySub.setIris([entityId]);
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
    canOpenInspector={false}
    canCopyId={false}
  >
    {#snippet headerSubtitle()}
      {#if entityData?.types?.length > 0}
        <div class="header-types">
          {#each entityData.types as type, idx}
            {#if idx > 0}<span class="type-separator">·</span>{/if}
            <Button variant="ghost" class="type-link" onclick={() => openEntityInspector(type.iri)}>{type.label}</Button>
          {/each}
        </div>
      {/if}
    {/snippet}

    {#snippet headerActions()}
      {#each widgetDefinitions as def}
        <Button variant="ghost" size="icon"
          title={def.description}
          onclick={() => openWidgetForEntity(def.widget_type)}
        ><span class="material-symbols-outlined">{def.icon || 'open_in_new'}</span></Button>
      {/each}
      {#if entityData}
        <Button variant="ghost" size="icon"
          title={isLocked ? 'Unlock entity' : 'Lock entity'}
          disabled={togglingLock}
          onclick={toggleSystemLock}
        ><span class="material-symbols-outlined">{isLocked ? 'lock' : 'lock_open'}</span></Button>
      {/if}
      {#if entityData && !entityData.isClass && !isLocked}
        <Button variant="destructive" size="icon"
          title="Delete"
          onclick={initiateDelete}
        ><span class="material-symbols-outlined">delete_forever</span></Button>
      {/if}
      <Button variant="ghost" size="icon" title="Copy IRI" onclick={copyEntityIri}><span class="material-symbols-outlined">content_copy</span></Button>
      {#if entityData?.status || (entityData?.allowedStatuses?.length > 0 && !entityData?.isClass && !isLocked)}
        <Popover.Root
          bind:open={statusPickerOpen}
          onOpenChange={(v) => { if (isLocked || !(entityData.allowedStatuses?.length > 0)) statusPickerOpen = false; }}
        >
          <Popover.Trigger
            disabled={isLocked || !(entityData.allowedStatuses?.length > 0)}
          >
            {#snippet child({ props })}
              <button
                {...props}
                class="status-badge"
                class:clickable={entityData.allowedStatuses?.length > 0 && !isLocked}
                style="--status-color: {entityData.status?.color || 'var(--color-neutral)'}"
                title={isLocked ? 'Entity is system-locked' : (entityData.status?.iri ?? 'Definir status')}
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
            {/snippet}
          </Popover.Trigger>
          <Popover.Content align="end" class="status-picker-content">
            <div role="listbox" class="status-picker-list">
              {#each entityData.allowedStatuses as s}
                <Button
                  variant="ghost"
                  class={`status-picker-item${s.iri === entityData.status?.iri ? ' active' : ''}`}
                  style="--status-color: {s.color || 'var(--color-neutral)'}"
                  role="option"
                  aria-selected={s.iri === entityData.status?.iri}
                  onclick={() => updateStatus(s.iri)}
                >
                  <span class="material-symbols-outlined status-badge-icon">{s.icon || 'radio_button_checked'}</span>
                  <span class="status-badge-label">{s.label}</span>
                </Button>
              {/each}
            </div>
          </Popover.Content>
        </Popover.Root>
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
          {entityId}
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
            <Button
              variant="ghost"
              class="action-bar-btn"
              onclick={() => runAutomation(entityId, null, entityData?.label)}
              disabled={isRunning}
            >
              <span class="material-symbols-outlined" class:spinning={isRunning}>
                {isRunning ? 'progress_activity' : 'play_circle'}
              </span>
              Run
            </Button>
          {/if}
          {#each applicableAutomations as auto}
            {@const isAutoRunning = runningAutomationIri === auto.iri}
            <Button
              variant="ghost"
              class="action-bar-btn"
              onclick={() => runAutomation(auto.iri, entityId, auto.label)}
              disabled={isAutoRunning}
              title={auto.label}
            >
              <span class="material-symbols-outlined" class:spinning={isAutoRunning}>
                {isAutoRunning ? 'progress_activity' : 'play_circle'}
              </span>
              {auto.label}
            </Button>
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

  :global([data-slot="button"].type-link) {
    padding: 0;
    height: auto;
    font-family: var(--font-body);
    font-size: 11px;
    color: var(--color-neutral-active);
    opacity: 0.7;
  }

  :global([data-slot="button"].type-link:hover) {
    background: transparent;
    opacity: 1;
    color: var(--color-neutral-active);
  }

  .type-separator {
    color: var(--color-neutral);
    opacity: 0.4;
    font-size: 11px;
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

  :global([data-slot="button"].status-picker-item) {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 8px;
    height: auto;
    width: 100%;
    justify-content: flex-start;
    border-radius: var(--radius);
  }

  :global([data-slot="button"].status-picker-item.active) {
    background: color-mix(in srgb, var(--status-color) 30%, transparent);
  }

  :global([data-slot="button"].status-picker-item:hover) {
    background: color-mix(in srgb, var(--status-color) 15%, transparent);
    color: inherit;
  }

  :global(.status-picker-content) {
    padding: 4px;
    min-width: 160px;
  }

  :global(.status-picker-list) {
    display: flex;
    flex-direction: column;
    gap: 2px;
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
    background: color-mix(in srgb, var(--color-warning) 10%, transparent);
  }

  .locked-banner-icon {
    font-size: 18px;
    color: var(--color-warning);
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
    color: var(--color-warning);
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

  :global([data-slot="button"].action-bar-btn) {
    gap: 6px;
    padding: 8px 12px;
    background: color-mix(in srgb, var(--color-interactive) 12%, transparent);
    color: var(--color-interactive);
    font-family: var(--font-body);
    font-size: 12px;
    font-weight: 600;
    height: auto;
    flex-shrink: 0;
    border-radius: 0;
    overflow: hidden;
    white-space: nowrap;
  }

  :global([data-slot="button"].action-bar-btn .material-symbols-outlined) {
    font-size: 16px;
    flex-shrink: 0;
  }

  :global([data-slot="button"].action-bar-btn:hover) {
    background: color-mix(in srgb, var(--color-interactive) 22%, transparent);
    color: var(--color-neutral-active);
  }

  :global([data-slot="button"].action-bar-btn:disabled) {
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
    background: color-mix(in srgb, var(--color-success) 20%, var(--color-black));
    font-family: var(--font-body);
    font-size: 12px;
    font-weight: 600;
    color: var(--color-success);
    white-space: nowrap;
    z-index: 101;
    pointer-events: none;
  }

  .delete-toast .material-symbols-outlined {
    font-size: 16px;
  }

</style>
