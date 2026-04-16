<script>
  import { invoke } from '@tauri-apps/api/core'
  import { convertFileSrc } from '@tauri-apps/api/core'
  import { focus } from '$lib/utils/actions'

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
  } = $props()

  let selected = $state(currentValues.map(v => ({ iri: v.iri, label: v.label, icon: v.icon })))
  let query = $state('')
  let results = $state([])
  let searching = $state(false)
  let showDropdown = $state(false)
  let debounceTimer = null

  const maxReached = $derived(maxCount != null && selected.length >= maxCount)

  function isIconUrl(icon) {
    if (!icon) return false
    return icon.startsWith('http://') || icon.startsWith('https://') ||
           icon.startsWith('data:') || icon.startsWith('file://') || icon.startsWith('/')
  }

  function getIconUrl(icon) {
    if (!icon) return ''
    if (icon.startsWith('file://')) return convertFileSrc(icon.replace(/^file:\/\//, ''))
    if (icon.startsWith('/')) return convertFileSrc(icon)
    return icon
  }

  async function search(q) {
    if (!q.trim()) {
      results = []
      showDropdown = false
      return
    }
    searching = true
    try {
      const raw = await invoke('graph__search_entities', {
        query: q,
        limit: 20,
        typeIri: rangeClassIri ?? null,
      })
      results = JSON.parse(raw)
      showDropdown = results.length > 0 && !maxReached
    } catch {
      results = []
    } finally {
      searching = false
    }
  }

  function onInput(e) {
    query = e.target.value
    clearTimeout(debounceTimer)
    debounceTimer = setTimeout(() => search(query), 200)
  }

  function selectResult(result) {
    if (maxReached) return
    if (!selected.some(s => s.iri === result.id)) {
      selected = [...selected, { iri: result.id, label: result.label, icon: result.icon ?? null }]
    }
    query = ''
    results = []
    showDropdown = false
  }

  function remove(iri) {
    selected = selected.filter(s => s.iri !== iri)
  }

  async function save() {
    if (saving) return
    await onsave(propertyIri, selected.map(s => s.iri))
  }

  const cardinalityHint = $derived.by(() => {
    if (minCount != null && maxCount != null) {
      if (minCount === maxCount) return `Exactly ${minCount}`
      return `${minCount}–${maxCount}`
    }
    if (minCount != null) return `At least ${minCount}`
    if (maxCount != null) return `Up to ${maxCount}`
    return null
  })
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
          <button class="chip-remove" onclick={() => remove(item.iri)} title="Remove">
            <span class="material-symbols-outlined">close</span>
          </button>
        </span>
      {/each}
    </div>
  {/if}

  {#if !maxReached}
    <div class="search-row">
      <input
        class="search-input"
        type="text"
        placeholder="Search {rangeClassLabel ?? 'entity'}…"
        value={query}
        oninput={onInput}
        onfocus={() => { if (results.length > 0) showDropdown = true }}
        onblur={() => setTimeout(() => { showDropdown = false }, 150)}
        use:focus={selected.length === 0}
      />
      {#if searching}
        <span class="material-symbols-outlined search-spin">progress_activity</span>
      {/if}
    </div>

    {#if showDropdown}
      <div class="dropdown">
        {#each results as result (result.id)}
          {@const alreadySelected = selected.some(s => s.iri === result.id)}
          <button
            class="dropdown-item"
            class:selected={alreadySelected}
            onmousedown={(e) => e.preventDefault()}
            onclick={() => selectResult(result)}
          >
            {#if result.icon}
              {#if isIconUrl(result.icon)}
                <img src={getIconUrl(result.icon)} alt="" class="item-img" />
              {:else}
                <span class="material-symbols-outlined item-icon">{result.icon}</span>
              {/if}
            {/if}
            <span class="item-label">{result.label}</span>
            {#if alreadySelected}
              <span class="material-symbols-outlined item-check">check</span>
            {/if}
          </button>
        {/each}
      </div>
    {/if}
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

  .search-row {
    position: relative;
    display: flex;
    align-items: center;
  }

  .search-input {
    flex: 1;
    background: color-mix(in srgb, var(--color-black) 50%, transparent);
    border: none;
    color: var(--color-neutral-active);
    font-family: var(--font-body);
    font-size: 14px;
    padding: 6px 8px;
    outline: none;
  }

  .search-spin {
    position: absolute;
    right: 8px;
    font-size: 14px;
    color: var(--color-neutral);
    animation: spin 1s linear infinite;
  }

  .dropdown {
    background: color-mix(in srgb, var(--color-black) 90%, transparent);
    overflow: hidden;
    max-height: 200px;
    overflow-y: auto;
  }

  .dropdown-item {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 7px 10px;
    background: none;
    border: none;
    cursor: pointer;
    color: var(--color-neutral-active);
    font-family: var(--font-body);
    font-size: 14px;
    text-align: left;
    transition: background 0.1s;
  }

  .dropdown-item:hover {
    background: color-mix(in srgb, var(--color-interactive) 15%, transparent);
  }

  .dropdown-item.selected {
    background: color-mix(in srgb, var(--color-interactive) 10%, transparent);
    color: var(--color-interactive);
  }

  .item-icon {
    font-size: 16px;
    color: var(--color-interactive);
    flex-shrink: 0;
  }

  .item-img {
    width: 18px;
    height: 18px;
    object-fit: cover;
    flex-shrink: 0;
  }

  .item-label {
    flex: 1;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .item-check {
    font-size: 14px;
    color: var(--color-interactive);
    flex-shrink: 0;
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

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }
</style>
