<script>
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import WidgetContainer from './WidgetContainer.svelte';

  let { widgetId, windowState = 'normal', onWindowStateChange } = $props();

  let calls = $state([]);
  let loading = $state(false);
  let fromDate = $state('');
  let toDate = $state('');
  let unlistenEntityUpdated = null;

  // Group calls by model for summary view
  let summary = $derived(() => {
    const map = {};
    for (const c of calls) {
      const key = c.model || 'unknown';
      if (!map[key]) map[key] = { model: key, count: 0, inputTokens: 0, outputTokens: 0, totalCost: 0 };
      map[key].count++;
      map[key].inputTokens += c.inputTokens ?? 0;
      map[key].outputTokens += c.outputTokens ?? 0;
      map[key].totalCost += c.estimatedCost ?? 0;
    }
    return Object.values(map).sort((a, b) => b.totalCost - a.totalCost);
  });

  function formatCost(cost) {
    if (cost == null) return '—';
    return `$${cost.toFixed(6)}`;
  }

  function formatTokens(n) {
    if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
    if (n >= 1_000) return `${(n / 1_000).toFixed(1)}k`;
    return String(n);
  }

  function formatDate(iso) {
    if (!iso) return '—';
    try { return new Date(iso).toLocaleString(); } catch { return iso; }
  }

  async function load() {
    loading = true;
    try {
      const fromMs = fromDate ? new Date(fromDate).getTime() : null;
      const toMs = toDate ? new Date(toDate + 'T23:59:59').getTime() : null;
      calls = await invoke('ai__list_api_calls', { fromMs, toMs, limit: 200 });
    } catch (e) {
      calls = [];
    } finally {
      loading = false;
    }
  }

  async function closeWidget() {
    try { await invoke('widget_blackboard__remove_widget', { widgetId }); } catch {}
  }

  onMount(async () => {
    await load();
    unlistenEntityUpdated = await listen('entity-updated', async (event) => {
      // Reload when a new AIAPICall is written
      if (String(event.payload?.entityId ?? '').includes('AIAPICall')) {
        await load();
      }
    });
  });

  onDestroy(() => { if (unlistenEntityUpdated) unlistenEntityUpdated(); });
</script>

<WidgetContainer
  icon="payments"
  title="Consumo de IA"
  {windowState}
  {onWindowStateChange}
  onClose={closeWidget}
>
  <div class="ah-root">
    <div class="ah-filters">
      <label>De <input type="date" bind:value={fromDate} onchange={load} /></label>
      <label>Até <input type="date" bind:value={toDate} onchange={load} /></label>
    </div>

    {#if loading}
      <p class="ah-hint">Carregando…</p>
    {:else if calls.length === 0}
      <p class="ah-hint">Nenhuma chamada registrada no período.</p>
    {:else}
      <details open>
        <summary class="ah-section-title">Resumo por modelo ({summary().length})</summary>
        <table class="ah-table">
          <thead><tr><th>Modelo</th><th>Chamadas</th><th>Tokens entrada</th><th>Tokens saída</th><th>Custo total</th></tr></thead>
          <tbody>
            {#each summary() as row}
              <tr>
                <td class="ah-model">{row.model}</td>
                <td>{row.count}</td>
                <td>{formatTokens(row.inputTokens)}</td>
                <td>{formatTokens(row.outputTokens)}</td>
                <td class="ah-cost">{formatCost(row.totalCost)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </details>

      <details>
        <summary class="ah-section-title">Histórico ({calls.length} chamadas)</summary>
        <table class="ah-table">
          <thead><tr><th>Data/Hora</th><th>Modelo</th><th>In</th><th>Out</th><th>Custo</th></tr></thead>
          <tbody>
            {#each calls as c}
              <tr>
                <td class="ah-date">{formatDate(c.calledAt)}</td>
                <td class="ah-model">{c.model || '—'}</td>
                <td>{formatTokens(c.inputTokens ?? 0)}</td>
                <td>{formatTokens(c.outputTokens ?? 0)}</td>
                <td class="ah-cost">{formatCost(c.estimatedCost)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </details>
    {/if}
  </div>
</WidgetContainer>

<style>
  .ah-root { padding: 8px; font-size: 0.82em; display: flex; flex-direction: column; gap: 8px; overflow: auto; }
  .ah-filters { display: flex; gap: 12px; align-items: center; flex-wrap: wrap; }
  .ah-filters label { display: flex; gap: 4px; align-items: center; }
  .ah-filters input { border: 1px solid var(--border-color, #ccc); border-radius: 4px; padding: 2px 4px; }
  .ah-hint { color: var(--text-muted, #888); margin: 0; }
  .ah-section-title { font-weight: 600; cursor: pointer; padding: 4px 0; user-select: none; }
  .ah-table { width: 100%; border-collapse: collapse; margin-top: 4px; }
  .ah-table th, .ah-table td { text-align: left; padding: 3px 6px; border-bottom: 1px solid var(--border-color, #eee); white-space: nowrap; }
  .ah-table th { font-weight: 600; color: var(--text-muted, #666); }
  .ah-model { max-width: 160px; overflow: hidden; text-overflow: ellipsis; }
  .ah-date { font-size: 0.9em; color: var(--text-muted, #888); }
  .ah-cost { font-weight: 600; }
</style>
