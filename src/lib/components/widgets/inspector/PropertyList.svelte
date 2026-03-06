<script>
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { slide } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  import MarkdownValue from './MarkdownValue.svelte';

  let { properties, requiredFields = [], openEntityInspector } = $props();

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
          values: []
        };
      }
      acc[prop.property].values.push({
        value: prop.value,
        valueLabel: prop.valueLabel,
        valueIcon: prop.valueIcon,
        unitLabel: prop.unitLabel,
        datatype: prop.datatype,
        valueStatus: prop.valueStatus
      });
      return acc;
    }, {})
  );

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
    if (!datatype) return 'Data';
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
</script>

{#if properties?.length > 0}
  <div class="details-list">
    {#each Object.values(groupedDetails) as detailGroup (detailGroup.property)}
      <div class="detail-item" transition:slide={{ duration: 400, easing: cubicOut }}>
        <div class="detail-header">
          <div class="detail-name">
            {detailGroup.propertyLabel}
            {#if detailGroup.isObjectProperty}
              <span class="detail-type">Object</span>
            {:else}
              <span class="detail-type">{formatDatatype(detailGroup.datatype)}</span>
            {/if}
            {#if detailGroup.values.length > 1}
              <span class="detail-count">{detailGroup.values.length}</span>
            {/if}
            {#if requiredFields.includes(detailGroup.property)}
              <span class="detail-required" title="Required field">*</span>
            {/if}
          </div>
          {#if detailGroup.sourceClassLabel}
            <div class="detail-source">from {detailGroup.sourceClassLabel}</div>
          {/if}
        </div>

        {#if detailGroup.propertyComment}
          <div class="detail-comment">{detailGroup.propertyComment}</div>
        {/if}

        <div class="detail-values-group">
          {#each detailGroup.values as val, idx (
            detailGroup.property + '_' + val.value + '_' + idx
          )}
            <div
              class="detail-value"
              class:clickable={detailGroup.isObjectProperty}
              role={detailGroup.isObjectProperty ? "button" : undefined}
              tabindex={detailGroup.isObjectProperty ? 0 : undefined}
              onclick={() => detailGroup.isObjectProperty && openEntityInspector(val.value)}
              onkeydown={(e) =>
                detailGroup.isObjectProperty && e.key === 'Enter' && openEntityInspector(val.value)}
            >
              {#if detailGroup.isObjectProperty && val.valueIcon}
                {#if isIconUrl(val.valueIcon)}
                  <img src={getIconUrl(val.valueIcon)} alt="" class="value-icon-image" />
                {:else}
                  <span class="material-symbols-outlined value-icon">{val.valueIcon}</span>
                {/if}
              {/if}
              {#if !detailGroup.isObjectProperty && val.datatype === 'xsd:dateTime'}
                {@const date = new Date(Number(val.value))}
                <div class="timestamp-display">
                  <span class="value-text">
                    {date.toLocaleString('en-US', {
                      year: 'numeric', month: 'short', day: 'numeric',
                      hour: 'numeric', minute: '2-digit', second: '2-digit', hour12: true
                    })}
                  </span>
                  <span class="timestamp-relative">{formatDate(date.getTime())}</span>
                </div>
              {:else if !detailGroup.isObjectProperty && val.datatype === 'xsd:date'}
                {@const [y, m, d] = val.value.split('-').map(Number)}
                {@const date = new Date(y, m - 1, d)}
                <span class="value-text">
                  {date.toLocaleDateString('en-US', { year: 'numeric', month: 'short', day: 'numeric' })}
                </span>
              {:else if !detailGroup.isObjectProperty && isUrl(val.datatype)}
                <button class="url-value" onclick={() => openUrl_(val.value)} title={val.value}>
                  <span class="value-text">{val.valueLabel || val.value}</span>
                  <span class="material-symbols-outlined url-open-icon">open_in_new</span>
                </button>
              {:else if !detailGroup.isObjectProperty && isStringType(val.datatype)}
                {#if (val.value ?? '').length > 50_000}
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
                  <span class="material-symbols-outlined inline-status-icon">
                    radio_button_checked
                  </span>
                  <span class="inline-status-label">{val.valueStatus.label}</span>
                </span>
              {/if}
            </div>
          {/each}
        </div>
      </div>
    {/each}
  </div>
{/if}

<style>
  .details-list {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-bottom: 16px;
  }

  .detail-item {
    padding: 12px;
    background: color-mix(in srgb, var(--color-white) 3%, transparent);
    border-radius: 8px;
    border-left: 3px solid color-mix(in srgb, var(--color-neutral) 30%, transparent);
  }

  .detail-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 6px;
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

  .detail-count {
    font-size: 10px;
    padding: 2px 6px;
    background: color-mix(in srgb, var(--color-accent) 20%, transparent);
    color: var(--color-accent);
    border-radius: 4px;
    font-weight: 600;
  }

  .detail-required {
    font-size: 13px;
    font-weight: 700;
    color: var(--color-warning, #f59e0b);
    line-height: 1;
  }

  .detail-source {
    font-size: 11px;
    color: var(--color-neutral);
    font-family: var(--font-body);
  }

  .detail-comment {
    font-size: 12px;
    color: var(--color-neutral);
    margin-bottom: 8px;
    line-height: 1.4;
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
</style>
