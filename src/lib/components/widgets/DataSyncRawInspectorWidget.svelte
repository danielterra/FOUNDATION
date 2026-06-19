<script lang="ts">
  import { onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import WidgetContainer from './WidgetContainer.svelte';
  import { createEntitySubscription } from '$lib/realtime/subscriptions';

  let { widgetId, entityId, windowState = 'normal', onWindowStateChange } = $props<{
    widgetId: string;
    entityId: string;
    windowState?: string;
    onWindowStateChange?: (state: string) => void;
  }>();

  // ── Tipos ─────────────────────────────────────────────────────────────────

  interface RawRecord {
    iri: string;
    external_id: string | null;
    received_at: string | null;
    transform_status: string;
    retry_count: number | null;
  }

  interface ListResponse {
    items: RawRecord[];
    next_cursor: number | null;
    has_more: boolean;
    counts_by_status: Record<string, number>;
    snapshot_tx: number;
  }

  interface InspectResponse {
    iri: string;
    data_source_iri: string | null;
    external_id: string | null;
    transform_status: string;
    received_at: string | null;
    retry_count: number | null;
    raw_source_ref: string | null;
    raw_payload: string | null;
    raw_file_path: string | null;
    transform_error: string | null;
  }

  // ── Status IRIs e mapeamentos ─────────────────────────────────────────────

  const STATUS_PENDING     = 'foundation:Pending';
  const STATUS_TRANSFORMED = 'foundation:Status_1781300928585';
  const STATUS_ERROR       = 'foundation:Status_1781300928559';
  const STATUS_SKIPPED     = 'foundation:Status_1781300928612';

  type StatusKey = 'all' | 'pending' | 'transformed' | 'error' | 'skipped';

  interface FilterTab {
    key: StatusKey;
    label: string;
    iri: string | null;
  }

  const FILTER_TABS: FilterTab[] = [
    { key: 'all',         label: 'Todos',      iri: null             },
    { key: 'pending',     label: 'Pending',    iri: STATUS_PENDING   },
    { key: 'transformed', label: 'Transformed',iri: STATUS_TRANSFORMED },
    { key: 'error',       label: 'Error',      iri: STATUS_ERROR     },
    { key: 'skipped',     label: 'Skipped',    iri: STATUS_SKIPPED   },
  ];

  type PillVariant = 'neutral' | 'success' | 'error' | 'skipped' | 'pending';

  function statusPill(iri: string): { label: string; variant: PillVariant } {
    if (iri === STATUS_TRANSFORMED) return { label: 'Transformado', variant: 'success'  };
    if (iri === STATUS_ERROR)       return { label: 'Erro',         variant: 'error'    };
    if (iri === STATUS_SKIPPED)     return { label: 'Ignorado',     variant: 'skipped'  };
    if (iri === STATUS_PENDING)     return { label: 'Pendente',     variant: 'pending'  };
    return { label: iri, variant: 'neutral' };
  }

  function formatDate(iso: string | null): string {
    if (!iso) return '—';
    const d = new Date(iso);
    return isNaN(d.getTime()) ? iso : d.toLocaleString('pt-BR');
  }

  const RAW_PAYLOAD_TRUNCATE_BYTES = 10_000;

  // ── Estado ────────────────────────────────────────────────────────────────

  const PAGE_SIZE = 50;

  let records          = $state<RawRecord[]>([]);
  let countsByStatus   = $state<Record<string, number>>({});
  let loading          = $state(true);
  let loadError        = $state<string | null>(null);
  let activeFilter     = $state<StatusKey>('all');
  let cursorStack      = $state<(number | null)[]>([null]);
  let nextCursor       = $state<number | null>(null);
  let hasMore          = $state(false);

  let selectedIri      = $state<string | null>(null);
  let detail           = $state<InspectResponse | null>(null);
  let detailLoading    = $state(false);
  let detailError      = $state<string | null>(null);
  let payloadExpanded  = $state(false);

  let retryingMap      = $state<Record<string, boolean>>({});
  let retryError       = $state<Record<string, string | null>>({});

  // ── Contagem por aba (derivada) ───────────────────────────────────────────

  const tabCount = $derived<Record<StatusKey, number>>({
    all:         Object.values(countsByStatus).reduce((a, b) => a + b, 0),
    pending:     countsByStatus[STATUS_PENDING]     ?? 0,
    transformed: countsByStatus[STATUS_TRANSFORMED] ?? 0,
    error:       countsByStatus[STATUS_ERROR]       ?? 0,
    skipped:     countsByStatus[STATUS_SKIPPED]     ?? 0,
  });

  // ── Realtime ──────────────────────────────────────────────────────────────

  const entitySub = createEntitySubscription(async (event) => {
    if (event.type === 'joined-set') {
      if (!records.some(r => r.iri === event.entityId)) {
        await loadRecords();
      }
      return;
    }
    const affectedIri = event.entityId;
    if (records.some(r => r.iri === affectedIri)) {
      await loadRecords();
    }
    if (selectedIri === affectedIri && detail) {
      await loadDetail(affectedIri);
    }
  });

  $effect(() => {
    entitySub.setIris([entityId, ...records.map(r => r.iri)]);
    entitySub.setCreationQueries([
      {
        classIri:    'foundation:RawDataRecord',
        predicate:   'foundation:belongsToDataSource',
        objectValue: entityId,
      },
    ]);
  });

  // ── Carregamento ──────────────────────────────────────────────────────────

  // Coalesce concurrent triggers (filter clicks + realtime events) into at most
  // one in-flight call plus one queued re-run, so the widget never piles up
  // overlapping list queries against the read pool.
  let loadInFlight = false;
  let loadPending  = false;

  async function loadRecords() {
    if (loadInFlight) {
      loadPending = true;
      return;
    }
    loadInFlight = true;
    try {
      const filterIri = FILTER_TABS.find(t => t.key === activeFilter)?.iri ?? undefined;
      const afterTx = cursorStack[cursorStack.length - 1];
      const raw = await invoke<string>('datasync__list_raw', {
        dataSourceIri:   entityId,
        transformStatus: filterIri,
        limit:           PAGE_SIZE,
        afterTx:         afterTx ?? undefined,
      });
      const data: ListResponse = JSON.parse(raw);
      records        = data.items;
      nextCursor     = data.next_cursor;
      hasMore        = data.has_more;
      countsByStatus = data.counts_by_status;
      loadError      = null;
      loading        = false;

      entitySub.setSinceTx(data.snapshot_tx);
      entitySub.replayMissed();
    } catch (err) {
      loadError = String(err);
      loading   = false;
    } finally {
      loadInFlight = false;
      if (loadPending) {
        loadPending = false;
        loadRecords();
      }
    }
  }

  $effect(() => {
    activeFilter;
    cursorStack;
    loading = true;
    loadRecords();
  });

  // Total matching the active filter (from the aggregate counts).
  const filterTotal = $derived(tabCount[activeFilter] ?? 0);

  function goToFilter(key: StatusKey) {
    if (activeFilter === key) return;
    activeFilter = key;
    cursorStack = [null];
  }

  function prevPage() {
    if (cursorStack.length > 1) cursorStack = cursorStack.slice(0, -1);
  }

  function gotoNextPage() {
    if (hasMore && nextCursor !== null) cursorStack = [...cursorStack, nextCursor];
  }

  // ── Inspeção de detalhe ───────────────────────────────────────────────────

  async function loadDetail(iri: string) {
    detailLoading = true;
    detailError   = null;
    payloadExpanded = false;
    try {
      const raw = await invoke<string>('datasync__inspect_raw', { rawRecordIri: iri });
      detail = JSON.parse(raw);
    } catch (err) {
      detailError = String(err);
      detail      = null;
    } finally {
      detailLoading = false;
    }
  }

  function selectRecord(iri: string) {
    if (selectedIri === iri) {
      selectedIri = null;
      detail      = null;
    } else {
      selectedIri = iri;
      loadDetail(iri);
    }
  }

  // ── Reprocessamento ───────────────────────────────────────────────────────

  async function retryRecord(iri: string) {
    if (retryingMap[iri]) return;
    retryingMap = { ...retryingMap, [iri]: true };
    retryError  = { ...retryError,  [iri]: null  };
    try {
      await invoke('datasync__retry_raw', { rawRecordIri: iri });
    } catch (err) {
      retryError = { ...retryError, [iri]: String(err) };
    } finally {
      retryingMap = { ...retryingMap, [iri]: false };
    }
  }

  // ── Fechar widget ─────────────────────────────────────────────────────────

  function closeWidget() {
    invoke('widget_blackboard__remove_widget', { widgetId }).catch(() => {});
  }

  onDestroy(() => {
    entitySub.destroy();
  });
</script>

<WidgetContainer
  icon="manage_search"
  title="Registros Brutos"
  {windowState}
  {onWindowStateChange}
  {entityId}
  onClose={closeWidget}
>
  {#snippet headerActions()}
    <div class="filter-bar" role="tablist" aria-label="Filtrar por status">
      {#each FILTER_TABS as tab (tab.key)}
        <button
          class="filter-chip"
          class:active={activeFilter === tab.key}
          role="tab"
          aria-selected={activeFilter === tab.key}
          onclick={() => goToFilter(tab.key)}
          onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') goToFilter(tab.key); }}
        >
          {tab.label}
          <span class="chip-count">{tabCount[tab.key]}</span>
        </button>
      {/each}
    </div>
  {/snippet}

  <div class="raw-content">
    {#if loading && records.length === 0}
      <div class="state-center">
        <span class="material-symbols-outlined spinning">progress_activity</span>
        <span class="state-label">Carregando registros…</span>
      </div>
    {:else if loadError && records.length === 0}
      <div class="state-center state-error">
        <span class="material-symbols-outlined">error</span>
        <span class="state-label">{loadError}</span>
      </div>
    {:else if records.length === 0}
      <div class="state-center">
        <span class="material-symbols-outlined">inbox</span>
        <span class="state-label">Nenhum registro recebido por esta fonte.</span>
      </div>
    {:else}
      <div class="list-area">
        <table class="records-table" aria-label="Registros brutos">
          <thead>
            <tr>
              <th scope="col">externalId</th>
              <th scope="col">receivedAt</th>
              <th scope="col">transformStatus</th>
              <th scope="col">retryCount</th>
              <th scope="col" class="col-actions"><span class="sr-only">Ações</span></th>
            </tr>
          </thead>
          <tbody>
            {#each records as record (record.iri)}
              {@const pill = statusPill(record.transform_status)}
              {@const isSelected = selectedIri === record.iri}
              {@const isRetrying = retryingMap[record.iri] ?? false}
              <tr
                class="record-row"
                class:selected={isSelected}
                onclick={() => selectRecord(record.iri)}
                onkeydown={(e) => { if (e.key === 'Enter') selectRecord(record.iri); }}
                tabindex="0"
                aria-selected={isSelected}
              >
                <td class="cell-mono">{record.external_id ?? '—'}</td>
                <td class="cell-date">{formatDate(record.received_at)}</td>
                <td>
                  <span class="pill pill-{pill.variant}">{pill.label}</span>
                </td>
                <td class="cell-center">{record.retry_count ?? '—'}</td>
                <td class="cell-actions">
                  {#if record.transform_status === STATUS_ERROR}
                    <button
                      class="retry-btn"
                      class:retrying={isRetrying}
                      disabled={isRetrying}
                      onclick={(e) => { e.stopPropagation(); retryRecord(record.iri); }}
                      onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') { e.stopPropagation(); retryRecord(record.iri); } }}
                      aria-label="Reprocessar registro {record.external_id ?? record.iri}"
                      title="Reprocessar"
                    >
                      <span class="material-symbols-outlined" class:spinning={isRetrying}>
                        {isRetrying ? 'progress_activity' : 'replay'}
                      </span>
                    </button>
                  {/if}
                </td>
              </tr>
              {#if isSelected}
                <tr class="detail-row" aria-live="polite">
                  <td colspan="5">
                    <div class="detail-panel">
                      {#if detailLoading}
                        <div class="detail-loading">
                          <span class="material-symbols-outlined spinning">progress_activity</span>
                          <span>Carregando detalhes…</span>
                        </div>
                      {:else if detailError}
                        <div class="detail-error">
                          <span class="material-symbols-outlined">error</span>
                          <span>{detailError}</span>
                        </div>
                      {:else if detail}
                        <dl class="detail-fields">
                          <div class="detail-field">
                            <dt>external_id</dt>
                            <dd class="mono">{detail.external_id ?? '—'}</dd>
                          </div>
                          <div class="detail-field">
                            <dt>received_at</dt>
                            <dd>{formatDate(detail.received_at)}</dd>
                          </div>
                          <div class="detail-field">
                            <dt>retry_count</dt>
                            <dd>{detail.retry_count ?? '—'}</dd>
                          </div>
                          {#if detail.raw_source_ref}
                            <div class="detail-field">
                              <dt>raw_source_ref</dt>
                              <dd class="mono">{detail.raw_source_ref}</dd>
                            </div>
                          {/if}
                          {#if detail.raw_file_path}
                            <div class="detail-field">
                              <dt>raw_file_path</dt>
                              <dd class="mono">{detail.raw_file_path}</dd>
                            </div>
                          {/if}
                        </dl>

                        {#if detail.transform_error}
                          <div class="transform-error-block">
                            <div class="block-label error-label">
                              <span class="material-symbols-outlined">error</span>
                              transformError
                            </div>
                            <pre class="error-pre">{detail.transform_error}</pre>
                          </div>
                        {/if}

                        {#if detail.raw_payload !== null}
                          <div class="payload-block">
                            <div class="block-label">raw_payload</div>
                            {#if detail.raw_payload.length > RAW_PAYLOAD_TRUNCATE_BYTES && !payloadExpanded}
                              <pre class="payload-pre">{detail.raw_payload.slice(0, RAW_PAYLOAD_TRUNCATE_BYTES)}</pre>
                              <button
                                class="expand-btn"
                                onclick={() => { payloadExpanded = true; }}
                              >
                                Mostrar tudo ({Math.round(detail.raw_payload.length / 1024)} KB)
                              </button>
                            {:else}
                              <pre class="payload-pre">{detail.raw_payload}</pre>
                            {/if}
                          </div>
                        {/if}

                        {#if retryError[record.iri]}
                          <div class="retry-error-inline">
                            <span class="material-symbols-outlined">warning</span>
                            {retryError[record.iri]}
                          </div>
                        {/if}
                      {/if}
                    </div>
                  </td>
                </tr>
              {/if}
            {/each}
          </tbody>
        </table>
      </div>
      <div class="pager">
        <span class="pager-info">
          {filterTotal} registro{filterTotal !== 1 ? 's' : ''}
          {#if loading}<span class="material-symbols-outlined spinning pager-spin">progress_activity</span>{/if}
        </span>
        <div class="pager-controls">
          <button class="pager-btn" disabled={cursorStack.length <= 1} onclick={prevPage} aria-label="Página anterior" title="Página anterior">
            <span class="material-symbols-outlined">chevron_left</span>
          </button>
          <button class="pager-btn" disabled={!hasMore} onclick={gotoNextPage} aria-label="Próxima página" title="Próxima página">
            <span class="material-symbols-outlined">chevron_right</span>
          </button>
        </div>
      </div>
    {/if}
  </div>
</WidgetContainer>

<style>
  .raw-content {
    flex: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  /* ── Barra de filtros (chips/abas) ── */

  .filter-bar {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 6px 12px;
    flex-wrap: wrap;
  }

  .filter-chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 3px 10px;
    border-radius: 999px;
    font-size: 11px;
    font-weight: 500;
    cursor: pointer;
    border: none;
    background: color-mix(in srgb, var(--color-neutral) 10%, transparent);
    color: var(--color-neutral);
    font-family: inherit;
    transition: background 100ms ease, color 100ms ease;
  }

  .filter-chip:hover:not(.active) {
    background: color-mix(in srgb, var(--color-neutral) 20%, transparent);
  }

  .filter-chip.active {
    background: color-mix(in srgb, var(--color-interactive) 20%, transparent);
    color: var(--color-interactive);
  }

  .chip-count {
    font-size: 10px;
    font-weight: 700;
    opacity: 0.8;
  }

  /* ── Estados centralizados ── */

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

  /* ── Área de lista ── */

  .list-area {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
  }

  /* ── Paginação ── */

  .pager {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: 6px 12px;
    border-top: 1px solid var(--color-surface-3);
    background: var(--color-surface-1);
    font-size: 11px;
    color: var(--color-neutral-secondary);
  }

  .pager-info {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .pager-spin {
    font-size: 13px;
    opacity: 0.7;
  }

  .pager-controls {
    display: inline-flex;
    align-items: center;
    gap: 8px;
  }

  .pager-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    border-radius: var(--radius);
    border: none;
    background: color-mix(in srgb, var(--color-neutral) 10%, transparent);
    color: var(--color-neutral);
    cursor: pointer;
    font-family: inherit;
    transition: background 100ms ease;
  }

  .pager-btn:hover:not(:disabled) {
    background: color-mix(in srgb, var(--color-interactive) 18%, transparent);
    color: var(--color-interactive);
  }

  .pager-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .pager-btn .material-symbols-outlined {
    font-size: 16px;
  }

  .records-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }

  .records-table thead th {
    position: sticky;
    top: 0;
    background: var(--color-surface-1);
    padding: 7px 10px;
    text-align: left;
    font-size: 10px;
    font-weight: 600;
    color: var(--color-neutral-disabled);
    letter-spacing: 0.04em;
    text-transform: uppercase;
    border-bottom: 1px solid var(--color-surface-3);
    z-index: 1;
  }

  .col-actions {
    width: 36px;
  }

  .record-row {
    cursor: pointer;
    border-bottom: 1px solid color-mix(in srgb, var(--color-surface-3) 60%, transparent);
    transition: background 80ms ease;
  }

  .record-row:hover,
  .record-row:focus-visible {
    background: color-mix(in srgb, var(--color-interactive) 8%, transparent);
    outline: none;
  }

  .record-row.selected {
    background: color-mix(in srgb, var(--color-interactive) 12%, transparent);
  }

  .record-row td {
    padding: 7px 10px;
    vertical-align: middle;
    color: var(--color-neutral);
  }

  .cell-mono {
    font-family: var(--font-mono, monospace);
    font-size: 11px;
    max-width: 160px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .cell-date {
    white-space: nowrap;
    color: var(--color-neutral-secondary);
    font-size: 11px;
  }

  .cell-center {
    text-align: center;
  }

  .cell-actions {
    text-align: right;
    padding-right: 8px;
    width: 36px;
  }

  /* ── Pills de status ── */

  .pill {
    display: inline-flex;
    align-items: center;
    padding: 2px 8px;
    border-radius: 999px;
    font-size: 10px;
    font-weight: 600;
    line-height: 1.4;
    white-space: nowrap;
  }

  .pill-success {
    background: color-mix(in srgb, var(--color-success) 18%, transparent);
    color: var(--color-success);
  }

  .pill-error {
    background: color-mix(in srgb, var(--color-error) 18%, transparent);
    color: var(--color-error);
  }

  .pill-pending {
    background: color-mix(in srgb, var(--color-warning) 18%, transparent);
    color: var(--color-warning);
  }

  .pill-skipped {
    background: color-mix(in srgb, var(--color-neutral) 15%, transparent);
    color: var(--color-neutral-secondary);
  }

  .pill-neutral {
    background: color-mix(in srgb, var(--color-neutral) 12%, transparent);
    color: var(--color-neutral);
  }

  /* ── Botão de retry ── */

  .retry-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 26px;
    height: 26px;
    border-radius: var(--radius);
    border: none;
    background: color-mix(in srgb, var(--color-warning) 15%, transparent);
    color: var(--color-warning);
    cursor: pointer;
    font-family: inherit;
    transition: background 100ms ease;
  }

  .retry-btn:hover:not(:disabled) {
    background: color-mix(in srgb, var(--color-warning) 28%, transparent);
  }

  .retry-btn:disabled,
  .retry-btn.retrying {
    opacity: 0.6;
    cursor: not-allowed;
  }

  .retry-btn .material-symbols-outlined {
    font-size: 16px;
  }

  /* ── Painel de detalhe (inline sob a linha) ── */

  .detail-row td {
    padding: 0;
    background: var(--color-surface-0);
  }

  .detail-panel {
    padding: 12px 14px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    border-bottom: 1px solid var(--color-surface-3);
  }

  .detail-loading,
  .detail-error {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--color-neutral-secondary);
    padding: 4px 0;
  }

  .detail-error {
    color: var(--color-error);
  }

  .detail-loading .material-symbols-outlined,
  .detail-error .material-symbols-outlined {
    font-size: 16px;
  }

  .detail-fields {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: 6px 16px;
    margin: 0;
    padding: 0;
  }

  .detail-field {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .detail-field dt {
    font-size: 10px;
    font-weight: 600;
    color: var(--color-neutral-disabled);
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .detail-field dd {
    font-size: 12px;
    color: var(--color-neutral);
    margin: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .detail-field dd.mono {
    font-family: var(--font-mono, monospace);
    font-size: 11px;
  }

  /* ── Bloco de transformError ── */

  .transform-error-block {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .block-label {
    font-size: 10px;
    font-weight: 600;
    color: var(--color-neutral-disabled);
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }

  .error-label {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--color-error);
  }

  .error-label .material-symbols-outlined {
    font-size: 14px;
  }

  .error-pre {
    margin: 0;
    padding: 8px 10px;
    background: color-mix(in srgb, var(--color-error) 10%, transparent);
    border-radius: var(--radius);
    color: var(--color-error);
    font-family: var(--font-mono, monospace);
    font-size: 11px;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-all;
    max-height: 120px;
    overflow-y: auto;
  }

  /* ── Bloco de raw_payload ── */

  .payload-block {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .payload-pre {
    margin: 0;
    padding: 8px 10px;
    background: var(--color-surface-2);
    border-radius: var(--radius);
    color: var(--color-neutral);
    font-family: var(--font-mono, monospace);
    font-size: 11px;
    line-height: 1.5;
    white-space: pre-wrap;
    word-break: break-all;
    max-height: 200px;
    overflow-y: auto;
  }

  .expand-btn {
    align-self: flex-start;
    font-size: 11px;
    font-weight: 600;
    color: var(--color-interactive);
    background: none;
    border: none;
    padding: 0;
    cursor: pointer;
    font-family: inherit;
  }

  .expand-btn:hover {
    color: var(--color-interactive-hover);
  }

  /* ── Erro inline de retry ── */

  .retry-error-inline {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--color-error);
    background: color-mix(in srgb, var(--color-error) 10%, transparent);
    border-radius: var(--radius);
    padding: 6px 10px;
  }

  .retry-error-inline .material-symbols-outlined {
    font-size: 14px;
    flex-shrink: 0;
  }

  /* ── Animação de rotação ── */

  .spinning {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }

  /* ── Acessibilidade ── */

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }
</style>
