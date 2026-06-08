<script lang="ts">
  import { untrack } from 'svelte'
  import { invoke, convertFileSrc } from '@tauri-apps/api/core'
  import EntitySearchCombobox from './EntitySearchCombobox.svelte'

  type RefItem = { iri: string; label: string; icon?: string | null }

  let {
    propertyIri,
    rangeClassIri = null,
    rangeClassLabel = null,
    currentValues = [],
    minCount = null,
    maxCount = null,
    saving = false,
    onsave,
    oncancel,
  }: {
    propertyIri: string
    rangeClassIri?: string | null
    rangeClassLabel?: string | null
    currentValues?: RefItem[]
    minCount?: number | null
    maxCount?: number | null
    saving?: boolean
    onsave: (propertyIri: string, iris: string[]) => Promise<void>
    oncancel: () => void
  } = $props()

  let selected = $state<RefItem[]>(
    untrack(() => currentValues.map(v => ({ iri: v.iri, label: v.label, icon: v.icon ?? null })))
  )

  const maxReached = $derived(maxCount != null && selected.length >= maxCount)

  const cardinalityHint = $derived.by(() => {
    if (minCount != null && maxCount != null) {
      if (minCount === maxCount) return `Exactly ${minCount}`
      return `${minCount}–${maxCount}`
    }
    if (minCount != null) return `At least ${minCount}`
    if (maxCount != null) return `Up to ${maxCount}`
    return null
  })

  async function searchFn(query: string) {
    const raw = await invoke<string>('owl__search_entities', {
      query,
      limit: 20,
      typeIri: rangeClassIri ?? null,
    })
    return JSON.parse(raw) as { id: string; label: string; icon?: string | null }[]
  }

  function handleSelect(item: { id: string; label: string; icon?: string | null }) {
    if (maxReached) return
    if (!selected.some(s => s.iri === item.id)) {
      selected = [...selected, { iri: item.id, label: item.label, icon: item.icon ?? null }]
    }
  }

  function remove(iri: string) {
    selected = selected.filter(s => s.iri !== iri)
  }

  function isIconUrl(icon: string): boolean {
    return (
      icon.startsWith('http://') ||
      icon.startsWith('https://') ||
      icon.startsWith('data:') ||
      icon.startsWith('file://') ||
      icon.startsWith('/')
    )
  }

  function getIconUrl(icon: string): string {
    if (icon.startsWith('file://')) return convertFileSrc(icon.replace(/^file:\/\//, ''))
    if (icon.startsWith('/')) return convertFileSrc(icon)
    return icon
  }

  async function save() {
    if (saving) return
    await onsave(propertyIri, selected.map(s => s.iri))
  }
</script>

<div class="ref-select">
  {#if selected.length > 0}
    <div class="chips">
      {#each selected as item (item.iri)}
        <span class="chip">
          {#if item.icon}
            {#if isIconUrl(item.icon)}
              <img src={getIconUrl(item.icon)} alt="" class="chip-img" />
            {:else}
              <span class="material-symbols-outlined chip-icon">{item.icon}</span>
            {/if}
          {/if}
          <span class="chip-label">{item.label ?? item.iri}</span>
          <button class="chip-remove" onclick={() => remove(item.iri)} aria-label="Remover {item.label ?? item.iri}">
            <span class="material-symbols-outlined">close</span>
          </button>
        </span>
      {/each}
    </div>
  {/if}

  {#if !maxReached}
    <EntitySearchCombobox
      {searchFn}
      debounceMs={200}
      multiple={true}
      placeholder="Search {rangeClassLabel ?? 'entity'}…"
      emptyText="Nenhum resultado."
      {saving}
      disabled={saving}
      onSelect={handleSelect}
    />
  {/if}

  <div class="actions-row">
    {#if cardinalityHint}
      <span class="cardinality-hint">{cardinalityHint}</span>
    {/if}
    <div class="actions">
      <button class="save-btn" onclick={save} disabled={saving}>
        {#if saving}
          <span class="material-symbols-outlined spinning">progress_activity</span>
        {:else}
          <span class="material-symbols-outlined">check</span>
        {/if}
        Save
      </button>
      <button
        class="cancel-btn"
        onmousedown={(e) => e.preventDefault()}
        onclick={oncancel}
      >
        <span class="material-symbols-outlined">close</span>
        Cancel
      </button>
    </div>
  </div>
</div>

<style>
  .ref-select {
    display: flex;
    flex-direction: column;
    gap: 6px;
    flex: 1;
    min-width: 0;
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }

  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 6px 3px 8px;
    background: color-mix(in srgb, var(--color-interactive) 15%, transparent);
    font-family: var(--font-body);
    font-size: 12px;
    color: var(--color-neutral-active);
  }

  .chip-icon {
    font-size: 14px;
    color: var(--color-interactive);
  }

  .chip-img {
    width: 14px;
    height: 14px;
    object-fit: cover;
  }

  .chip-label {
    max-width: 140px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chip-remove {
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    color: var(--color-neutral);
    display: flex;
    align-items: center;
    transition: color 0.15s;
  }

  .chip-remove:hover {
    color: var(--color-neutral-active);
  }

  .chip-remove .material-symbols-outlined {
    font-size: 14px;
  }

  .actions-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
  }

  .cardinality-hint {
    font-family: var(--font-body);
    font-size: 11px;
    color: var(--color-neutral);
    opacity: 0.6;
  }

  .actions {
    display: flex;
    gap: 6px;
    margin-left: auto;
  }

  .save-btn,
  .cancel-btn {
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

  .save-btn {
    background: color-mix(in srgb, var(--color-interactive) 25%, transparent);
    color: var(--color-interactive);
  }

  .save-btn:hover:not(:disabled) {
    background: color-mix(in srgb, var(--color-interactive) 40%, transparent);
  }

  .save-btn:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .cancel-btn {
    background: color-mix(in srgb, var(--color-neutral) 12%, transparent);
    color: var(--color-neutral);
  }

  .cancel-btn:hover {
    background: color-mix(in srgb, var(--color-neutral) 22%, transparent);
  }

  .save-btn .material-symbols-outlined,
  .cancel-btn .material-symbols-outlined {
    font-size: 14px;
  }

  @keyframes spinning {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  .spinning {
    animation: spinning 1s linear infinite;
  }
</style>
