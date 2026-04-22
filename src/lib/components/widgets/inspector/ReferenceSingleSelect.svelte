<script>
  import { invoke } from '@tauri-apps/api/core'
  import { convertFileSrc } from '@tauri-apps/api/core'
  import { focus } from '$lib/utils/actions'

  let {
    propertyIri,
    rangeClassIri = null,
    rangeClassLabel = null,
    currentValue = null,
    saving = false,
    onsave,
    oncancel,
  } = $props()

  let query = $state('')
  let results = $state([])
  let searching = $state(false)
  let showDropdown = $state(false)
  let debounceTimer = null

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
      showDropdown = results.length > 0
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

  async function selectResult(result) {
    if (saving) return
    showDropdown = false
    await onsave(propertyIri, [result.id])
  }

  async function clear() {
    if (saving) return
    await onsave(propertyIri, [])
  }
</script>

<div class="ref-single">
  {#if currentValue}
    <div class="current-value">
      {#if currentValue.icon}
        {#if isIconUrl(currentValue.icon)}
          <img src={getIconUrl(currentValue.icon)} alt="" class="current-icon-img" />
        {:else}
          <span class="material-symbols-outlined current-icon">{currentValue.icon}</span>
        {/if}
      {/if}
      <span class="current-label">{currentValue.label ?? currentValue.iri}</span>
      <button class="clear-btn" onclick={clear} disabled={saving} title="Remover">
        <span class="material-symbols-outlined">close</span>
      </button>
    </div>
  {/if}

  <div class="search-row">
    <span class="material-symbols-outlined search-icon">search</span>
    <input
      class="search-input"
      type="text"
      placeholder="Buscar {rangeClassLabel ?? 'entidade'}…"
      value={query}
      oninput={onInput}
      onfocus={() => { if (results.length > 0) showDropdown = true }}
      onblur={() => setTimeout(() => { showDropdown = false }, 150)}
      use:focus={!currentValue}
    />
    {#if searching}
      <span class="material-symbols-outlined search-spin">progress_activity</span>
    {/if}
  </div>

  {#if showDropdown}
    <div class="dropdown">
      {#each results as result (result.id)}
        {@const isCurrent = currentValue?.iri === result.id}
        <button
          class="dropdown-item"
          class:current={isCurrent}
          onmousedown={(e) => e.preventDefault()}
          onclick={() => selectResult(result)}
          disabled={saving}
        >
          {#if result.icon}
            {#if isIconUrl(result.icon)}
              <img src={getIconUrl(result.icon)} alt="" class="item-img" />
            {:else}
              <span class="material-symbols-outlined item-icon">{result.icon}</span>
            {/if}
          {/if}
          <span class="item-label">{result.label}</span>
          {#if isCurrent}
            <span class="material-symbols-outlined item-check">check</span>
          {/if}
        </button>
      {/each}
    </div>
  {/if}

  <div class="actions-row">
    <button class="cancel-btn" onmousedown={(e) => e.preventDefault()} onclick={oncancel} disabled={saving}>
      <span class="material-symbols-outlined">close</span>
      Cancelar
    </button>
  </div>
</div>

<style>
  .ref-single {
    display: flex;
    flex-direction: column;
    gap: 6px;
    flex: 1;
    min-width: 0;
  }

  .current-value {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    background: color-mix(in srgb, var(--color-interactive) 12%, transparent);
  }

  .current-icon {
    font-size: 16px;
    color: var(--color-interactive);
    flex-shrink: 0;
  }

  .current-icon-img {
    width: 16px;
    height: 16px;
    object-fit: cover;
    flex-shrink: 0;
  }

  .current-label {
    flex: 1;
    font-family: var(--font-body);
    font-size: 14px;
    color: var(--color-neutral-active);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .clear-btn {
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    color: var(--color-neutral);
    display: flex;
    align-items: center;
    flex-shrink: 0;
  }

  .clear-btn:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .clear-btn .material-symbols-outlined {
    font-size: 14px;
  }

  .search-row {
    position: relative;
    display: flex;
    align-items: center;
    background: color-mix(in srgb, var(--color-black) 50%, transparent);
  }

  .search-icon {
    font-size: 14px;
    color: var(--color-neutral);
    opacity: 0.5;
    padding-left: 8px;
    flex-shrink: 0;
  }

  .search-input {
    flex: 1;
    background: transparent;
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
    flex-shrink: 0;
  }

  .dropdown {
    background: color-mix(in srgb, var(--color-black) 90%, transparent);
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
  }

  .dropdown-item.current {
    color: var(--color-interactive);
  }

  .dropdown-item:disabled {
    opacity: 0.5;
    cursor: default;
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
    justify-content: flex-end;
  }

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
    background: color-mix(in srgb, var(--color-neutral) 12%, transparent);
    color: var(--color-neutral);
  }

  .cancel-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .cancel-btn .material-symbols-outlined {
    font-size: 14px;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }
</style>
