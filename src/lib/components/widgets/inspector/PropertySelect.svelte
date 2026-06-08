<script>
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';

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
      <button class="clear-btn" onclick={clear} disabled={saving} title="Remover" aria-label="Remover propriedade selecionada">
        <span class="material-symbols-outlined">close</span>
      </button>
    </div>
  {/if}

  <div class="header-row">
    <span class="hint">Propriedades da controlClass</span>
    <button class="cancel-btn" onclick={oncancel} disabled={saving} aria-label="Cancelar">
      <span class="material-symbols-outlined">close</span>
    </button>
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
        <button
          class="result"
          class:selected={currentValue?.iri === prop.iri}
          onclick={() => pick(prop.iri)}
          disabled={saving}
        >
          <span class="material-symbols-outlined">{prop.icon ?? 'output'}</span>
          <span class="result-label">{prop.label ?? prop.iri}</span>
          {#if prop.range}
            <span class="result-range">{prop.range.split(':').pop()}</span>
          {/if}
        </button>
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

  .clear-btn {
    padding: 2px 4px;
    background: transparent;
    border: none;
    color: var(--color-neutral);
    cursor: pointer;
    display: flex;
    align-items: center;
  }

  .clear-btn .material-symbols-outlined { font-size: 14px; }

  .clear-btn:hover:not(:disabled) {
    color: var(--color-danger);
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

  .cancel-btn {
    padding: 4px 8px;
    background: color-mix(in srgb, var(--color-neutral) 12%, transparent);
    border: none;
    color: var(--color-neutral);
    cursor: pointer;
    display: flex;
    align-items: center;
  }

  .cancel-btn .material-symbols-outlined { font-size: 16px; }

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

  .result.selected {
    background: color-mix(in srgb, var(--color-interactive) 20%, transparent);
  }

  .result .material-symbols-outlined {
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
