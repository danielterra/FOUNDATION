<script>
  import { openUrl, openPath } from '@tauri-apps/plugin-opener';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import MarkdownValue from './MarkdownValue.svelte';

  let { properties, requiredFields = [], isClass = false, openEntityInspector, onSave } = $props();

  let editingKey = $state(null);
  let draftValue = $state('');
  let saving = $state(false);

  function editKey(propertyIri, valueIdx) {
    return `${propertyIri}::${valueIdx}`;
  }

  function startEdit(propertyIri, currentValue, valueIdx) {
    editingKey = editKey(propertyIri, valueIdx);
    draftValue = currentValue ?? '';
  }

  function cancelEdit() {
    editingKey = null;
    draftValue = '';
  }

  async function saveEdit(propertyIri) {
    if (!onSave || saving) return;
    saving = true;
    try {
      await onSave(propertyIri, draftValue);
    } finally {
      saving = false;
      editingKey = null;
      draftValue = '';
    }
  }

  function handleBlur(propertyIri) {
    saveEdit(propertyIri);
  }

  function handleKeydown(event, propertyIri) {
    if (event.key === 'Escape') {
      cancelEdit();
    } else if (event.key === 'Enter' && (event.ctrlKey || event.metaKey)) {
      saveEdit(propertyIri);
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

  let optionalCollapsed = $state(true);
  let emptyCollapsed = $state(true);

  const groupedDetails = $derived(
    (properties ?? []).reduce((acc, prop) => {
      if (!acc[prop.property]) {
        acc[prop.property] = {
          property: prop.property,
          propertyLabel: prop.propertyLabel,
          propertyComment: prop.propertyComment,
          isObjectProperty: prop.isObjectProperty,
          sourceClassLabel: prop.sourceClassLabel,
          datatype: prop.datatype,
          rangeClassIri: prop.rangeClassIri,
          rangeClassLabel: prop.rangeClassLabel,
          rangeClassIcon: prop.rangeClassIcon,
          isCalculated: prop.isCalculated ?? false,
          isEmpty: prop.isEmpty ?? false,
          values: []
        };
      }
      if (!prop.isEmpty) {
        acc[prop.property].values.push({
          value: prop.value,
          valueLabel: prop.valueLabel,
          valueIcon: prop.valueIcon,
          unitLabel: prop.unitLabel,
          datatype: prop.datatype,
          valueStatus: prop.valueStatus,
          formulaError: prop.formulaError ?? null,
          fileInfo: prop.fileInfo ?? null
        });
      }
      return acc;
    }, {})
  );

  const sections = $derived.by(() => {
    const all = Object.values(groupedDetails);

    function groupBySource(items) {
      const buckets = new Map();
      for (const item of items) {
        const key = item.sourceClassLabel ?? null;
        if (!buckets.has(key)) buckets.set(key, []);
        buckets.get(key).push(item);
      }
      const result = [];
      if (buckets.has(null)) result.push({ sourceClassLabel: null, items: buckets.get(null) });
      for (const [key, items] of buckets) {
        if (key !== null) result.push({ sourceClassLabel: key, items });
      }
      return result;
    }

    if (isClass) {
      const required = all.filter(g => requiredFields.includes(g.property));
      const optional = all.filter(g => !requiredFields.includes(g.property));
      return {
        mode: 'class',
        required: groupBySource(required),
        optional: groupBySource(optional),
        requiredCount: required.length,
        optionalCount: optional.length,
      };
    } else {
      const filled = all.filter(g => !g.isEmpty);
      const empty = all.filter(g => g.isEmpty);
      return {
        mode: 'thing',
        filled: groupBySource(filled),
        empty: groupBySource(empty),
        filledCount: filled.length,
        emptyCount: empty.length,
      };
    }
  });

  function isUrl(datatype) {
    return datatype === 'xsd:anyURI';
  }

  function isStringType(datatype) {
    return !datatype || datatype === 'xsd:string' || datatype === 'rdf:langString';
  }

  async function openUrl_(url) {
    try {
      await openUrl(url);
    } catch (err) {
      console.error('Failed to open URL:', err);
    }
  }

  function formatDate(timestamp) {
    const ts = typeof timestamp === 'string' ? parseInt(timestamp) : timestamp;
    const date = new Date(ts);

    if (isNaN(date.getTime())) return timestamp;

    const now = new Date();
    const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    const yesterday = new Date(today);
    yesterday.setDate(yesterday.getDate() - 1);

    const dateDay = new Date(date.getFullYear(), date.getMonth(), date.getDate());
    const diffMs = now - date;
    const diffMins = Math.floor(diffMs / 60000);
    const diffHours = Math.floor(diffMs / 3600000);
    const diffDays = Math.floor(diffMs / 86400000);

    if (diffMins < 1) return 'just now';
    if (diffMins < 60) return `${diffMins} ${diffMins === 1 ? 'minute' : 'minutes'} ago`;

    if (dateDay.getTime() === today.getTime()) {
      if (diffHours < 2) return '1 hour ago';
      return `${diffHours} hours ago`;
    }

    if (dateDay.getTime() === yesterday.getTime()) {
      const timeStr = date.toLocaleTimeString(
        'en-US', { hour: 'numeric', minute: '2-digit', hour12: true });
      return `Yesterday at ${timeStr}`;
    }

    if (diffDays < 7) {
      const dayName = date.toLocaleDateString('en-US', { weekday: 'long' });
      const timeStr = date.toLocaleTimeString(
        'en-US', { hour: 'numeric', minute: '2-digit', hour12: true });
      return `${dayName} at ${timeStr}`;
    }

    if (date.getFullYear() === now.getFullYear()) {
      return date.toLocaleDateString('en-US', {
        month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit', hour12: true
      });
    }

    return date.toLocaleDateString('en-US', {
      year: 'numeric', month: 'short', day: 'numeric',
      hour: 'numeric', minute: '2-digit', hour12: true
    });
  }

  function formatDatatype(datatype) {
    if (!datatype || datatype === 'xsd:string' || datatype === 'rdf:langString') return 'String';
    const parts = datatype.split(':');
    const typeName = parts.length > 1 ? parts[1] : datatype;
    return typeName.charAt(0).toUpperCase() + typeName.slice(1);
  }

  function isIconUrl(icon) {
    if (!icon) return false;
    return icon.startsWith('http://') || icon.startsWith('https://') ||
           icon.startsWith('data:') || icon.startsWith('file://') || icon.startsWith('/');
  }

  function getIconUrl(icon) {
    if (!icon) return '';
    if (icon.startsWith('http://') || icon.startsWith('https://') ||
        icon.startsWith('data:')) return icon;
    if (icon.startsWith('file://')) return convertFileSrc(icon.replace(/^file:\/\//, ''));
    if (icon.startsWith('/')) return convertFileSrc(icon);
    return icon;
  }

  const FILE_TYPE_MIME = {
    'foundation:FileType_PDF':  'application/pdf',
    'foundation:FileType_PNG':  'image/png',
    'foundation:FileType_JPEG': 'image/jpeg',
    'foundation:FileType_JPG':  'image/jpeg',
    'foundation:FileType_GIF':  'image/gif',
    'foundation:FileType_WEBP': 'image/webp',
    'foundation:FileType_BMP':  'image/bmp',
    'foundation:FileType_SVG':  'image/svg+xml',
  };

  function getFileMimeType(fileInfo) {
    return fileInfo?.fileTypeIri ? (FILE_TYPE_MIME[fileInfo.fileTypeIri] ?? null) : null;
  }

  function getFileDisplayPath(filePath) {
    if (!filePath) return null;
    return filePath.startsWith('file://') ? filePath.replace('file://', '') : filePath;
  }

  function formatFileSize(bytes) {
    if (!bytes) return null;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(1024));
    return `${(bytes / Math.pow(1024, i)).toFixed(1)} ${sizes[i]}`;
  }

  async function openFileFromInfo(fileInfo) {
    const cleanPath = getFileDisplayPath(fileInfo?.filePath);
    if (!cleanPath) return;
    try { await openPath(cleanPath); } catch (err) { console.error('Failed to open file:', err); }
  }
</script>

{#snippet editForm(propertyIri)}
  <div class="edit-container">
    <textarea
      class="edit-textarea"
      bind:value={draftValue}
      onblur={() => handleBlur(propertyIri)}
      onkeydown={(e) => handleKeydown(e, propertyIri)}
      autofocus
      rows="5"
    ></textarea>
    <div class="edit-actions">
      <button
        class="edit-save-btn"
        onmousedown={(e) => e.preventDefault()}
        onclick={() => saveEdit(propertyIri)}
        disabled={saving}
      >
        {#if saving}
          <span class="material-symbols-outlined spinning-small">progress_activity</span>
        {:else}
          <span class="material-symbols-outlined">check</span>
        {/if}
        Save
      </button>
      <button
        class="edit-cancel-btn"
        onmousedown={(e) => e.preventDefault()}
        onclick={cancelEdit}
      >
        <span class="material-symbols-outlined">close</span>
        Cancel
      </button>
    </div>
  </div>
{/snippet}

{#snippet detailItem(detailGroup)}
  <div class="detail-item" transition:slide={{ duration: 300, easing: cubicOut }}>
    <div class="detail-header">
      <div class="detail-name">
        {detailGroup.propertyLabel}
        {#if detailGroup.isObjectProperty}
          <span class="detail-type detail-type-object">
            {#if detailGroup.rangeClassIcon}
              <span class="material-symbols-outlined detail-type-icon">{detailGroup.rangeClassIcon}</span>
            {/if}
            {detailGroup.rangeClassLabel ?? 'Object'}
          </span>
        {:else}
          <span class="detail-type">{formatDatatype(detailGroup.datatype)}</span>
        {/if}
        {#if detailGroup.isCalculated}
          <span class="calculated-badge" title="Calculated field">ƒ</span>
        {/if}
        {#if detailGroup.values.length > 1}
          <span class="detail-count">{detailGroup.values.length}</span>
        {/if}
      </div>
      {#if onSave && !detailGroup.isObjectProperty && !detailGroup.isCalculated && isStringType(detailGroup.datatype) && (detailGroup.isEmpty || detailGroup.values.length <= 1)}
        <button
          class="edit-btn"
          title="Edit"
          onclick={() => startEdit(detailGroup.property, detailGroup.values[0]?.value ?? '', 0)}
        >
          <span class="material-symbols-outlined">edit</span>
        </button>
      {/if}
    </div>

    {#if detailGroup.propertyComment}
      <div class="detail-comment">{detailGroup.propertyComment}</div>
    {/if}

    {#if detailGroup.isEmpty && editingKey !== editKey(detailGroup.property, 0)}
      <div class="empty-value">—</div>
    {:else if detailGroup.isEmpty && editingKey === editKey(detailGroup.property, 0)}
      <div class="detail-value">
        {@render editForm(detailGroup.property)}
      </div>
    {:else if detailGroup.rangeClassIri === 'foundation:File'}
      <div class="file-grid">
        {#each detailGroup.values as val, idx (detailGroup.property + '_' + val.value + '_' + idx)}
          {@const mimeType = getFileMimeType(val.fileInfo)}
          {@const cleanPath = getFileDisplayPath(val.fileInfo?.filePath)}
          {@const fileName = val.fileInfo?.fileName || val.valueLabel || val.value}
          {@const fileSize = val.fileInfo?.fileSize}
          <button
            class="file-grid-card"
            onclick={() => val.fileInfo ? openFileFromInfo(val.fileInfo) : openEntityInspector(val.value)}
            title={fileName}
          >
            <div class="file-grid-preview">
              {#if mimeType?.startsWith('image/') && cleanPath}
                <img src={convertFileSrc(cleanPath)} alt={fileName} class="file-grid-img" />
              {:else if mimeType === 'application/pdf'}
                <span class="material-symbols-outlined file-grid-icon">picture_as_pdf</span>
              {:else}
                <span class="material-symbols-outlined file-grid-icon">insert_drive_file</span>
              {/if}
            </div>
            <div class="file-grid-info">
              <span class="file-grid-name">{fileName}</span>
              {#if fileSize}
                <span class="file-grid-size">{formatFileSize(fileSize)}</span>
              {/if}
            </div>
          </button>
        {/each}
      </div>
    {:else}
      <div class="detail-values-group">
        {#each detailGroup.values as val, idx (detailGroup.property + '_' + val.value + '_' + idx)}
          {#if detailGroup.isObjectProperty}
            <div
              class="detail-value clickable"
              class:calculated={detailGroup.isCalculated}
              role="button"
              tabindex="0"
              onclick={() => openEntityInspector(val.value)}
              onkeydown={(e) => e.key === 'Enter' && openEntityInspector(val.value)}
            >
              {#if val.valueIcon}
                {#if isIconUrl(val.valueIcon)}
                  <img src={getIconUrl(val.valueIcon)} alt="" class="value-icon-image" />
                {:else}
                  <span class="material-symbols-outlined value-icon">{val.valueIcon}</span>
                {/if}
              {/if}
              {#if val.unitLabel}
                <span class="unit">{val.unitLabel}</span>
              {/if}
              <span class="value-text">{val.valueLabel || val.value}</span>
              {#if val.valueStatus}
                <span
                  class="inline-status"
                  style="--status-color: {val.valueStatus.color || 'var(--color-neutral)'}"
                  title={val.valueStatus.iri}
                >
                  <span class="material-symbols-outlined inline-status-icon">radio_button_checked</span>
                  <span class="inline-status-label">{val.valueStatus.label}</span>
                </span>
              {/if}
              {#if val.formulaError}
                <span class="formula-error" title={val.formulaError}>
                  <span class="material-symbols-outlined formula-error-icon">warning</span>
                  <span class="formula-error-text">{val.formulaError}</span>
                </span>
              {/if}
            </div>
          {:else}
            <div class="detail-value" class:calculated={detailGroup.isCalculated}>
              {#if val.datatype === 'xsd:dateTime'}
                {@const date = new Date(val.value)}
                <div class="timestamp-display">
                  <span class="value-text">
                    {date.toLocaleString('en-US', {
                      year: 'numeric', month: 'short', day: 'numeric',
                      hour: 'numeric', minute: '2-digit', second: '2-digit', hour12: true
                    })}
                  </span>
                  <span class="timestamp-relative">{formatDate(date.getTime())}</span>
                </div>
              {:else if val.datatype === 'xsd:date'}
                {@const [y, m, d] = val.value.split('-').map(Number)}
                {@const date = new Date(y, m - 1, d)}
                <span class="value-text">
                  {date.toLocaleDateString('en-US', { year: 'numeric', month: 'short', day: 'numeric' })}
                </span>
              {:else if isUrl(val.datatype)}
                <button class="url-value" onclick={() => openUrl_(val.value)} title={val.value}>
                  <span class="value-text">{val.valueLabel || val.value}</span>
                  <span class="material-symbols-outlined url-open-icon">open_in_new</span>
                </button>
              {:else if isStringType(val.datatype)}
                {#if editingKey === editKey(detailGroup.property, idx)}
                  {@render editForm(detailGroup.property)}
                {:else if (val.value ?? '').length > 50_000}
                  <div class="value-large">
                    <pre class="value-pre">{val.value}</pre>
                    <button class="copy-btn" onclick={() => navigator.clipboard.writeText(val.value)} title="Copy value">
                      <span class="material-symbols-outlined">content_copy</span>
                    </button>
                  </div>
                {:else}
                  <MarkdownValue value={val.value} />
                {/if}
              {:else}
                {#if val.unitLabel}
                  <span class="unit">{val.unitLabel}</span>
                {/if}
                <span class="value-text">{val.valueLabel || val.value}</span>
              {/if}
              {#if val.valueStatus}
                <span
                  class="inline-status"
                  style="--status-color: {val.valueStatus.color || 'var(--color-neutral)'}"
                  title={val.valueStatus.iri}
                >
                  <span class="material-symbols-outlined inline-status-icon">radio_button_checked</span>
                  <span class="inline-status-label">{val.valueStatus.label}</span>
                </span>
              {/if}
              {#if val.formulaError}
                <span class="formula-error" title={val.formulaError}>
                  <span class="material-symbols-outlined formula-error-icon">warning</span>
                  <span class="formula-error-text">{val.formulaError}</span>
                </span>
              {/if}
            </div>
          {/if}
        {/each}
      </div>
    {/if}
  </div>
{/snippet}

{#snippet sourceGroups(groups, sepTop = 0)}
  {#each groups as sourceGroup}
    {#if sourceGroup.sourceClassLabel}
      <div class="source-separator" use:sticky={{ top: sepTop }}>{sourceGroup.sourceClassLabel}</div>
    {/if}
    {#each sourceGroup.items as detailGroup (detailGroup.property)}
      {@render detailItem(detailGroup)}
    {/each}
  {/each}
{/snippet}

{#if properties?.length > 0}
  <div class="details-list">

    {#if sections.mode === 'class'}

      {#if sections.requiredCount > 0}
        <div class="section">
          <div class="section-header" use:sticky={{ top: 0 }}>
            <span class="section-title">Required</span>
            <span class="section-count">{sections.requiredCount}</span>
          </div>
          <div class="section-body">
            {@render sourceGroups(sections.required, 28)}
          </div>
        </div>
      {/if}

      {#if sections.optionalCount > 0}
        <div class="section">
          <button
            class="section-header collapsible"
            use:sticky={{ top: 0 }}
            onclick={() => optionalCollapsed = !optionalCollapsed}
          >
            <span class="material-symbols-outlined chevron" class:expanded={!optionalCollapsed}>
              chevron_right
            </span>
            <span class="section-title">Optional</span>
            <span class="section-count">{sections.optionalCount}</span>
          </button>
          {#if !optionalCollapsed}
            <div class="section-body" transition:slide={{ duration: 300, easing: cubicOut }}>
              {@render sourceGroups(sections.optional, 28)}
            </div>
          {/if}
        </div>
      {/if}

    {:else}

      {#if sections.filledCount > 0}
        <div class="section-body">
          {@render sourceGroups(sections.filled, 0)}
        </div>
      {/if}

      {#if sections.emptyCount > 0}
        <div class="section">
          <button
            class="section-header collapsible"
            use:sticky={{ top: 0 }}
            onclick={() => emptyCollapsed = !emptyCollapsed}
          >
            <span class="material-symbols-outlined chevron" class:expanded={!emptyCollapsed}>
              chevron_right
            </span>
            <span class="section-title">Empty fields</span>
            <span class="section-count">{sections.emptyCount}</span>
          </button>
          {#if !emptyCollapsed}
            <div class="section-body" transition:slide={{ duration: 300, easing: cubicOut }}>
              {@render sourceGroups(sections.empty, 28)}
            </div>
          {/if}
        </div>
      {/if}

    {/if}

  </div>
{/if}

<style>
  .details-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 16px;
  }

  .section {
    display: flex;
    flex-direction: column;
  }

  .section-header {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 2px;
    margin-bottom: 6px;
    z-index: 2;
    background: color-mix(in srgb, var(--color-black) 97%, transparent);
    backdrop-filter: blur(12px);
  }

  .section-header.collapsible {
    background: color-mix(in srgb, var(--color-black) 97%, transparent);
    border: none;
    cursor: pointer;
    width: 100%;
    text-align: left;
    border-radius: 4px;
    transition: background 0.15s;
  }

  .section-header.collapsible:hover {
    background: color-mix(in srgb, var(--color-black) 92%, var(--color-white) 4%);
  }

  .chevron {
    font-size: 16px;
    color: var(--color-neutral);
    opacity: 0.6;
    transition: transform 0.2s;
    flex-shrink: 0;
  }

  .chevron.expanded {
    transform: rotate(90deg);
  }

  .section-title {
    font-family: var(--font-body);
    font-size: 10px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.6px;
    color: color-mix(in srgb, var(--color-neutral) 60%, transparent);
    flex: 1;
  }

  .section-count {
    font-size: 10px;
    font-weight: 600;
    color: color-mix(in srgb, var(--color-neutral) 70%, transparent);
    padding: 1px 6px;
    background: color-mix(in srgb, var(--color-neutral) 15%, transparent);
    border-radius: 10px;
  }

  .section-body {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .source-separator {
    font-family: var(--font-body);
    font-size: 10px;
    font-weight: 600;
    color: color-mix(in srgb, var(--color-neutral) 40%, transparent);
    padding: 6px 2px 2px;
    border-top: 1px solid color-mix(in srgb, var(--color-neutral) 12%, transparent);
    margin-top: 4px;
    z-index: 1;
    background: color-mix(in srgb, var(--color-black) 97%, transparent);
    backdrop-filter: blur(12px);
  }

  .source-separator:first-child {
    margin-top: 0;
    border-top: none;
    padding-top: 2px;
  }

  .detail-item {
    padding: 10px 12px;
    background: color-mix(in srgb, var(--color-white) 3%, transparent);
    border-radius: 8px;
    border-left: 3px solid color-mix(in srgb, var(--color-neutral) 30%, transparent);
  }

  .detail-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 6px;
    gap: 4px;
  }

  .detail-name {
    font-family: var(--font-title);
    font-size: 13px;
    font-weight: 600;
    color: var(--color-neutral-active);
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .detail-type {
    font-size: 10px;
    padding: 2px 6px;
    background: color-mix(in srgb, var(--color-neutral) 20%, transparent);
    color: var(--color-neutral);
    border-radius: 4px;
    font-weight: 600;
  }

  .detail-type-object {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }

  .detail-type-icon {
    font-size: 11px;
  }

  .detail-count {
    font-size: 10px;
    padding: 2px 6px;
    background: color-mix(in srgb, var(--color-accent) 20%, transparent);
    color: var(--color-accent);
    border-radius: 4px;
    font-weight: 600;
  }

  .detail-comment {
    font-size: 12px;
    color: var(--color-neutral);
    margin-bottom: 8px;
    line-height: 1.4;
  }

  .empty-value {
    font-family: var(--font-body);
    font-size: 13px;
    color: var(--color-neutral);
    opacity: 0.35;
    padding: 2px 0;
  }

  .detail-values-group {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .detail-value {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px;
    background: color-mix(in srgb, var(--color-black) 30%, transparent);
    border-radius: 6px;
  }

  .value-large {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }

  .value-pre {
    font-family: monospace;
    font-size: 11px;
    color: var(--color-neutral-active);
    background: color-mix(in srgb, var(--color-black) 40%, transparent);
    border-radius: 4px;
    padding: 8px;
    max-height: 200px;
    overflow-y: auto;
    white-space: pre-wrap;
    word-break: break-all;
    margin: 0;
  }

  .copy-btn {
    align-self: flex-end;
    background: none;
    border: none;
    cursor: pointer;
    color: var(--color-neutral);
    padding: 2px;
    display: flex;
    align-items: center;
    border-radius: 4px;
    transition: color 0.15s;
  }

  .copy-btn:hover {
    color: var(--color-neutral-active);
  }

  .copy-btn .material-symbols-outlined {
    font-size: 16px;
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

  .value-icon {
    font-size: 18px;
    color: var(--color-interactive);
  }

  .value-icon-image {
    width: 24px;
    height: 24px;
    border-radius: 4px;
    object-fit: cover;
  }

  .value-text {
    font-family: var(--font-body);
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

  .url-value {
    display: flex;
    align-items: center;
    gap: 4px;
    flex: 1;
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    color: var(--color-interactive);
    text-align: left;
  }

  .url-value:hover .value-text {
    text-decoration: underline;
  }

  .url-open-icon {
    font-size: 14px;
    opacity: 0.6;
    flex-shrink: 0;
  }

  .inline-status {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 2px 7px 2px 4px;
    background: color-mix(in srgb, var(--status-color) 18%, transparent);
    border: 1px solid color-mix(in srgb, var(--status-color) 40%, transparent);
    border-radius: 20px;
    flex-shrink: 0;
    margin-left: auto;
  }

  .inline-status-icon {
    font-size: 12px;
    color: var(--status-color);
  }

  .inline-status-label {
    font-family: var(--font-body);
    font-size: 10px;
    font-weight: 600;
    color: var(--status-color);
    white-space: nowrap;
  }

  .calculated-badge {
    font-size: 11px;
    font-weight: 700;
    font-style: italic;
    padding: 2px 6px;
    background: color-mix(in srgb, var(--color-accent) 15%, transparent);
    color: var(--color-accent);
    border-radius: 4px;
    line-height: 1;
    cursor: default;
  }

  .detail-value.calculated {
    font-style: italic;
    color: var(--color-neutral);
    background: color-mix(in srgb, var(--color-black) 20%, transparent);
    border-left: 2px solid color-mix(in srgb, var(--color-accent) 40%, transparent);
  }

  .detail-value.calculated .value-text {
    color: var(--color-neutral);
  }

  .formula-error {
    display: flex;
    align-items: flex-start;
    gap: 4px;
    padding: 4px 6px;
    background: color-mix(in srgb, var(--color-error, #ef4444) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-error, #ef4444) 30%, transparent);
    border-radius: 4px;
    width: 100%;
    margin-top: 4px;
    box-sizing: border-box;
  }

  .file-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
    gap: 8px;
    margin-top: 4px;
  }

  .file-grid-card {
    display: flex;
    flex-direction: column;
    gap: 6px;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-white) 12%, transparent);
    border-radius: 8px;
    padding: 8px;
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s, transform 0.15s;
    text-align: left;
  }

  .file-grid-card:hover {
    background: color-mix(in srgb, var(--color-white) 10%, transparent);
    border-color: color-mix(in srgb, var(--color-white) 22%, transparent);
    transform: translateY(-1px);
  }

  .file-grid-preview {
    width: 100%;
    aspect-ratio: 4 / 3;
    border-radius: 5px;
    overflow: hidden;
    background: color-mix(in srgb, var(--color-white) 3%, transparent);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .file-grid-img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .file-grid-icon {
    font-size: 36px;
    color: var(--color-neutral);
    opacity: 0.5;
  }

  .file-grid-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .file-grid-name {
    font-size: 11px;
    color: var(--color-neutral-active);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
  }

  .file-grid-size {
    font-size: 10px;
    color: var(--color-neutral);
    opacity: 0.7;
  }

  .formula-error-icon {
    font-size: 14px;
    color: var(--color-error, #ef4444);
    flex-shrink: 0;
    line-height: 1.4;
  }

  .formula-error-text {
    font-family: var(--font-body);
    font-size: 11px;
    color: var(--color-error, #ef4444);
    line-height: 1.4;
  }

  .edit-btn {
    background: none;
    border: none;
    cursor: pointer;
    color: var(--color-neutral);
    opacity: 0;
    padding: 2px;
    display: flex;
    align-items: center;
    border-radius: 4px;
    transition: color 0.15s, opacity 0.15s;
    flex-shrink: 0;
  }

  .edit-btn .material-symbols-outlined {
    font-size: 14px;
  }

  .edit-btn:hover {
    color: var(--color-neutral-active);
    background: color-mix(in srgb, var(--color-white) 8%, transparent);
  }

  .detail-item:hover .edit-btn {
    opacity: 1;
  }

  .edit-container {
    display: flex;
    flex-direction: column;
    gap: 6px;
    flex: 1;
    min-width: 0;
  }

  .edit-textarea {
    width: 100%;
    min-height: 80px;
    background: color-mix(in srgb, var(--color-black) 50%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-interactive) 50%, transparent);
    border-radius: 6px;
    color: var(--color-neutral-active);
    font-family: var(--font-body);
    font-size: 13px;
    line-height: 1.5;
    padding: 8px;
    resize: vertical;
    box-sizing: border-box;
    outline: none;
    transition: border-color 0.15s;
  }

  .edit-textarea:focus {
    border-color: var(--color-interactive);
  }

  .edit-actions {
    display: flex;
    gap: 6px;
  }

  .edit-save-btn,
  .edit-cancel-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    border: none;
    border-radius: 4px;
    cursor: pointer;
    font-family: var(--font-body);
    font-size: 12px;
    font-weight: 600;
    transition: background 0.15s;
  }

  .edit-save-btn {
    background: color-mix(in srgb, var(--color-interactive) 25%, transparent);
    color: var(--color-interactive);
  }

  .edit-save-btn:hover:not(:disabled) {
    background: color-mix(in srgb, var(--color-interactive) 40%, transparent);
  }

  .edit-save-btn:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .edit-cancel-btn {
    background: color-mix(in srgb, var(--color-neutral) 12%, transparent);
    color: var(--color-neutral);
  }

  .edit-cancel-btn:hover {
    background: color-mix(in srgb, var(--color-neutral) 22%, transparent);
  }

  .edit-save-btn .material-symbols-outlined,
  .edit-cancel-btn .material-symbols-outlined {
    font-size: 14px;
  }

  .spinning-small {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }
</style>
