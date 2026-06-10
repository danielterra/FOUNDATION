<script lang="ts">
  import { invoke } from '@tauri-apps/api/core'
  import { convertFileSrc } from '@tauri-apps/api/core'
  import EntitySearchCombobox from './EntitySearchCombobox.svelte'
  import { Button } from '$lib/components/ui/button'

  let {
    propertyIri,
    rangeClassIri = null,
    rangeClassLabel = null,
    currentValue = null,
    saving = false,
    onsave,
    oncancel,
  } = $props()

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

  async function searchFn(query: string) {
    const raw = await invoke<string>('owl__search_entities', {
      query,
      limit: 20,
      typeIri: rangeClassIri ?? null,
    })
    return JSON.parse(raw) as Array<{ id: string; label: string; icon?: string | null }>
  }

  async function handleSelect(item: { id: string; label: string; icon?: string | null }) {
    if (saving) return
    await onsave(propertyIri, [item.id])
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
      <Button variant="ghost" size="icon-sm" onclick={clear} disabled={saving} aria-label="Remover">
        <span class="material-symbols-outlined">close</span>
      </Button>
    </div>
  {/if}

  <EntitySearchCombobox
    {searchFn}
    debounceMs={200}
    onSelect={handleSelect}
    placeholder="Buscar {rangeClassLabel ?? 'entidade'}…"
    emptyText="Nenhuma entidade encontrada."
    {saving}
  />

  <div class="actions-row">
    <Button variant="ghost" size="sm" onclick={oncancel} disabled={saving}>
      <span class="material-symbols-outlined">close</span>
      Cancelar
    </Button>
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

  .actions-row {
    display: flex;
    align-items: center;
    justify-content: flex-end;
  }

</style>
