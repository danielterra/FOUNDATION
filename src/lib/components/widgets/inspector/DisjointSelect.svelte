<script>
  import { invoke } from '@tauri-apps/api/core';
  import { focus } from '$lib/utils/actions';

  let {
    excludeIris = [],
    saving = false,
    onsave,
    oncancel,
  } = $props();

  let query = $state('');
  let results = $state([]);
  let searching = $state(false);
  let showDropdown = $state(false);
  let debounceTimer = null;

  async function search(q) {
    if (!q.trim()) {
      results = [];
      showDropdown = false;
      return;
    }
    searching = true;
    try {
      const raw = await invoke('graph__search_entities', {
        query: q,
        limit: 20,
        typeIri: 'owl:Class',
      });
      const parsed = JSON.parse(raw);
      results = parsed.filter(r => !excludeIris.includes(r.id));
      showDropdown = results.length > 0;
    } catch {
      results = [];
    } finally {
      searching = false;
    }
  }

  function onInput(e) {
    query = e.target.value;
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(() => search(query), 200);
  }

  async function pick(result) {
    if (saving) return;
    await onsave(result.id);
  }
</script>

<div class="disjoint-select">
  <div class="search-row">
    <input
      type="text"
      use:focus
      class="search-input"
      placeholder="Buscar classe..."
      value={query}
      oninput={onInput}
      disabled={saving}
    />
    <button class="cancel-btn" onclick={oncancel} disabled={saving}>
      <span class="material-symbols-outlined">close</span>
    </button>
  </div>

  {#if searching}
    <div class="hint">Buscando...</div>
  {/if}

  {#if showDropdown}
    <div class="dropdown">
      {#each results as r (r.id)}
        <button class="result" onclick={() => pick(r)} disabled={saving}>
          {#if r.icon}
            <span class="material-symbols-outlined">{r.icon}</span>
          {/if}
          <span class="result-label">{r.label ?? r.id}</span>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .disjoint-select {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 10px;
  }

  .search-row {
    display: flex;
    gap: 4px;
  }

  .search-input {
    flex: 1;
    padding: 6px 10px;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-neutral) 25%, transparent);
    color: var(--color-neutral-active);
    font-family: var(--font-body);
    font-size: 13px;
  }

  .search-input:focus {
    outline: none;
    border-color: var(--color-interactive);
  }

  .cancel-btn {
    padding: 4px 8px;
    background: color-mix(in srgb, var(--color-neutral) 12%, transparent);
    border: none;
    color: var(--color-neutral);
    cursor: pointer;
  }

  .cancel-btn .material-symbols-outlined {
    font-size: 16px;
  }

  .hint {
    font-size: 11px;
    color: color-mix(in srgb, var(--color-neutral) 60%, transparent);
    padding: 4px 0;
  }

  .dropdown {
    display: flex;
    flex-direction: column;
    background: var(--color-surface-1);
    border: 1px solid color-mix(in srgb, var(--color-neutral) 20%, transparent);
    max-height: 200px;
    overflow-y: auto;
  }

  .result {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    background: transparent;
    border: none;
    color: var(--color-neutral-active);
    font-family: var(--font-body);
    font-size: 13px;
    cursor: pointer;
    text-align: left;
  }

  .result:hover:not(:disabled) {
    background: color-mix(in srgb, var(--color-interactive) 15%, transparent);
  }

  .result .material-symbols-outlined {
    font-size: 16px;
    color: var(--color-interactive);
  }

  .result-label {
    flex: 1;
  }
</style>
