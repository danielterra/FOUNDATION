<script>
  import { invoke } from '@tauri-apps/api/core';
  import { focus } from '$lib/utils/actions';

  let {
    mode = 'add',
    initialValues = {},
    saving = false,
    onsave,
    oncancel,
  } = $props();

  const XSD_TYPES = [
    { value: 'xsd:string', label: 'String' },
    { value: 'xsd:integer', label: 'Integer' },
    { value: 'xsd:decimal', label: 'Decimal' },
    { value: 'xsd:boolean', label: 'Boolean' },
    { value: 'xsd:date', label: 'Date' },
    { value: 'xsd:dateTime', label: 'DateTime' },
    { value: 'xsd:anyURI', label: 'URL' },
  ];

  const NUMERIC_TYPES = new Set(['xsd:integer', 'xsd:decimal']);

  let label = $state(initialValues.label ?? '');
  let propertyType = $state(initialValues.propertyType ?? 'datatype');
  let range = $state(initialValues.range ?? 'xsd:string');
  let unit = $state(initialValues.unit ?? '');
  let comment = $state(initialValues.comment ?? '');

  let classQuery = $state('');
  let classResults = $state([]);
  let classSearching = $state(false);
  let showClassDropdown = $state(false);
  let selectedClassName = $state(
    initialValues.rangeLabel ?? (initialValues.range && !initialValues.range.startsWith('xsd:') ? initialValues.range : '')
  );

  let debounceTimer = null;

  $effect(() => {
    if (propertyType === 'datatype') {
      if (!range.startsWith('xsd:')) {
        range = 'xsd:string';
      }
    } else {
      if (range.startsWith('xsd:')) {
        range = '';
        selectedClassName = '';
        classQuery = '';
      }
    }
  });

  const needsUnit = $derived(propertyType === 'datatype' && NUMERIC_TYPES.has(range));
  const canSave = $derived(
    label.trim().length > 0 &&
    (propertyType === 'datatype'
      ? range.startsWith('xsd:')
      : range.length > 0) &&
    (!needsUnit || unit.trim().length > 0)
  );

  async function searchClasses(q) {
    if (!q.trim()) {
      classResults = [];
      showClassDropdown = false;
      return;
    }
    classSearching = true;
    try {
      const raw = await invoke('graph__search_entities', { query: q, limit: 15, typeIri: 'owl:Class' });
      classResults = JSON.parse(raw);
      showClassDropdown = classResults.length > 0;
    } catch {
      classResults = [];
    } finally {
      classSearching = false;
    }
  }

  function onClassQueryInput(e) {
    classQuery = e.target.value;
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => searchClasses(classQuery), 250);
  }

  function selectClass(result) {
    range = result.id;
    selectedClassName = result.label;
    classQuery = '';
    classResults = [];
    showClassDropdown = false;
  }

  function clearClass() {
    range = '';
    selectedClassName = '';
    classQuery = '';
    classResults = [];
  }

  function handleSave() {
    if (!canSave || saving) return;
    onsave({
      label: label.trim(),
      propertyType,
      range,
      unit: needsUnit ? unit.trim() : null,
      comment: comment.trim() || null,
    });
  }
</script>

