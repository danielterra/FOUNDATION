<script>
  import { untrack } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { Input } from '$lib/components/ui/input';
  import { Textarea } from '$lib/components/ui/textarea';
  import * as Select from '$lib/components/ui/select';
  import EntitySearchCombobox from './EntitySearchCombobox.svelte';

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

  let label = $state(untrack(() => initialValues.label ?? ''));
  let propertyType = $state(untrack(() => initialValues.propertyType ?? 'datatype'));
  let range = $state(untrack(() => initialValues.range ?? 'xsd:string'));
  let unit = $state(untrack(() => initialValues.unit ?? ''));
  let comment = $state(untrack(() => initialValues.comment ?? ''));

  let selectedClassName = $state(untrack(() =>
    initialValues.rangeLabel ?? (initialValues.range && !initialValues.range.startsWith('xsd:') ? initialValues.range : '')
  ));

  let labelInputRef = $state(null);

  $effect(() => {
    if (mode === 'add' && labelInputRef) labelInputRef.focus();
  });

  $effect(() => {
    if (propertyType === 'datatype') {
      if (!range.startsWith('xsd:')) {
        range = 'xsd:string';
      }
    } else {
      if (range.startsWith('xsd:')) {
        range = '';
        selectedClassName = '';
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

  async function classSearchFn(query) {
    const raw = await invoke('owl__search_entities', { query, limit: 15, typeIri: 'owl:Class' });
    return JSON.parse(raw);
  }

  function selectClass(result) {
    range = result.id;
    selectedClassName = result.label;
  }

  function clearClass() {
    range = '';
    selectedClassName = '';
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
    <Input
      bind:ref={labelInputRef}
      id="cpf-label-input"
      class="cpf-input"
      type="text"
      placeholder="Property name"
      bind:value={label}
      onkeydown={(e) => { if (e.key === 'Enter') handleSave(); if (e.key === 'Escape') oncancel(); }}
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
      <Select.Root
        type="single"
        value={range}
        onValueChange={(v) => { if (v) range = v; }}
      >
        <Select.Trigger class="cpf-select-trigger">
          <Select.Value placeholder="Selecionar tipo" />
        </Select.Trigger>
        <Select.Content>
          {#each XSD_TYPES as xsd}
            <Select.Item value={xsd.value}>{xsd.label}</Select.Item>
          {/each}
        </Select.Content>
      </Select.Root>
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
            <EntitySearchCombobox
              searchFn={classSearchFn}
              debounceMs={250}
              onSelect={selectClass}
              placeholder="Search for a class…"
              emptyText="No class found."
              {saving}
            />
          </div>
        {/if}
      </div>
    {/if}
  </div>

  {#if needsUnit}
    <div class="cpf-row">
      <label class="cpf-label" for="cpf-unit-input">Unit</label>
      <Input
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
    <Textarea
      id="cpf-comment-input"
      class="cpf-textarea"
      placeholder="Optional description"
      bind:value={comment}
      rows={2}
    />
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

  :global([data-slot="input"].cpf-input),
  :global([data-slot="textarea"].cpf-textarea) {
    flex: 1;
    min-width: 0;
    background: color-mix(in srgb, var(--color-black) 50%, transparent);
    border: none;
    color: var(--color-neutral-active);
    font-family: var(--font-body);
    font-size: 14px;
    padding: 5px 8px;
    height: auto;
    border-radius: 0;
    box-shadow: none;
    min-height: unset;
  }

  :global([data-slot="textarea"].cpf-textarea) {
    resize: vertical;
  }

  :global(.cpf-select-trigger) {
    flex: 1;
    min-width: 0;
    background: color-mix(in srgb, var(--color-black) 50%, transparent);
    border: none;
    color: var(--color-neutral-active);
    font-family: var(--font-body);
    font-size: 14px;
    padding: 5px 8px;
    height: auto;
    border-radius: 0;
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
    border: none;
    background: transparent;
    color: var(--color-neutral);
    font-family: var(--font-body);
    font-size: 12px;
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
  }

  .cpf-type-btn.active {
    background: color-mix(in srgb, var(--color-interactive) 20%, transparent);
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
  }

  .cpf-class-search-wrap {
    flex: 1;
    min-width: 0;
  }

  .cpf-selected-class {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 5px 8px;
    background: color-mix(in srgb, var(--color-interactive) 15%, transparent);
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
