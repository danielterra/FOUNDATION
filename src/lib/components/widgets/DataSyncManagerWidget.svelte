<script lang="ts">
  import { onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import WidgetContainer from './WidgetContainer.svelte';
  import { createEntitySubscription } from '$lib/realtime/subscriptions';

  let { widgetId, conversationIri = null, windowState = 'normal', onWindowStateChange } = $props<{
    widgetId: string;
    conversationIri?: string | null;
    windowState?: string;
    onWindowStateChange?: (state: string) => void;
  }>();

  // ── Tipos ─────────────────────────────────────────────────────────────────

  interface SourceCounts {
    pending: number;
    error: number;
    transformed: number;
    skipped: number;
  }

  interface DataSource {
    iri: string;
    label: string;
    status: string;
    is_connected: boolean;
    last_connection_error: string | null;
    sync_direction: string | null;
    transport_kind: string | null;
    sync_schedule: string | null;
    counts: SourceCounts;
  }

  interface ListResponse {
    snapshot_tx: number;
    sources: DataSource[];
  }

  // ── Tradução de status (sem jargão técnico) ────────────────────────────

  const STATUS_ACTIVE  = 'foundation:Status_1781300928499';
  const STATUS_PAUSED  = 'foundation:Status_1781300928524';
  const STATUS_ERROR   = 'foundation:Status_1781300928559';

  interface StatusDisplay {
    label: string;
    colorClass: 'status-ok' | 'status-neutral' | 'status-error';
    icon: string;
  }

  function resolveStatus(source: DataSource): StatusDisplay {
    if (!source.is_connected) {
      return { label: 'Com problema', colorClass: 'status-error', icon: 'warning' };
    }
    if (source.status === STATUS_ACTIVE)  return { label: 'Conectado',  colorClass: 'status-ok',      icon: 'check_circle' };
    if (source.status === STATUS_PAUSED)  return { label: 'Pausado',    colorClass: 'status-neutral',  icon: 'pause_circle' };
    if (source.status === STATUS_ERROR)   return { label: 'Com problema', colorClass: 'status-error',  icon: 'warning' };
    return { label: 'Desconhecido', colorClass: 'status-neutral', icon: 'help' };
  }

  function resolveDirection(dir: string | null): string {
    if (dir === 'in')   return 'Recebe dados';
    if (dir === 'out')  return 'Envia dados';
    if (dir === 'both') return 'Sincroniza nos dois sentidos';
    return 'Sincronização';
  }

  // ── Estado ────────────────────────────────────────────────────────────────

  let sources   = $state<DataSource[]>([]);
  let loading   = $state(true);
  let loadError = $state<string | null>(null);

  // Estado por fonte: chave = IRI
  let syncingMap  = $state<Record<string, boolean>>({});
  let syncResult  = $state<Record<string, { ok: boolean; text: string } | null>>({});

  // ── Realtime ──────────────────────────────────────────────────────────────

  let debounceTimer: ReturnType<typeof setTimeout> | undefined;

  const entitySub = createEntitySubscription((_event) => {
    clearTimeout(debounceTimer);
    debounceTimer = setTimeout(loadSources, 350);
  });

  $effect(() => {
    entitySub.setIris(sources.map(s => s.iri));
    entitySub.setCreationQueries([
      { classIri: 'foundation:DataSource',    predicate: 'rdf:type', objectValue: 'foundation:DataSource' },
      { classIri: 'foundation:RawDataRecord', predicate: 'rdf:type', objectValue: 'foundation:RawDataRecord' },
      { classIri: 'foundation:SyncRecord',    predicate: 'rdf:type', objectValue: 'foundation:SyncRecord' },
    ]);
  });

  // ── Carregamento ──────────────────────────────────────────────────────────

  async function loadSources() {
    try {
      const raw = await invoke<string>('datasync__list_sources');
      const data: ListResponse = JSON.parse(raw);
      sources   = data.sources;
      loadError = null;
      loading   = false;

      entitySub.setSinceTx(data.snapshot_tx);
      entitySub.replayMissed();
    } catch (err) {
      loadError = 'Não foi possível carregar as fontes de dados. Tente novamente em instantes.';
      loading   = false;
    }
  }

  loadSources();

  // ── Ação de sincronização ─────────────────────────────────────────────────

  async function triggerSync(iri: string) {
    if (syncingMap[iri]) return;
    syncingMap  = { ...syncingMap,  [iri]: true };
    syncResult  = { ...syncResult,  [iri]: null };
    try {
      await invoke('datasync__run', { dataSourceIri: iri });
      syncResult = { ...syncResult, [iri]: { ok: true, text: 'Sincronização iniciada.' } };
    } catch (err) {
      syncResult = { ...syncResult, [iri]: { ok: false, text: String(err) } };
    } finally {
      syncingMap = { ...syncingMap, [iri]: false };
    }
  }

  // ── Fechar widget ─────────────────────────────────────────────────────────

  async function closeWidget() {
    invoke('widget_blackboard__remove_widget', { widgetId }).catch(() => {});
  }

  onDestroy(() => {
    entitySub.destroy();
    clearTimeout(debounceTimer);
  });
</script>

<WidgetContainer
  icon="sync"
  title="Sincronização de Dados"
  {windowState}
  {onWindowStateChange}
  onClose={closeWidget}
>
  <div class="sync-content">
    {#if loading}
      <div class="state-center">
        <span class="material-symbols-outlined spinning">progress_activity</span>
        <span class="state-label">Carregando fontes…</span>
      </div>
    {:else if loadError}
      <div class="state-center state-error">
        <span class="material-symbols-outlined">error</span>
        <span class="state-label">{loadError}</span>
      </div>
    {:else if sources.length === 0}
      <div class="state-center">
        <span class="material-symbols-outlined">cloud_off</span>
        <span class="state-label">Nenhuma fonte de dados conectada ainda.</span>
      </div>
    {:else}
      <ul class="source-list" role="list">
        {#each sources as source (source.iri)}
          {@const display = resolveStatus(source)}
          <li class="source-row">
            <div class="source-header">
              <div class="source-meta">
                <span class="source-name">{source.label}</span>
                <div class="pills">
                  <span class="pill pill-{display.colorClass}">
                    <span class="material-symbols-outlined pill-icon">{display.icon}</span>
                    {display.label}
                  </span>
                  <span class="pill pill-neutral">
                    {resolveDirection(source.sync_direction)}
                  </span>
                </div>
              </div>
              <button
                class="sync-btn"
                class:syncing={syncingMap[source.iri]}
                disabled={syncingMap[source.iri]}
                onclick={() => triggerSync(source.iri)}
                aria-label="Sincronizar {source.label} agora"
                title="Sincronizar agora"
              >
                <span class="material-symbols-outlined" class:spinning={syncingMap[source.iri]}>
                  {syncingMap[source.iri] ? 'progress_activity' : 'sync'}
                </span>
                <span>{syncingMap[source.iri] ? 'Sincronizando…' : 'Sincronizar agora'}</span>
              </button>
            </div>

            <!-- Contadores de pendentes e erros (protagonistas) -->
            {#if source.counts.pending > 0 || source.counts.error > 0}
              <div class="counters">
                {#if source.counts.error > 0}
                  <span class="counter counter-error">
                    <span class="material-symbols-outlined counter-icon">error</span>
                    {source.counts.error} com erro
                  </span>
                {/if}
                {#if source.counts.pending > 0}
                  <span class="counter counter-pending">
                    <span class="material-symbols-outlined counter-icon">pending</span>
                    {source.counts.pending} pendente{source.counts.pending !== 1 ? 's' : ''}
                  </span>
                {/if}
              </div>
            {:else}
              <div class="counters">
                <span class="counter counter-quiet">
                  <span class="material-symbols-outlined counter-icon">check_circle</span>
                  {source.counts.transformed} item{source.counts.transformed !== 1 ? 's' : ''} sincronizado{source.counts.transformed !== 1 ? 's' : ''}
                </span>
              </div>
            {/if}

            <!-- Erro de conexão quando presente -->
            {#if source.last_connection_error}
              <p class="connection-error">{source.last_connection_error}</p>
            {/if}

            <!-- Feedback da ação de sync -->
            {#if syncResult[source.iri]}
              {@const res = syncResult[source.iri]!}
              <div class="sync-feedback" class:feedback-ok={res.ok} class:feedback-err={!res.ok}>
                <span class="material-symbols-outlined feedback-icon">
                  {res.ok ? 'check_circle' : 'error'}
                </span>
                <span>{res.text}</span>
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</WidgetContainer>

<style>
  .sync-content {
    flex: 1;
    overflow-y: auto;
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  /* ── Estados centralizados (loading / vazio / erro) ── */

  .state-center {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 10px;
    color: var(--color-neutral);
    padding: 32px 16px;
  }

  .state-center .material-symbols-outlined {
    font-size: 36px;
    opacity: 0.5;
  }

  .state-error {
    color: var(--color-error);
  }

  .state-error .material-symbols-outlined {
    opacity: 1;
  }

  .state-label {
    font-size: 13px;
    text-align: center;
    line-height: 1.5;
  }

  /* ── Lista de fontes ── */

  .source-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .source-row {
    background: var(--color-surface-2);
    border-radius: var(--radius);
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  /* ── Cabeçalho da linha (nome + pills + botão) ── */

  .source-header {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
  }

  .source-meta {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
    flex: 1;
  }

  .source-name {
    font-size: 13px;
    font-weight: 600;
    color: var(--color-neutral-active);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  /* ── Pills (chips arredondados conforme convenção do projeto) ── */

  .pills {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }

  .pill {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    padding: 2px 8px;
    border-radius: 999px;
    font-size: 11px;
    font-weight: 500;
    line-height: 1.4;
  }

  .pill-icon {
    font-size: 12px;
  }

  .pill-status-ok {
    background: color-mix(in srgb, var(--color-success) 18%, transparent);
    color: var(--color-success);
  }

  .pill-status-neutral {
    background: color-mix(in srgb, var(--color-neutral) 15%, transparent);
    color: var(--color-neutral);
  }

  .pill-status-error {
    background: color-mix(in srgb, var(--color-error) 18%, transparent);
    color: var(--color-error);
  }

  .pill-neutral {
    background: color-mix(in srgb, var(--color-neutral) 12%, transparent);
    color: var(--color-neutral);
  }

  /* ── Botão de sincronizar ── */

  .sync-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 5px 10px;
    border-radius: var(--radius);
    background: color-mix(in srgb, var(--color-interactive) 15%, transparent);
    color: var(--color-interactive);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    flex-shrink: 0;
    transition: background 120ms ease;
    border: none;
    font-family: inherit;
  }

  .sync-btn:hover:not(:disabled) {
    background: color-mix(in srgb, var(--color-interactive) 25%, transparent);
  }

  .sync-btn:disabled,
  .sync-btn.syncing {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .sync-btn .material-symbols-outlined {
    font-size: 15px;
  }

  /* ── Contadores ── */

  .counters {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .counter {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
  }

  .counter-icon {
    font-size: 14px;
  }

  .counter-error {
    color: var(--color-error);
    font-weight: 600;
  }

  .counter-pending {
    color: var(--color-warning);
    font-weight: 600;
  }

  .counter-quiet {
    color: var(--color-neutral);
  }

  /* ── Erro de conexão ── */

  .connection-error {
    margin: 0;
    font-size: 11px;
    color: var(--color-error);
    background: color-mix(in srgb, var(--color-error) 10%, transparent);
    border-radius: var(--radius);
    padding: 6px 10px;
    line-height: 1.5;
  }

  /* ── Feedback inline de ação ── */

  .sync-feedback {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    border-radius: var(--radius);
    padding: 6px 10px;
  }

  .feedback-icon {
    font-size: 14px;
    flex-shrink: 0;
  }

  .feedback-ok {
    background: color-mix(in srgb, var(--color-success) 12%, transparent);
    color: var(--color-success);
  }

  .feedback-err {
    background: color-mix(in srgb, var(--color-error) 12%, transparent);
    color: var(--color-error);
  }

  /* ── Animação de rotação (progress_activity) ── */

  .spinning {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }
</style>
