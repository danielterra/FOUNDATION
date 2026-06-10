<script>
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { Button } from '$lib/components/ui/button';

  let {
    nodeIri,
    propertyIri,
    currentValue = null,
    saving = false,
    onsave,
    oncancel,
  } = $props();

  let loading = $state(true);
  let properties = $state([]);
  let controlClassResolved = $state(true);
  let error = $state(null);

  onMount(async () => {
    try {
      const raw = await invoke('inspector__get_applicable_properties', { nodeIri });
      const result = JSON.parse(raw);
      controlClassResolved = result.controlClassResolved;
      properties = result.properties ?? [];
    } catch (err) {
      error = String(err);
    } finally {
      loading = false;
    }
  });

  async function pick(propIri) {
    if (saving) return;
    await onsave(propertyIri, [propIri]);
  }

  async function clear() {
    if (saving) return;
    await onsave(propertyIri, []);
  }
</script>

<div class="property-select">
  {#if currentValue}
    <div class="current-value">
      <span class="material-symbols-outlined current-icon">output</span>
      <span class="current-label">{currentValue.label ?? currentValue.iri}</span>
      <Button variant="ghost" size="icon-sm" onclick={clear} disabled={saving} title="Remover" aria-label="Remover propriedade selecionada">
        <span class="material-symbols-outlined">close</span>
      </Button>
    </div>
  {/if}

  <div class="header-row">
    <span class="hint">Propriedades da controlClass</span>
    <Button variant="ghost" size="icon-sm" onclick={oncancel} disabled={saving} aria-label="Cancelar">
      <span class="material-symbols-outlined">close</span>
    </Button>
  </div>

  {#if loading}
    <div class="state-msg">Carregando...</div>
  {:else if error}
    <div class="state-msg state-error">{error}</div>
  {:else if !controlClassResolved}
    <div class="state-msg state-warn">Configure a controlClass da automação primeiro.</div>
  {:else if properties.length === 0}
    <div class="state-msg">Nenhuma propriedade encontrada para a controlClass.</div>
  {:else}
    <div class="dropdown">
      {#each properties as prop (prop.iri)}
        <Button
          variant="ghost"
          class={`result${currentValue?.iri === prop.iri ? ' selected' : ''}`}
          onclick={() => pick(prop.iri)}
          disabled={saving}
        >
          <span class="material-symbols-outlined">{prop.icon ?? 'output'}</span>
          <span class="result-label">{prop.label ?? prop.iri}</span>
          {#if prop.range}
            <span class="result-range">{prop.range.split(':').pop()}</span>
          {/if}
        </Button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .property-select {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 10px;
  }

  .current-value {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px;
    background: color-mix(in srgb, var(--color-interactive) 12%, transparent);
    border-radius: var(--radius);
  }

  .current-icon {
    font-size: 14px;
    color: var(--color-interactive);
  }

  .current-label {
    flex: 1;
    font-size: 13px;
    color: var(--color-neutral-active);
    font-family: var(--font-body);
  }

  .header-row {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .hint {
    flex: 1;
    font-size: 11px;
    color: color-mix(in srgb, var(--color-neutral) 60%, transparent);
  }

  .state-msg {
    font-size: 12px;
    color: color-mix(in srgb, var(--color-neutral) 60%, transparent);
    padding: 6px 2px;
    font-family: var(--font-body);
  }

  .state-error {
    color: var(--color-error);
  }

  .state-warn {
    color: var(--color-warning);
  }

  .dropdown {
    display: flex;
    flex-direction: column;
    background: var(--color-surface-1);
    border: 1px solid color-mix(in srgb, var(--color-neutral) 20%, transparent);
    max-height: 200px;
    overflow-y: auto;
  }

  :global([data-slot="button"].result) {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    height: auto;
    justify-content: flex-start;
    color: var(--color-neutral-active);
    font-family: var(--font-body);
    font-size: 13px;
    border-radius: 0;
    width: 100%;
  }

  :global([data-slot="button"].result:hover:not(:disabled)) {
    background: color-mix(in srgb, var(--color-interactive) 15%, transparent);
  }

  :global([data-slot="button"].result.selected) {
    background: color-mix(in srgb, var(--color-interactive) 20%, transparent);
  }

  :global([data-slot="button"].result .material-symbols-outlined) {
    font-size: 16px;
    color: var(--color-interactive);
  }

  .result-label {
    flex: 1;
  }

  .result-range {
    font-size: 11px;
    color: color-mix(in srgb, var(--color-neutral) 55%, transparent);
  }
</style>