<div class="cpf-form">
  <div class="cpf-row">
    <label class="cpf-label" for="cpf-label-input">Label</label>
    <input
      id="cpf-label-input"
      class="cpf-input"
      type="text"
      placeholder="Property name"
      bind:value={label}
      onkeydown={(e) => { if (e.key === 'Enter') handleSave(); if (e.key === 'Escape') oncancel(); }}
      use:focus={mode === 'add'}
    />
  </div>

  <div class="cpf-row">
    <span class="cpf-label">Type</span>
    <div class="cpf-type-select">
      <button
        class="cpf-type-btn"
        class:active={propertyType === 'datatype'}
        onclick={() => propertyType = 'datatype'}
        disabled={mode === 'edit'}
      >
        <span class="material-symbols-outlined cpf-type-icon">data_object</span>
        Datatype
      </button>
      <button
        class="cpf-type-btn"
        class:active={propertyType === 'object'}
        onclick={() => propertyType = 'object'}
        disabled={mode === 'edit'}
      >
        <span class="material-symbols-outlined cpf-type-icon">link</span>
        Object
      </button>
    </div>
  </div>

  <div class="cpf-row">
    <span class="cpf-label">Range</span>
    {#if propertyType === 'datatype'}
      <select class="cpf-select" bind:value={range}>
        {#each XSD_TYPES as xsd}
          <option value={xsd.value}>{xsd.label}</option>
        {/each}
      </select>
    {:else}
      <div class="cpf-class-picker">
        {#if selectedClassName}
          <div class="cpf-selected-class">
            <span class="material-symbols-outlined cpf-class-icon">category</span>
            <span class="cpf-class-name">{selectedClassName}</span>
            <button class="cpf-clear-btn" onclick={clearClass} title="Clear">
              <span class="material-symbols-outlined">close</span>
            </button>
          </div>
        {:else}
          <div class="cpf-class-search-wrap">
            <input
              class="cpf-input"
              type="text"
              placeholder="Search for a class…"
              value={classQuery}
              oninput={onClassQueryInput}
              onkeydown={(e) => { if (e.key === 'Escape') { oncancel(); } }}
            />
            {#if classSearching}
              <span class="material-symbols-outlined cpf-spinner">progress_activity</span>
            {/if}
            {#if showClassDropdown}
              <div class="cpf-dropdown">
                {#each classResults as result}
                  <button class="cpf-dropdown-item" onclick={() => selectClass(result)}>
                    {#if result.icon}
                      <span class="material-symbols-outlined cpf-item-icon">{result.icon}</span>
                    {/if}
                    <span>{result.label}</span>
                  </button>
                {/each}
              </div>
            {/if}
          </div>
        {/if}
      </div>
    {/if}
  </div>

  {#if needsUnit}
    <div class="cpf-row">
      <label class="cpf-label" for="cpf-unit-input">Unit</label>
      <input
        id="cpf-unit-input"
        class="cpf-input"
        type="text"
        placeholder="e.g. unit:Meter, unit:Second"
        bind:value={unit}
      />
    </div>
  {/if}

  <div class="cpf-row">
    <label class="cpf-label" for="cpf-comment-input">Comment</label>
    <textarea
      id="cpf-comment-input"
      class="cpf-textarea"
      placeholder="Optional description"
      bind:value={comment}
      rows="2"
    ></textarea>
  </div>

  <div class="cpf-actions">
    <button class="cpf-save-btn" onclick={handleSave} disabled={!canSave || saving}>
      {#if saving}
        <span class="material-symbols-outlined spinning-small">progress_activity</span>
      {:else}
        <span class="material-symbols-outlined">{mode === 'add' ? 'add' : 'check'}</span>
      {/if}
      {mode === 'add' ? 'Add property' : 'Save'}
    </button>
    <button class="cpf-cancel-btn" onclick={oncancel}>
      <span class="material-symbols-outlined">close</span>
      Cancel
    </button>
  </div>
</div>

<style>
  .cpf-form {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px;
    background: color-mix(in srgb, var(--color-interactive) 6%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-interactive) 25%, transparent);
  }

  .cpf-row {
    display: flex;
    align-items: flex-start;
    gap: 8px;
  }

  .cpf-label {
    width: 54px;
    min-width: 54px;
    font-size: 11px;
    font-weight: 600;
    color: var(--color-neutral);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    padding-top: 6px;
  }

  .cpf-input {
    flex: 1;
    min-width: 0;
    background: color-mix(in srgb, var(--color-black) 50%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-interactive) 35%, transparent);
    color: var(--color-neutral-active);
    font-family: var(--font-body);
    font-size: 14px;
    padding: 5px 8px;
    outline: none;
    transition: border-color 0.15s;
  }

  .cpf-input:focus {
    border-color: var(--color-interactive);
  }

  .cpf-textarea {
    flex: 1;
    min-width: 0;
    background: color-mix(in srgb, var(--color-black) 50%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-interactive) 35%, transparent);
    color: var(--color-neutral-active);
    font-family: var(--font-body);
    font-size: 14px;
    padding: 5px 8px;
    outline: none;
    resize: vertical;
    transition: border-color 0.15s;
  }

  .cpf-textarea:focus {
    border-color: var(--color-interactive);
  }

  .cpf-select {
    flex: 1;
    background: color-mix(in srgb, var(--color-black) 50%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-interactive) 35%, transparent);
    color: var(--color-neutral-active);
    font-family: var(--font-body);
    font-size: 14px;
    padding: 5px 8px;
    outline: none;
    cursor: pointer;
  }

  .cpf-type-select {
    display: flex;
    gap: 4px;
    flex: 1;
  }

  .cpf-type-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 10px;
    border: 1px solid color-mix(in srgb, var(--color-neutral) 20%, transparent);
    background: transparent;
    color: var(--color-neutral);
    font-family: var(--font-body);
    font-size: 12px;
    cursor: pointer;
    transition: background 0.15s, color 0.15s, border-color 0.15s;
  }

  .cpf-type-btn.active {
    background: color-mix(in srgb, var(--color-interactive) 20%, transparent);
    border-color: color-mix(in srgb, var(--color-interactive) 50%, transparent);
    color: var(--color-interactive);
  }

  .cpf-type-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .cpf-type-icon {
    font-size: 14px;
  }

  .cpf-class-picker {
    flex: 1;
    min-width: 0;
    position: relative;
  }

  .cpf-class-search-wrap {
    position: relative;
    display: flex;
    align-items: center;
  }

  .cpf-class-search-wrap .cpf-input {
    width: 100%;
  }

  .cpf-spinner {
    position: absolute;
    right: 8px;
    font-size: 14px;
    color: var(--color-neutral);
    animation: spin 1s linear infinite;
  }

  .cpf-selected-class {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 8px;
    background: color-mix(in srgb, var(--color-interactive) 15%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-interactive) 35%, transparent);
    font-size: 14px;
    color: var(--color-neutral-active);
  }

  .cpf-class-icon {
    font-size: 14px;
    color: var(--color-neutral);
  }

  .cpf-class-name {
    flex: 1;
  }

  .cpf-clear-btn {
    display: flex;
    align-items: center;
    background: none;
    border: none;
    cursor: pointer;
    color: var(--color-neutral);
    padding: 0;
  }

  .cpf-clear-btn .material-symbols-outlined {
    font-size: 14px;
  }

  .cpf-dropdown {
    position: absolute;
    top: calc(100% + 4px);
    left: 0;
    right: 0;
    background: var(--color-bg-elevated, var(--color-black));
    border: 1px solid color-mix(in srgb, var(--color-interactive) 30%, transparent);
    box-shadow: 0 4px 12px rgba(0, 0, 0, 0.4);
    z-index: 100;
    max-height: 180px;
    overflow-y: auto;
  }

  .cpf-dropdown-item {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 7px 10px;
    background: none;
    border: none;
    color: var(--color-neutral-active);
    font-family: var(--font-body);
    font-size: 14px;
    cursor: pointer;
    text-align: left;
  }

  .cpf-dropdown-item:hover {
    background: color-mix(in srgb, var(--color-interactive) 15%, transparent);
  }

  .cpf-item-icon {
    font-size: 14px;
    color: var(--color-neutral);
  }

  .cpf-actions {
    display: flex;
    gap: 6px;
    margin-top: 2px;
  }

  .cpf-save-btn,
  .cpf-cancel-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 5px 12px;
    border: none;
    cursor: pointer;
    font-family: var(--font-body);
    font-size: 12px;
    font-weight: 600;
    transition: background 0.15s;
  }

  .cpf-save-btn {
    background: color-mix(in srgb, var(--color-interactive) 25%, transparent);
    color: var(--color-interactive);
  }

  .cpf-save-btn:hover:not(:disabled) {
    background: color-mix(in srgb, var(--color-interactive) 40%, transparent);
  }

  .cpf-save-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .cpf-cancel-btn {
    background: color-mix(in srgb, var(--color-neutral) 12%, transparent);
    color: var(--color-neutral);
  }

  .cpf-cancel-btn:hover {
    background: color-mix(in srgb, var(--color-neutral) 22%, transparent);
  }

  .cpf-save-btn .material-symbols-outlined,
  .cpf-cancel-btn .material-symbols-outlined {
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
