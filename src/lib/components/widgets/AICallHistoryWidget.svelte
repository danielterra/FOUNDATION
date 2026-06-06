<script>
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { createEntitySubscription } from '$lib/realtime/subscriptions';
  import WidgetContainer from './WidgetContainer.svelte';

  let { widgetId, windowState = 'normal', onWindowStateChange } = $props();

  let calls = $state([]);
  let loading = $state(false);
  let fromDate = $state('');
  let toDate = $state('');
  // Snapshot tx cursor: max(tx) at the last load. Used for lossless replay.
  let snapshotTx = 0;
  // Set of call IRIs already seen — prevents double-counting on replay.
  let seenCallIris = new Set();

  const entitySub = createEntitySubscription((event) => {
    if (event.type === 'joined-set') {
      // A new AIAPICall entered the set — add it to the history and update the summary.
      const iri = event.entityId;
      // Idempotency: skip if already known.
      if (seenCallIris.has(iri)) return;
      if (event.tx != null && event.tx <= snapshotTx) return;
      // Fetch the individual call via the entity inspector to build the call record.
      invoke('inspector__get_entity', { entityId: iri }).then((resultStr) => {
        if (seenCallIris.has(iri)) return;
        const data = JSON.parse(resultStr);
        const props = data?.properties ?? [];
        const getProp = (key) => props.find(p => p.property === key)?.value ?? null;
        const call = {
          iri,
          model: getProp('foundation:model') ?? '',
          calledAt: getProp('foundation:calledAt') ?? '',
          inputTokens: parseInt(getProp('foundation:inputTokens') ?? '0', 10) || 0,
          outputTokens: parseInt(getProp('foundation:outputTokens') ?? '0', 10) || 0,
          estimatedCost: parseFloat(getProp('foundation:estimatedCost') ?? '0') || 0,
          conversationIri: getProp('foundation:generatedByConversation') ?? null,
        };
        seenCallIris.add(iri);
        calls = [call, ...calls];
        entitySub.setIris([...seenCallIris]);
      }).catch(() => {});
    }
  });

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
      const [fetched, snapTx] = await Promise.all([
        invoke('ai__list_api_calls', { fromMs, toMs, limit: 200 }),
        invoke('chat__get_conversation_snapshot_tx', { conversationId: 'foundation:AIAPICall' }).catch(() => 0),
      ]);
      calls = fetched;
      seenCallIris = new Set(calls.map(c => c.iri).filter(Boolean));
      snapshotTx = /** @type {number} */ (snapTx);
      entitySub.setIris([...seenCallIris]);
      entitySub.setSinceTx(snapshotTx);
      entitySub.replayMissed();
    } catch (e) {
      calls = [];
      seenCallIris = new Set();
    } finally {
      loading = false;
    }
  }

  async function closeWidget() {
    try { await invoke('widget_blackboard__remove_widget', { widgetId }); } catch {}
  }

  onMount(async () => {
    // Register creation-query before loading to avoid birth-race.
    entitySub.setCreationQueries([{
      classIri: 'foundation:AIAPICall',
      predicate: 'rdf:type',
      objectValue: 'foundation:AIAPICall',
    }]);
    await load();
  });

  onDestroy(() => { entitySub.destroy(); });
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
  .ah-filters input { border: 1px solid var(--border-color, #ccc); border-radius: var(--radius); padding: 2px 4px; }
  .ah-hint { color: var(--text-muted, #888); margin: 0; }
  .ah-section-title { font-weight: 600; cursor: pointer; padding: 4px 0; user-select: none; }
  .ah-table { width: 100%; border-collapse: collapse; margin-top: 4px; }
  .ah-table th, .ah-table td { text-align: left; padding: 3px 6px; border-bottom: 1px solid var(--border-color, #eee); white-space: nowrap; }
  .ah-table th { font-weight: 600; color: var(--text-muted, #666); }
  .ah-model { max-width: 160px; overflow: hidden; text-overflow: ellipsis; }
  .ah-date { font-size: 0.9em; color: var(--text-muted, #888); }
  .ah-cost { font-weight: 600; }
</style>
