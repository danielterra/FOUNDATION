<script>
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { convertFileSrc } from '@tauri-apps/api/core';
  import NumberFlow from '@number-flow/svelte';
  import MarkdownValue from './MarkdownValue.svelte';
  import PropertyEditForm from './PropertyEditForm.svelte';
  import RecurrenceEditor from './RecurrenceEditor.svelte';
  import { focus } from '$lib/utils/actions';

  let {
    detailGroup,
    editingKey,
    editingDatatype,
    draftValue = $bindable(),
    saving,
    now,
    onSaveEdit,
    onCancelEdit,
    openEntityInspector,
  } = $props();

  function editKey(propertyIri, valueIdx) {
    return `${propertyIri}::${valueIdx}`;
  }

  function isUrl(datatype) {
    return datatype === 'xsd:anyURI';
  }

  function isRruleType(datatype) {
    return datatype === 'foundation:rrule';
  }

  function rruleLabel(rrule) {
    if (!rrule) return 'Nunca';
    if (rrule.includes('FREQ=HOURLY')) return 'A Cada Hora';
    if (rrule.includes('BYDAY=MO,TU,WE,TH,FR')) return 'Dias de Semana';
    if (rrule.includes('BYDAY=SA,SU')) return 'Fins de Semana';
    if (rrule.includes('FREQ=DAILY')) return 'Diariamente';
    if (rrule.includes('FREQ=WEEKLY') && rrule.includes('INTERVAL=2')) return 'Quinzenalmente';
    if (rrule.includes('FREQ=WEEKLY')) return 'Semanalmente';
    if (rrule.includes('FREQ=MONTHLY') && rrule.includes('INTERVAL=6')) return 'A Cada 6 Meses';
    if (rrule.includes('FREQ=MONTHLY') && rrule.includes('INTERVAL=3')) return 'A Cada 3 Meses';
    if (rrule.includes('FREQ=MONTHLY')) return 'Mensalmente';
    if (rrule.includes('FREQ=YEARLY')) return 'Anualmente';
    return 'Personalizada';
  }

  function isStringType(datatype) {
    return !datatype || datatype === 'xsd:string' || datatype === 'rdf:langString';
  }

  function isDateType(datatype) {
    return datatype === 'xsd:date' || datatype === 'xsd:dateTime';
  }

  function isNumericType(datatype) {
    return datatype === 'xsd:decimal' || datatype === 'xsd:integer' ||
           datatype === 'xsd:int' || datatype === 'xsd:long' ||
           datatype === 'xsd:float' || datatype === 'xsd:double' ||
           datatype === 'xsd:nonNegativeInteger' || datatype === 'xsd:positiveInteger';
  }

  function numericStep(datatype) {
    return (datatype === 'xsd:integer' || datatype === 'xsd:int' ||
            datatype === 'xsd:long' || datatype === 'xsd:nonNegativeInteger' ||
            datatype === 'xsd:positiveInteger') ? '1' : 'any';
  }

  async function openUrl_(url) {
    try {
      await openUrl(url);
    } catch (err) {
      console.error('Failed to open URL:', err);
    }
  }

  function formatDate(timestamp) {
    const currentNow = now;
    const ts = typeof timestamp === 'string' ? parseInt(timestamp) : timestamp;
    const date = new Date(ts);

    if (isNaN(date.getTime())) return timestamp;

    const nowDate = new Date(currentNow);
    const today = new Date(nowDate.getFullYear(), nowDate.getMonth(), nowDate.getDate());
    const yesterday = new Date(today);
    yesterday.setDate(yesterday.getDate() - 1);

    const dateDay = new Date(date.getFullYear(), date.getMonth(), date.getDate());
    const diffMs = currentNow - date.getTime();
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
        'en-US', { hour: '2-digit', minute: '2-digit', hour12: false });
      return `Yesterday at ${timeStr}`;
    }

    if (diffDays < 7) {
      const dayName = date.toLocaleDateString('en-US', { weekday: 'long' });
      const timeStr = date.toLocaleTimeString(
        'en-US', { hour: '2-digit', minute: '2-digit', hour12: false });
      return `${dayName} at ${timeStr}`;
    }

    if (date.getFullYear() === nowDate.getFullYear()) {
      return date.toLocaleDateString('en-US', {
        month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', hour12: false
      });
    }

    return date.toLocaleDateString('en-US', {
      year: 'numeric', month: 'short', day: 'numeric',
      hour: '2-digit', minute: '2-digit', hour12: false
    });
  }

  function datePart(v) { return v?.split('T')[0] ?? ''; }
  function timePart(v) { return v?.split('T')[1] ?? ''; }
  function combineDatetime(d, t) {
    if (!d) return '';
    return t ? `${d}T${t}` : `${d}T00:00`;
  }

  const CURRENCY_CODES = new Set(['BRL', 'USD', 'EUR', 'GBP', 'JPY', 'CNY', 'ARS', 'CLP', 'MXN', 'PEN', 'COP', 'UYU']);

  function isCurrencyCode(unitLabel) {
    return !!unitLabel && CURRENCY_CODES.has(unitLabel);
  }

  function formatCurrency(value, currency) {
    const num = Number(value);
    if (isNaN(num)) return String(value);
    return new Intl.NumberFormat('pt-BR', { style: 'currency', currency }).format(num);
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

{#snippet numericEditor(propertyIri, step)}
  <div class="date-edit-container">
    <input
      class="date-input"
      type="number"
      {step}
      bind:value={draftValue}
      onkeydown={(e) => {
        if (e.key === 'Escape') onCancelEdit();
        else if (e.key === 'Enter') onSaveEdit(propertyIri);
      }}
      use:focus
    />
    <div class="edit-actions">
      <button
        class="edit-save-btn"
        onmousedown={(e) => e.preventDefault()}
        onclick={() => onSaveEdit(propertyIri)}
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
        onclick={onCancelEdit}
      >
        <span class="material-symbols-outlined">close</span>
        Cancel
      </button>
    </div>
  </div>
{/snippet}

{#snippet dateEditor(propertyIri, inputType)}
  <div class="date-edit-container">
    {#if inputType === 'datetime-local'}
      <div class="datetime-pair">
        <input
          class="date-input"
          type="date"
          value={datePart(draftValue)}
          oninput={(e) => draftValue = combineDatetime(e.currentTarget.value, timePart(draftValue))}
          onchange={(e) => draftValue = combineDatetime(e.currentTarget.value, timePart(draftValue))}
          use:focus
        />
        <input
          class="date-input"
          type="time"
          value={timePart(draftValue)}
          oninput={(e) => draftValue = combineDatetime(datePart(draftValue), e.currentTarget.value)}
          onchange={(e) => draftValue = combineDatetime(datePart(draftValue), e.currentTarget.value)}
          onkeydown={(e) => {
            if (e.key === 'Escape') onCancelEdit();
            else if (e.key === 'Enter') onSaveEdit(propertyIri);
          }}
        />
      </div>
    {:else}
      <input
        class="date-input"
        type="date"
        value={draftValue}
        oninput={(e) => draftValue = e.currentTarget.value}
        onchange={(e) => draftValue = e.currentTarget.value}
        onkeydown={(e) => {
          if (e.key === 'Escape') onCancelEdit();
          else if (e.key === 'Enter') onSaveEdit(propertyIri);
        }}
        use:focus
      />
    {/if}
    <div class="edit-actions">
      <button
        class="edit-save-btn"
        onmousedown={(e) => e.preventDefault()}
        onclick={() => onSaveEdit(propertyIri)}
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
        onclick={onCancelEdit}
      >
        <span class="material-symbols-outlined">close</span>
        Cancel
      </button>
    </div>
  </div>
{/snippet}

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
        {#if val.unitLabel && !isCurrencyCode(val.unitLabel)}
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
          {#if editingKey === editKey(detailGroup.property, idx)}
            {@render dateEditor(detailGroup.property, 'datetime-local')}
          {:else}
            {@const date = new Date(val.value)}
            <div class="timestamp-display">
              <span class="value-text">
                {date.toLocaleString('pt-BR', {
                  year: 'numeric', month: 'short', day: 'numeric',
                  hour: '2-digit', minute: '2-digit', second: '2-digit', hour12: false
                })}
              </span>
              <span class="timestamp-relative">{formatDate(date.getTime())}</span>
            </div>
          {/if}
        {:else if val.datatype === 'xsd:date'}
          {#if editingKey === editKey(detailGroup.property, idx)}
            {@render dateEditor(detailGroup.property, 'date')}
          {:else}
            {@const [y, m, d] = val.value.split('-').map(Number)}
            {@const date = new Date(y, m - 1, d)}
            <span class="value-text">
              {date.toLocaleDateString('en-US', { year: 'numeric', month: 'short', day: 'numeric' })}
            </span>
          {/if}
        {:else if isUrl(val.datatype)}
          <button class="url-value" onclick={() => openUrl_(val.value)} title={val.value}>
            <span class="value-text">{val.valueLabel || val.value}</span>
            <span class="material-symbols-outlined url-open-icon">open_in_new</span>
          </button>
        {:else if isStringType(val.datatype)}
          {#if editingKey === editKey(detailGroup.property, idx)}
            <PropertyEditForm
              propertyIri={detailGroup.property}
              bind:draftValue
              {saving}
              onsave={onSaveEdit}
              oncancel={onCancelEdit}
            />
          {:else if (val.value ?? '').length > 50_000}
            <div class="value-large">
              <pre class="value-pre">{val.value}</pre>
              <button class="copy-btn" onclick={() => navigator.clipboard.writeText(val.value)} title="Copy value">
                <span class="material-symbols-outlined">content_copy</span>
              </button>
            </div>
          {:else}
            <MarkdownValue value={val.value} {openEntityInspector} />
          {/if}
        {:else if isNumericType(val.datatype)}
          {#if editingKey === editKey(detailGroup.property, idx)}
            {@render numericEditor(detailGroup.property, numericStep(val.datatype))}
          {:else}
            {#if val.unitLabel && !isCurrencyCode(val.unitLabel)}
              <span class="unit">{val.unitLabel}</span>
            {/if}
            {#if detailGroup.isCalculated && !isNaN(Number(val.value)) && val.value !== ''}
              {#if isCurrencyCode(val.unitLabel)}
                <NumberFlow class="value-text" value={Number(val.value)} locales="pt-BR" format={{ style: 'currency', currency: val.unitLabel }} />
              {:else}
                <NumberFlow class="value-text" value={Number(val.value)} />
              {/if}
            {:else}
              <span class="value-text">
                {isCurrencyCode(val.unitLabel) ? formatCurrency(val.value, val.unitLabel) : (val.valueLabel || val.value)}
              </span>
            {/if}
          {/if}
        {:else if isRruleType(val.datatype)}
          {#if editingKey === editKey(detailGroup.property, idx)}
            <RecurrenceEditor
              value={val.value}
              onconfirm={(rrule) => { draftValue = rrule; onSaveEdit(detailGroup.property); }}
              oncancel={onCancelEdit}
            />
          {:else}
            <span class="value-text">{rruleLabel(val.value)}</span>
          {/if}
        {:else}
          {#if val.unitLabel && !isCurrencyCode(val.unitLabel)}
            <span class="unit">{val.unitLabel}</span>
          {/if}
          {#if detailGroup.isCalculated && !isNaN(Number(val.value)) && val.value !== ''}
            {#if isCurrencyCode(val.unitLabel)}
              <NumberFlow class="value-text" value={Number(val.value)} locales="pt-BR" format={{ style: 'currency', currency: val.unitLabel }} />
            {:else}
              <NumberFlow class="value-text" value={Number(val.value)} />
            {/if}
          {:else}
            <span class="value-text">
              {isCurrencyCode(val.unitLabel) ? formatCurrency(val.value, val.unitLabel) : (val.valueLabel || val.value)}
            </span>
          {/if}
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

<style>
  .detail-values-group {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .detail-value {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 3px 0px;
    background: color-mix(in srgb, var(--color-black) 30%, transparent);
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
    object-fit: cover;
  }

  .value-text {
    font-family: var(--font-body);
    font-size: 14px;
    color: var(--color-neutral-active);
    flex: 1;
  }

  .unit {
    font-size: 11px;
    color: var(--color-neutral);
    padding: 2px 6px;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
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
    border: none;
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

  .detail-value.calculated {
    font-style: italic;
    background: color-mix(in srgb, var(--color-accent) 7%, transparent);
    padding-left: 8px;
  }

  .detail-value.calculated .value-text {
    color: var(--color-neutral-active);
  }

  .formula-error {
    display: flex;
    align-items: flex-start;
    gap: 4px;
    padding: 4px 6px;
    background: color-mix(in srgb, var(--color-error, #ef4444) 12%, transparent);
    width: 100%;
    margin-top: 4px;
    box-sizing: border-box;
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

  .datetime-pair {
    display: flex;
    gap: 6px;
  }

  .datetime-pair .date-input {
    flex: 1;
    min-width: 0;
  }

  .date-edit-container {
    display: flex;
    flex-direction: column;
    gap: 6px;
    flex: 1;
    min-width: 0;
  }

  .date-input {
    background: color-mix(in srgb, var(--color-black) 50%, transparent);
    border: none;
    color: var(--color-neutral-active);
    font-family: var(--font-body);
    font-size: 14px;
    padding: 6px 8px;
    outline: none;
    color-scheme: dark;
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
