<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'
  import EntitySearchCombobox from './EntitySearchCombobox.svelte'
  import { Button } from '$lib/components/ui/button'

  let {
    excludeIris = [],
    saving = false,
    onsave,
    oncancel,
  } = $props()

  async function searchFn(query: string) {
    const raw = await invoke<string>('owl__search_entities', {
      query,
      limit: 20,
      typeIri: 'owl:Class',
    })
    const parsed = JSON.parse(raw) as Array<{ id: string; label: string; icon?: string | null }>
    return parsed.filter(r => !excludeIris.includes(r.id))
  }

  async function handleSelect(item: { id: string; label: string; icon?: string | null }) {
    if (saving) return
    await onsave(item.id)
  }
</script>

<div class="disjoint-select">
  <div class="search-row">
    <div class="combobox-wrap">
      <EntitySearchCombobox
        {searchFn}
        debounceMs={200}
        onSelect={handleSelect}
        placeholder="Buscar classe..."
        emptyText="Nenhuma classe encontrada."
        {saving}
      />
    </div>
    <Button variant="ghost" size="icon-sm" class="cancel-btn" onclick={oncancel} disabled={saving} aria-label="Cancelar">
      <span class="material-symbols-outlined">close</span>
    </Button>
  </div>
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
    align-items: stretch;
  }

  .combobox-wrap {
    flex: 1;
    min-width: 0;
  }

  :global([data-slot="button"].cancel-btn) {
    padding: 4px 8px;
    height: auto;
    background: color-mix(in srgb, var(--color-neutral) 12%, transparent);
    color: var(--color-neutral);
  }

  :global([data-slot="button"].cancel-btn:disabled) {
    opacity: 0.5;
    cursor: default;
  }

  :global([data-slot="button"].cancel-btn .material-symbols-outlined) {
    font-size: 16px;
  }
</style>
