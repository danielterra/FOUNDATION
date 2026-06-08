<script>
  import { untrack } from 'svelte';
  import { Input } from '$lib/components/ui/input';
  import * as Select from '$lib/components/ui/select';

  let { value = '', onconfirm, oncancel } = $props();

  function parseConfig(json) {
    const empty = { targetClass: '', filters: [] };
    if (!json) return empty;
    try {
      const c = JSON.parse(json);
      return { targetClass: c.targetClass || '', filters: c.filters || [] };
    } catch {
      return empty;
    }
  }

  const init = untrack(() => parseConfig(value));
  let targetClass = $state(init.targetClass);
  let filters = $state(init.filters.map(f => ({ ...f })));

  const OPERATORS = [
    { value: 'eq', label: '= igual a' },
    { value: 'neq', label: '≠ diferente de' },
    { value: 'gt', label: '> maior que' },
    { value: 'lt', label: '< menor que' },
    { value: 'gte', label: '≥ maior ou igual' },
    { value: 'lte', label: '≤ menor ou igual' },
    { value: 'between', label: 'entre' },
    { value: 'exists', label: 'existe' },
    { value: 'not_exists', label: 'não existe' },
  ];

  function isBetween(op) { return op === 'between'; }
  function isUnary(op) { return op === 'exists' || op === 'not_exists'; }

  function addFilter() {
    filters = [...filters, { propertyIri: '', operator: 'eq', value: '', valueFrom: '', valueTo: '' }];
  }

  function removeFilter(i) {
    filters = filters.filter((_, idx) => idx !== i);
  }

  function buildJson() {
    const config = {
      targetClass,
      filters: filters.map(f => {
        const out = { propertyIri: f.propertyIri, operator: f.operator };
        if (isBetween(f.operator)) {
          if (f.valueFrom) out.valueFrom = f.valueFrom;
          if (f.valueTo) out.valueTo = f.valueTo;
        } else if (!isUnary(f.operator)) {
          if (f.value !== undefined && f.value !== '') out.value = f.value;
        }
        return out;
      }),
    };
    return JSON.stringify(config);
  }

  function confirm() { onconfirm(buildJson()); }
</script>

<div class="qc-editor">
  <div class="section">
    <label class="section-label" for="qc-target-class-input">Classe alvo</label>
    <Input
      id="qc-target-class-input"
      class="qc-input"
      type="text"
      bind:value={targetClass}
      placeholder="foundation:MinhaClasse"
    />
  </div>

  <div class="section">
    <div class="section-header">
      <span class="section-label">Filtros</span>
      <button class="add-btn" onclick={addFilter} type="button">
        <span class="material-symbols-outlined">add</span>
        Adicionar filtro
      </button>
    </div>

    {#if filters.length === 0}
      <div class="empty-filters">Sem filtros — retorna todos os indivíduos da classe</div>
    {:else}
      <div class="filters-list">
        {#each filters as filter, i}
          <div class="filter-row">
            <Input
              class="qc-input prop-input"
              type="text"
              bind:value={filter.propertyIri}
              placeholder="foundation:propriedade"
            />
            <Select.Root
              type="single"
              value={filter.operator}
              onValueChange={(v) => { if (v) filter.operator = v; }}
            >
              <Select.Trigger class="qc-select-trigger">
                <Select.Value />
              </Select.Trigger>
              <Select.Content>
                {#each OPERATORS as op}
                  <Select.Item value={op.value}>{op.label}</Select.Item>
                {/each}
              </Select.Content>
            </Select.Root>
            {#if isBetween(filter.operator)}
              <Input
                class="qc-input val-input"
                type="text"
                bind:value={filter.valueFrom}
                placeholder="de"
              />
              <span class="between-sep">e</span>
              <Input
                class="qc-input val-input"
                type="text"
                bind:value={filter.valueTo}
                placeholder="até"
              />
            {:else if !isUnary(filter.operator)}
              <Input
                class="qc-input val-input"
                type="text"
                bind:value={filter.value}
                placeholder={"valor ou {{self.prop}}"}
              />
            {/if}
            <button class="remove-btn" onclick={() => removeFilter(i)} type="button">
              <span class="material-symbols-outlined">remove</span>
            </button>
          </div>
        {/each}
      </div>
    {/if}
  </div>

  <div class="btn-row">
    <button class="btn-cancel" onclick={oncancel} type="button">Cancelar</button>
    <button class="btn-ok" onclick={confirm} type="button">OK</button>
  </div>
</div>

<style>
  .qc-editor {
    display: flex;
    flex-direction: column;
    gap: 16px;
    font-family: var(--font-body);
    font-size: 13px;
    color: var(--color-neutral-active);
    padding: 10px 0;
  }

  .section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .section-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .section-label {
    font-size: 11px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--color-neutral);
  }

  :global([data-slot="input"].qc-input) {
    background: color-mix(in srgb, var(--color-black) 50%, transparent);
    border: none;
    color: var(--color-neutral-active);
    font-family: var(--font-mono, monospace);
    font-size: 12px;
    padding: 5px 8px;
    height: auto;
    border-radius: 0;
    box-shadow: none;
    min-width: 0;
  }

  :global([data-slot="input"].qc-input::placeholder) {
    color: var(--color-neutral);
    opacity: 0.4;
    font-family: var(--font-body);
  }

  :global(.qc-select-trigger) {
    background: color-mix(in srgb, var(--color-white) 8%, transparent);
    border: none;
    color: var(--color-neutral-active);
    font-family: var(--font-body);
    font-size: 12px;
    padding: 5px 6px;
    cursor: pointer;
    flex-shrink: 0;
    height: auto;
    border-radius: 0;
    min-width: 120px;
  }

  .filters-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .filter-row {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  :global([data-slot="input"].prop-input) {
    flex: 2;
  }

  :global([data-slot="input"].val-input) {
    flex: 1;
  }

  .between-sep {
    font-size: 12px;
    color: var(--color-neutral);
    flex-shrink: 0;
    padding: 0 2px;
  }

  .add-btn {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    background: none;
    border: none;
    cursor: pointer;
    color: var(--color-interactive);
    font-family: var(--font-body);
    font-size: 12px;
    font-weight: 600;
    padding: 2px 4px;
  }

  .add-btn .material-symbols-outlined {
    font-size: 14px;
  }

  .remove-btn {
    background: none;
    border: none;
    cursor: pointer;
    color: var(--color-neutral);
    padding: 2px;
    display: flex;
    align-items: center;
    flex-shrink: 0;
  }

  .remove-btn .material-symbols-outlined {
    font-size: 14px;
  }

  .empty-filters {
    font-size: 12px;
    color: var(--color-neutral);
    opacity: 0.5;
    font-style: italic;
    padding: 4px 0;
  }

  .btn-row {
    display: flex;
    gap: 10px;
    justify-content: flex-end;
    margin-top: 4px;
  }

  .btn-cancel {
    padding: 7px 16px;
    border: none;
    background: transparent;
    color: var(--color-neutral);
    font-family: var(--font-body);
    font-size: 13px;
    cursor: pointer;
  }

  .btn-ok {
    padding: 7px 20px;
    border: none;
    background: var(--color-interactive);
    color: var(--color-neutral-active);
    font-family: var(--font-body);
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }
</style>
