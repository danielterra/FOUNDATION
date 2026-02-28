<script>
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { openPath } from '@tauri-apps/plugin-opener';

  let { entityId, widgetId } = $props();

  let entityData = $state(null);
  let loading = $state(true);
  let error = $state(null);
  let unlistenEntityUpdated = $state(null);
  let collapsedGroups = $state(new Set());

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
      console.log('Copied entity IRI:', entityData.id);
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
      console.log('Opened inspector for:', entityIri);
    } catch (err) {
      console.error('Failed to open inspector:', err);
    }
  }

  async function openFile(filePath) {
    if (!filePath) return;
    try {
      await openPath(filePath);
    } catch (err) {
      console.error('Failed to open file:', err);
    }
  }

  function formatFileSize(bytes) {
    if (!bytes) return '0 B';
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(1024));
    return `${(bytes / Math.pow(1024, i)).toFixed(2)} ${sizes[i]}`;
  }

  function isAttachment() {
    return entityData?.types?.some(t => t.iri === 'foundation:Attachment');
  }

  function getAttachmentFilePath() {
    const prop = entityData?.properties?.find(p => p.property === 'foundation:filePath');
    return prop?.value;
  }

  function getAttachmentMimeType() {
    const prop = entityData?.properties?.find(p => p.property === 'foundation:mimeType');
    return prop?.value;
  }

  function getAttachmentFileName() {
    const prop = entityData?.properties?.find(p => p.property === 'foundation:fileName');
    return prop?.value || entityData?.label;
  }

  function getAttachmentFileSize() {
    const prop = entityData?.properties?.find(p => p.property === 'foundation:fileSize');
    return prop?.value ? parseInt(prop.value) : null;
  }

  function toggleClassGroup(classIri) {
    if (collapsedGroups.has(classIri)) {
      collapsedGroups.delete(classIri);
    } else {
      collapsedGroups.add(classIri);
    }
    collapsedGroups = new Set(collapsedGroups);
  }

  function isIconUrl(icon) {
    if (!icon) return false;
    return icon.startsWith('http://') ||
           icon.startsWith('https://') ||
           icon.startsWith('data:') ||
           icon.startsWith('/');
  }

  function getIconUrl(icon) {
    if (!icon) return '';

    // If it's a URL or data URI, return as-is
    if (icon.startsWith('http://') || icon.startsWith('https://') || icon.startsWith('data:')) {
      return icon;
    }

    // If it's an absolute local path, convert using Tauri's asset protocol
    if (icon.startsWith('/')) {
      return convertFileSrc(icon);
    }

    return icon;
  }


  function formatDate(timestamp) {
    // Handle both milliseconds and seconds timestamps
    const ts = typeof timestamp === 'string' ? parseInt(timestamp) : timestamp;
    const date = new Date(ts);

    // Check if valid date
    if (isNaN(date.getTime())) {
      return timestamp; // Return original if invalid
    }

    const now = new Date();
    const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    const yesterday = new Date(today);
    yesterday.setDate(yesterday.getDate() - 1);

    const dateDay = new Date(date.getFullYear(), date.getMonth(), date.getDate());

    const diffMs = now - date;
    const diffMins = Math.floor(diffMs / 60000);
    const diffHours = Math.floor(diffMs / 3600000);
    const diffDays = Math.floor(diffMs / 86400000);

    // Just now (< 1 minute)
    if (diffMins < 1) {
      return 'just now';
    }

    // Minutes ago (< 1 hour)
    if (diffMins < 60) {
      return `${diffMins} ${diffMins === 1 ? 'minute' : 'minutes'} ago`;
    }

    // Hours ago (same day)
    if (dateDay.getTime() === today.getTime()) {
      if (diffHours < 2) {
        return '1 hour ago';
      }
      return `${diffHours} hours ago`;
    }

    // Yesterday
    if (dateDay.getTime() === yesterday.getTime()) {
      const timeStr = date.toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit', hour12: true });
      return `Yesterday at ${timeStr}`;
    }

    // Last 7 days - show day name with time
    if (diffDays < 7) {
      const dayName = date.toLocaleDateString('en-US', { weekday: 'long' });
      const timeStr = date.toLocaleTimeString('en-US', { hour: 'numeric', minute: '2-digit', hour12: true });
      return `${dayName} at ${timeStr}`;
    }

    // This year - show month, day and time
    if (date.getFullYear() === now.getFullYear()) {
      return date.toLocaleDateString('en-US', {
        month: 'short',
        day: 'numeric',
        hour: 'numeric',
        minute: '2-digit',
        hour12: true
      });
    }

    // Other years - show full date with year
    return date.toLocaleDateString('en-US', {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
      hour: 'numeric',
      minute: '2-digit',
      hour12: true
    });
  }

  function isTimestamp(value) {
    // Check if value looks like a timestamp (numeric string with 10 or 13 digits)
    if (typeof value !== 'string') return false;
    return /^\d{10,13}$/.test(value);
  }

  onMount(async () => {
    loadEntity();

    // Listen for entity-updated events
    unlistenEntityUpdated = await listen('entity-updated', (event) => {
      // Check if this is the entity we're inspecting
      if (event.payload.entityId === entityId) {
        console.log('Entity updated, reloading inspector:', entityId);
        loadEntity();
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
        <button class="action-btn" onclick={copyEntityIri} title="Copy IRI">
          <span class="material-symbols-outlined">content_copy</span>
        </button>
        <button class="close-btn" onclick={closeWidget}>
          <span class="material-symbols-outlined">close</span>
        </button>
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
        {#if entityData.comment}
          <p class="description">{entityData.comment}</p>
        {/if}

        {#if isAttachment()}
          {@const filePath = getAttachmentFilePath()}
          {@const mimeType = getAttachmentMimeType()}
          {@const fileName = getAttachmentFileName()}
          {@const fileSize = getAttachmentFileSize()}
          {console.log('[INSPECTOR] Attachment detected:', { filePath, mimeType, fileName, fileSize, types: entityData?.types })}

          <div class="attachment-preview-section">
            <button
              class="attachment-preview-card"
              onclick={() => openFile(filePath)}
              title="Click to open in default app"
            >
              {#if mimeType?.startsWith('image/')}
                <div class="attachment-image-preview">
                  <img
                    src={convertFileSrc(filePath)}
                    alt={fileName}
                  />
                </div>
              {:else if mimeType === 'application/pdf'}
                <div class="attachment-pdf-preview">
                  <embed
                    src={convertFileSrc(filePath)}
                    type="application/pdf"
                  />
                </div>
              {:else}
                <div class="attachment-file-preview">
                  <span class="material-symbols-outlined">description</span>
                  <span class="file-label">File</span>
                </div>
              {/if}

              <div class="attachment-preview-info">
                <div class="attachment-preview-name">{fileName}</div>
                {#if fileSize}
                  <div class="attachment-preview-size">{formatFileSize(fileSize)}</div>
                {/if}
                <div class="attachment-preview-action">
                  <span class="material-symbols-outlined">open_in_new</span>
                  <span>Open file</span>
                </div>
              </div>
            </button>
          </div>
        {/if}

        {#if entityData.superClasses?.length > 0}
          <div class="thing-list">
            {#each entityData.superClasses as superClass}
              <div class="thing-item clickable" onclick={() => openEntityInspector(superClass.iri)}>
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
              <div class="thing-item clickable" onclick={() => openEntityInspector(subClass.iri)}>
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

        {#if entityData.properties?.length > 0}
            {@const groupedProperties = entityData.properties.reduce((acc, prop) => {
                if (!acc[prop.property]) {
                  acc[prop.property] = {
                    property: prop.property,
                    propertyLabel: prop.propertyLabel,
                    propertyComment: prop.propertyComment,
                    isObjectProperty: prop.isObjectProperty,
                    sourceClassLabel: prop.sourceClassLabel,
                    values: []
                  };
                }
                acc[prop.property].values.push({
                  value: prop.value,
                  valueLabel: prop.valueLabel,
                  valueIcon: prop.valueIcon,
                  unitLabel: prop.unitLabel
                });
                return acc;
              }, {})}

            <div class="properties-list">
            {#each Object.values(groupedProperties) as propGroup (propGroup.property)}
              <div class="property-item">
                <div class="property-header">
                  <div class="property-name">
                    {propGroup.propertyLabel}
                    {#if propGroup.isObjectProperty}
                      <span class="property-type">Object</span>
                    {:else}
                      <span class="property-type">Data</span>
                    {/if}
                    {#if propGroup.values.length > 1}
                      <span class="property-count">{propGroup.values.length}</span>
                    {/if}
                  </div>
                  {#if propGroup.sourceClassLabel}
                    <div class="property-source">from {propGroup.sourceClassLabel}</div>
                  {/if}
                </div>

                {#if propGroup.propertyComment}
                  <div class="property-comment">{propGroup.propertyComment}</div>
                {/if}

                <div class="property-values-group">
                  {#each propGroup.values as val, idx (propGroup.property + '_' + val.value + '_' + idx)}
                    <div
                      class="property-value"
                      class:clickable={propGroup.isObjectProperty}
                      onclick={() => propGroup.isObjectProperty && openEntityInspector(val.value)}
                    >
                      {#if propGroup.isObjectProperty && val.valueIcon}
                        <span class="material-symbols-outlined value-icon">{val.valueIcon}</span>
                      {/if}
                      {#if !propGroup.isObjectProperty && isTimestamp(val.value)}
                        {@const date = new Date(parseInt(val.value))}
                        <div class="timestamp-display">
                          <span class="value-text">
                            {date.toLocaleString('en-US', {
                              year: 'numeric',
                              month: 'short',
                              day: 'numeric',
                              hour: 'numeric',
                              minute: '2-digit',
                              second: '2-digit',
                              hour12: true
                            })}
                          </span>
                          <span class="timestamp-relative">
                            {formatDate(val.value)}
                          </span>
                        </div>
                      {:else}
                        <span class="value-text">{val.valueLabel || val.value}</span>
                      {/if}
                      {#if val.unitLabel}
                        <span class="unit">{val.unitLabel}</span>
                      {/if}
                    </div>
                  {/each}
                </div>
              </div>
            {/each}
            </div>
          {/if}

        {#if entityData.backlinks?.length > 0}
            {@const groupedByClass = entityData.backlinks.reduce((acc, backlink) => {
              const className = backlink.sourceClassLabel || 'Unknown';
              const classIri = backlink.sourceClass || 'unknown';

              if (!acc[classIri]) {
                acc[classIri] = {
                  className,
                  classIri,
                  entities: {}
                };
              }

              if (!acc[classIri].entities[backlink.value]) {
                acc[classIri].entities[backlink.value] = {
                  entity: backlink.value,
                  entityLabel: backlink.valueLabel || backlink.value,
                  entityIcon: backlink.valueIcon,
                  properties: []
                };
              }

              acc[classIri].entities[backlink.value].properties.push({
                property: backlink.property,
                propertyLabel: backlink.propertyLabel,
                propertyComment: backlink.propertyComment
              });

              return acc;
            }, {})}

            <div class="backlinks-list">
              {#each Object.values(groupedByClass) as classGroup}
                {@const entityCount = Object.keys(classGroup.entities).length}
                {@const isCollapsed = collapsedGroups.has(classGroup.classIri)}
                {#if entityCount > 5 && !collapsedGroups.has(classGroup.classIri) && !collapsedGroups.has(`expanded_${classGroup.classIri}`)}
                  {collapsedGroups.add(classGroup.classIri)}
                  {collapsedGroups = new Set(collapsedGroups)}
                {/if}
                <div class="class-group">
                  <button
                    class="class-header"
                    onclick={() => toggleClassGroup(classGroup.classIri)}
                  >
                    <span class="material-symbols-outlined chevron" class:expanded={!isCollapsed}>
                      chevron_right
                    </span>
                    <span class="material-symbols-outlined class-icon">category</span>
                    <span class="class-name">{classGroup.className}</span>
                    <span class="class-count">{entityCount} {entityCount === 1 ? 'entity' : 'entities'}</span>
                  </button>

                  {#if !isCollapsed}
                  {#each Object.values(classGroup.entities) as group}
                <div class="backlink-group">
                  <div
                    class="backlink-entity clickable"
                    onclick={() => openEntityInspector(group.entity)}
                  >
                    {#if group.entityIcon}
                      <span class="material-symbols-outlined entity-icon">{group.entityIcon}</span>
                    {:else}
                      <span class="material-symbols-outlined entity-icon">link</span>
                    {/if}
                    <div class="entity-info">
                      <div class="entity-label">{group.entityLabel}</div>
                      <div class="entity-count">{group.properties.length} {group.properties.length === 1 ? 'relationship' : 'relationships'}</div>
                    </div>
                    <span class="material-symbols-outlined arrow">arrow_forward</span>
                  </div>

                  <div class="backlink-properties">
                    {#each group.properties as prop}
                      <div class="backlink-property">
                        <span class="material-symbols-outlined prop-icon">arrow_back</span>
                        <div class="prop-info">
                          <span class="prop-label">{prop.propertyLabel}</span>
                          {#if prop.propertyComment}
                            <span class="prop-comment">{prop.propertyComment}</span>
                          {/if}
                        </div>
                      </div>
                    {/each}
                  </div>
                </div>
              {/each}
              {/if}
                </div>
              {/each}
            </div>
        {/if}

        {#if entityData.instances?.length > 0}
          <div class="thing-list">
            {#each entityData.instances as instance}
              <div class="thing-item instance clickable" onclick={() => openEntityInspector(instance.iri)}>
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
    box-shadow: 0 8px 32px rgba(0, 0, 0, 0.4);
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
  }

  .widget-icon-container {
    flex-shrink: 0;
  }

  .entity-icon-symbol {
    font-size: 30px;
    color: var(--color-interactive);
    display: flex;
    align-items: center;
    justify-content: center;
    width: 60px;
    height: 60px;
  }

  .entity-icon-image {
    width: 60px;
    height: 60px;
    border-radius: 8px;
    object-fit: cover;
  }

  .widget-title-info {
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1;
    min-width: 0;
  }

  .widget-title {
    display: flex;
    align-items: center;
    font-family: var(--font-title);
    font-size: 14px;
    font-weight: 600;
    color: var(--color-neutral-active);
  }

  .close-btn {
    background: none;
    border: none;
    padding: 4px;
    cursor: pointer;
    color: var(--color-neutral);
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.2s;
  }

  .close-btn:hover {
    background: color-mix(in srgb, var(--color-white) 10%, transparent);
    color: var(--color-neutral-active);
  }

  .close-btn .material-symbols-outlined {
    font-size: 20px;
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .action-btn {
    background: none;
    border: none;
    padding: 4px;
    cursor: pointer;
    color: var(--color-neutral);
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.2s;
  }

  .action-btn:hover {
    background: color-mix(in srgb, var(--color-white) 10%, transparent);
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

  .tabs {
    display: flex;
    gap: 4px;
    padding: 8px 12px 0;
    background: color-mix(in srgb, var(--color-black) 20%, transparent);
    border-bottom: 1px solid color-mix(in srgb, var(--color-white) 10%, transparent);
  }

  .tab {
    padding: 8px 12px;
    background: none;
    border: none;
    border-radius: 6px 6px 0 0;
    font-family: var(--font-body);
    font-size: 13px;
    color: var(--color-neutral);
    cursor: pointer;
    transition: all 0.2s;
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .tab:hover {
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    color: var(--color-neutral-active);
  }

  .tab.active {
    background: color-mix(in srgb, var(--color-white) 8%, transparent);
    color: var(--color-neutral-active);
    font-weight: 600;
  }

  .badge {
    background: var(--color-interactive);
    color: var(--color-black);
    padding: 2px 6px;
    border-radius: 10px;
    font-size: 11px;
    font-weight: 600;
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

  .description {
    margin: 0 0 16px 0;
    font-size: 14px;
    line-height: 1.6;
    color: var(--color-neutral);
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

  .properties-list {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-bottom: 16px;
  }

  .property-item {
    padding: 12px;
    background: color-mix(in srgb, var(--color-white) 3%, transparent);
    border-radius: 8px;
    border-left: 3px solid color-mix(in srgb, var(--color-interactive) 50%, transparent);
  }

  .property-item.backlink {
    border-left-color: color-mix(in srgb, var(--color-accent) 50%, transparent);
  }

  .property-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 6px;
  }

  .property-name {
    font-family: var(--font-title);
    font-size: 13px;
    font-weight: 600;
    color: var(--color-neutral-active);
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .property-type {
    font-size: 10px;
    padding: 2px 6px;
    background: color-mix(in srgb, var(--color-interactive) 20%, transparent);
    color: var(--color-interactive);
    border-radius: 4px;
    font-weight: 600;
  }

  .property-count {
    font-size: 10px;
    padding: 2px 6px;
    background: color-mix(in srgb, var(--color-accent) 20%, transparent);
    color: var(--color-accent);
    border-radius: 4px;
    font-weight: 600;
  }

  .property-source {
    font-size: 11px;
    color: var(--color-neutral);
    font-family: var(--font-code);
  }

  .property-comment {
    font-size: 12px;
    color: var(--color-neutral);
    margin-bottom: 8px;
    line-height: 1.4;
  }

  .property-values-group {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .property-value {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px;
    background: color-mix(in srgb, var(--color-black) 30%, transparent);
    border-radius: 6px;
  }

  .value-icon {
    font-size: 18px;
    color: var(--color-interactive);
  }

  .value-text {
    font-family: var(--font-code);
    font-size: 13px;
    color: var(--color-neutral-active);
    flex: 1;
  }

  .unit {
    font-size: 11px;
    color: var(--color-neutral);
    padding: 2px 6px;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    border-radius: 4px;
  }

  .timestamp-display {
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 1;
  }

  .timestamp-relative {
    font-size: 11px;
    color: var(--color-neutral);
    opacity: 0.6;
    font-family: var(--font-body);
    font-style: italic;
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 12px;
    padding: 40px;
    text-align: center;
    color: var(--color-neutral);
  }

  .empty-state .material-symbols-outlined {
    font-size: 48px;
    opacity: 0.3;
  }

  /* Backlinks styles */
  .backlinks-list {
    display: flex;
    flex-direction: column;
    gap: 20px;
    margin-bottom: 16px;
  }

  .class-group {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .class-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 12px;
    background: color-mix(in srgb, var(--color-interactive) 15%, transparent);
    border-radius: 8px;
    border-left: 3px solid var(--color-interactive);
    border: none;
    width: 100%;
    cursor: pointer;
    transition: all 0.2s;
    text-align: left;
  }

  .class-header:hover {
    background: color-mix(in srgb, var(--color-interactive) 20%, transparent);
  }

  .chevron {
    font-size: 20px;
    color: var(--color-neutral);
    transition: transform 0.2s;
  }

  .chevron.expanded {
    transform: rotate(90deg);
  }

  .class-icon {
    font-size: 20px;
    color: var(--color-interactive);
  }

  .class-name {
    font-family: var(--font-title);
    font-size: 13px;
    font-weight: 700;
    color: var(--color-neutral-active);
    text-transform: uppercase;
    letter-spacing: 0.5px;
    flex: 1;
  }

  .class-count {
    font-size: 11px;
    font-weight: 600;
    color: var(--color-interactive);
    padding: 2px 8px;
    background: color-mix(in srgb, var(--color-interactive) 20%, transparent);
    border-radius: 12px;
  }

  .backlink-group {
    background: color-mix(in srgb, var(--color-white) 3%, transparent);
    border-radius: 8px;
    overflow: hidden;
    border: 1px solid color-mix(in srgb, var(--color-white) 10%, transparent);
  }

  .backlink-entity {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    border-bottom: 1px solid color-mix(in srgb, var(--color-white) 10%, transparent);
    transition: all 0.2s;
  }

  .backlink-entity .entity-icon {
    font-size: 24px;
    color: var(--color-interactive);
  }

  .entity-info {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .entity-label {
    font-family: var(--font-title);
    font-size: 14px;
    font-weight: 600;
    color: var(--color-neutral-active);
  }

  .entity-count {
    font-size: 11px;
    color: var(--color-neutral);
  }

  .backlink-entity .arrow {
    font-size: 20px;
    color: var(--color-neutral);
    opacity: 0.5;
    transition: all 0.2s;
  }

  .backlink-entity:hover .arrow {
    opacity: 1;
    transform: translateX(4px);
  }

  .backlink-properties {
    padding: 8px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .backlink-property {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px;
    background: color-mix(in srgb, var(--color-black) 20%, transparent);
    border-radius: 6px;
  }

  .backlink-property .prop-icon {
    font-size: 16px;
    color: var(--color-interactive);
    opacity: 0.6;
  }

  .prop-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
  }

  .prop-label {
    font-family: var(--font-body);
    font-size: 12px;
    font-weight: 500;
    color: var(--color-neutral-active);
  }

  .prop-comment {
    font-size: 11px;
    color: var(--color-neutral);
    line-height: 1.3;
  }

  /* Attachment Preview Styles */
  .attachment-preview-section {
    margin-bottom: 16px;
  }

  .attachment-preview-card {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 12px;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-white) 15%, transparent);
    border-radius: 8px;
    padding: 12px;
    cursor: pointer;
    transition: all 0.2s;
  }

  .attachment-preview-card:hover {
    background: color-mix(in srgb, var(--color-white) 8%, transparent);
    border-color: color-mix(in srgb, var(--color-white) 25%, transparent);
    transform: translateY(-1px);
  }

  .attachment-image-preview {
    width: 100%;
    height: 200px;
    border-radius: 6px;
    overflow: hidden;
    background: color-mix(in srgb, var(--color-white) 3%, transparent);
  }

  .attachment-image-preview img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .attachment-pdf-preview,
  .attachment-file-preview {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    min-height: 400px;
    background: color-mix(in srgb, var(--color-white) 3%, transparent);
    border-radius: 6px;
    gap: 8px;
    overflow: hidden;
  }

  .attachment-pdf-preview embed {
    width: 100%;
    height: 400px;
    border: none;
    border-radius: 6px;
  }

  .attachment-pdf-preview .material-symbols-outlined,
  .attachment-file-preview .material-symbols-outlined {
    font-size: 64px;
    color: var(--color-interactive);
  }

  .pdf-label,
  .file-label {
    font-family: var(--font-body);
    font-size: 14px;
    font-weight: 500;
    color: var(--color-neutral);
  }

  .attachment-preview-info {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .attachment-preview-name {
    font-family: var(--font-body);
    font-size: 13px;
    font-weight: 500;
    color: var(--color-neutral-active);
    word-break: break-word;
  }

  .attachment-preview-size {
    font-size: 12px;
    color: var(--color-neutral);
  }

  .attachment-preview-action {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 4px;
    font-size: 12px;
    font-weight: 500;
    color: var(--color-interactive);
  }

  .attachment-preview-action .material-symbols-outlined {
    font-size: 16px;
  }
</style>
