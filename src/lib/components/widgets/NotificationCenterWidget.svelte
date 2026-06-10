<script>
  import { onMount, onDestroy, untrack } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { createEntitySubscription } from '$lib/realtime/subscriptions';
  import { openEntity } from '$lib/blackboard';
  import WidgetContainer from './WidgetContainer.svelte';
  import { Button } from '$lib/components/ui/button';
  import * as Select from '$lib/components/ui/select';
  import { Checkbox } from '$lib/components/ui/checkbox';

  let { widgetId, windowState = 'normal', onWindowStateChange, conversationIri = null } = $props();

  let notifications = $state([]);
  let loading = $state(true);
  let error = $state(null);
  let expandedIri = $state(null);
  let resolving = $state(false);
  let filterType = $state('all');
  let filterStatus = $state('pending');
  let selectedIris = $state({});
  // Snapshot tx cursor: max(tx) at the time of the last loadNotifications call.
  let snapshotTx = 0;

  const entitySub = createEntitySubscription(async (event) => {
    if (event.type === 'joined-set') {
      // A new AINotification entered the set — add it to the top and subscribe to updates.
      const iri = event.entityId;
      // Idempotency: skip if already in the list.
      if (notifications.some(n => n.iri === iri)) return;
      // Idempotency by tx cursor.
      if (event.tx != null && event.tx <= snapshotTx) return;
      await upsertNotification(iri);
      // Add the new IRI to the watched set so updates to it are received.
      const currentIris = notifications.map(n => n.iri);
      entitySub.setIris(currentIris);
      return;
    }
    if (event.type !== 'updated') return;
    pendingUpserts.add(event.entityId);
    clearTimeout(upsertTimer);
    upsertTimer = setTimeout(() => {
      const batch = [...pendingUpserts];
      pendingUpserts.clear();
      for (const iri of batch) upsertNotification(iri);
    }, 250);
  });
  let upsertTimer = null;
  let pendingUpserts = new Set();

  const STATUS_PENDING = 'foundation:Pending';
  const STATUS_COMPLETED = 'foundation:Completed';

  const TYPE_FILTERS = [
    { value: 'all', label: 'Todos os tipos' },
    { value: 'error', label: 'Erro' },
    { value: 'warning', label: 'Aviso' },
    { value: 'info', label: 'Info' },
  ];

  const STATUS_FILTERS = [
    { value: 'all', label: 'Todos os status' },
    { value: 'pending', label: 'Pendentes' },
    { value: 'resolved', label: 'Resolvidas' },
  ];

  const filteredNotifications = $derived(
    notifications.filter(n => filterType === 'all' || n.type === filterType)
  );

  function statusIriForFilter(filter) {
    if (filter === 'pending') return STATUS_PENDING;
    if (filter === 'resolved') return STATUS_COMPLETED;
    return null;
  }

  const TYPE_ICONS = {
    error: 'error',
    warning: 'warning',
    info: 'info_i',
  };

  const SOURCE_PROPERTIES = [
    'foundation:notificationSource',
    'foundation:derivedFromEmail',
  ];

  function collectSources(props) {
    const seen = new Set();
    return SOURCE_PROPERTIES.flatMap(propIri =>
      findAllProps(props, propIri)
        .filter(p => p.value && p.value.trim() !== '' && !seen.has(p.value) && seen.add(p.value))
        .map(p => ({ iri: p.value, label: p.value_label ?? p.value, icon: p.value_icon ?? 'link' }))
    );
  }

  function findProp(props, iri) {
    if (!Array.isArray(props)) return null;
    return props.find(p => p.property === iri) ?? null;
  }

  function findAllProps(props, iri) {
    if (!Array.isArray(props)) return [];
    return props.filter(p => p.property === iri);
  }

  function formatAbsoluteDate(iso) {
    if (!iso) return '';
    const t = Date.parse(iso);
    if (isNaN(t)) return iso;
    return new Date(t).toLocaleString();
  }

  function formatRelativeDate(timestampMs) {
    if (!timestampMs) return '';
    const diffMs = Date.now() - timestampMs;
    if (diffMs < 0) return 'agora';
    const sec = Math.floor(diffMs / 1000);
    if (sec < 60) return 'agora';
    const min = Math.floor(sec / 60);
    if (min < 60) return `${min}min atrás`;
    const hr = Math.floor(min / 60);
    if (hr < 24) return `${hr}h atrás`;
    const days = Math.floor(hr / 24);
    if (days < 7) return `${days}d atrás`;
    const weeks = Math.floor(days / 7);
    if (weeks < 5) return `${weeks}sem atrás`;
    const months = Math.floor(days / 30);
    if (months < 12) return `${months}mês atrás`;
    const years = Math.floor(days / 365);
    return `${years}a atrás`;
  }

  function timestampFromIri(iri) {
    const m = /_(\d{12,})/.exec(iri ?? '');
    return m ? parseInt(m[1], 10) : 0;
  }

  function resolveTimestamp(notif) {
    if (notif.createdAt) {
      const t = Date.parse(notif.createdAt);
      if (!isNaN(t)) return t;
    }
    if (notif.lastUpdatedAt) {
      const t = Date.parse(notif.lastUpdatedAt);
      if (!isNaN(t)) return t;
    }
    return timestampFromIri(notif.iri);
  }

  async function fetchNotificationDetails(iri) {
    try {
      const resultStr = await invoke('inspector__get_entity', { entityId: iri });
      const data = JSON.parse(resultStr);
      const props = data?.properties ?? [];

      const typeProp = findProp(props, 'foundation:notificationType');
      const createdAtProp = findProp(props, 'foundation:createdAt');
      const lastUpdatedAtProp = findProp(props, 'foundation:lastUpdatedAt');

      const typeRaw = (typeProp?.value ?? 'info').toLowerCase();
      const normalizedType = TYPE_ICONS[typeRaw] ? typeRaw : 'info';

      return {
        iri,
        title: data?.label ?? iri,
        body: data?.comment ?? '',
        type: normalizedType,
        sources: collectSources(props),
        createdAt: createdAtProp?.value ?? null,
        lastUpdatedAt: lastUpdatedAtProp?.value ?? null,
        status: data?.status ?? null,
      };
    } catch (err) {
      console.error('[NotificationCenter] Failed to load', iri, err);
      return null;
    }
  }

  function updateLocalStatus(iri, statusIri) {
    const idx = notifications.findIndex(n => n.iri === iri);
    if (idx >= 0) {
      const next = notifications.slice();
      next[idx] = { ...next[idx], status: { iri: statusIri } };
      notifications = next;
    }
  }

  async function fetchBatched(iris, batchSize = 10) {
    const results = [];
    for (let i = 0; i < iris.length; i += batchSize) {
      const batch = iris.slice(i, i + batchSize);
      const batchResults = await Promise.all(batch.map(fetchNotificationDetails));
      results.push(...batchResults);
    }
    return results;
  }

  async function loadNotifications() {
    loading = true;
    error = null;
    try {
      const [iris, snapTx] = await Promise.all([
        invoke('notification__list_iris', { statusIri: statusIriForFilter(filterStatus) }),
        invoke('chat__get_conversation_snapshot_tx', { conversationId: 'foundation:AINotification' }).catch(() => 0),
      ]);
      const details = await fetchBatched(iris);
      notifications = details
        .filter(n => n !== null)
        .map(n => ({ ...n, _ts: resolveTimestamp(n) }))
        .sort((a, b) => b._ts - a._ts);
      // Declare the flat set of notification IRIs for atomic updates.
      entitySub.setIris(notifications.map(n => n.iri));
      snapshotTx = /** @type {number} */ (snapTx);
      entitySub.setSinceTx(snapshotTx);
      entitySub.replayMissed();
    } catch (err) {
      error = String(err);
    } finally {
      loading = false;
    }
  }

  function isPending(notification) {
    return notification.status?.iri === STATUS_PENDING;
  }

  function isResolved(notification) {
    return notification.status?.iri === STATUS_COMPLETED;
  }

  function toggleExpand(iri) {
    expandedIri = expandedIri === iri ? null : iri;
  }

  async function setNotificationStatus(iri, statusIri) {
    try {
      await invoke('widget_inspector__update_status', { entityId: iri, statusIri });
    } catch (err) {
      console.error('[NotificationCenter] Failed to update status', iri, err);
    }
  }

  async function resolveNotification(iri, event) {
    event.stopPropagation();
    updateLocalStatus(iri, STATUS_COMPLETED);
    setNotificationStatus(iri, STATUS_COMPLETED).catch(() => updateLocalStatus(iri, STATUS_PENDING));
  }

  async function reopenNotification(iri, event) {
    event.stopPropagation();
    updateLocalStatus(iri, STATUS_PENDING);
    setNotificationStatus(iri, STATUS_PENDING).catch(() => updateLocalStatus(iri, STATUS_COMPLETED));
  }

  function isSelected(iri) {
    return selectedIris[iri] === true;
  }

  function toggleSelection(iri) {
    const next = { ...selectedIris };
    if (next[iri]) delete next[iri];
    else next[iri] = true;
    selectedIris = next;
  }

  function toggleSelectAllFiltered() {
    const filteredIris = filteredNotifications.map(n => n.iri);
    if (filteredIris.length === 0) return;
    const allSelected = filteredIris.every(iri => selectedIris[iri]);
    const next = { ...selectedIris };
    if (allSelected) {
      for (const iri of filteredIris) delete next[iri];
    } else {
      for (const iri of filteredIris) next[iri] = true;
    }
    selectedIris = next;
  }

  async function resolveSelected() {
    if (resolving) return;
    resolving = true;
    try {
      const targetIris = filteredNotifications
        .filter(n => isSelected(n.iri) && isPending(n))
        .map(n => n.iri);
      for (const iri of targetIris) updateLocalStatus(iri, STATUS_COMPLETED);
      const next = { ...selectedIris };
      for (const iri of targetIris) delete next[iri];
      selectedIris = next;
      await Promise.all(targetIris.map(iri =>
        setNotificationStatus(iri, STATUS_COMPLETED).catch(() => updateLocalStatus(iri, STATUS_PENDING))
      ));
    } finally {
      resolving = false;
    }
  }

  async function openSource(source, event) {
    event.stopPropagation();
    if (!source?.iri) return;
    try {
      await openEntity(source.iri, conversationIri);
    } catch (err) {
      console.error('[NotificationCenter] Failed to open source', err);
    }
  }

  async function closeWidget() {
    try {
      await invoke('widget_blackboard__remove_widget', { widgetId });
    } catch (err) {
      console.error('Failed to remove widget:', err);
    }
  }

  async function upsertNotification(iri) {
    const detail = await fetchNotificationDetails(iri);
    if (!detail) return;
    const enriched = { ...detail, _ts: resolveTimestamp(detail) };
    const idx = notifications.findIndex(n => n.iri === iri);
    if (idx >= 0) {
      const next = notifications.slice();
      next[idx] = enriched;
      next.sort((a, b) => b._ts - a._ts);
      notifications = next;
    } else {
      notifications = [enriched, ...notifications].sort((a, b) => b._ts - a._ts);
    }
  }

  onMount(async () => {
    entitySub.setCreationQueries([{
      classIri: 'foundation:AINotification',
      predicate: 'rdf:type',
      objectValue: 'foundation:AINotification',
    }]);
    await loadNotifications();
    mounted = true;
  });

  let mounted = $state(false);
  $effect(() => {
    filterStatus;
    if (untrack(() => mounted)) loadNotifications();
  });

  onDestroy(() => {
    entitySub.destroy();
    clearTimeout(upsertTimer);
  });
</script>

<WidgetContainer
  icon="notifications"
  title="Notification Center"
  {windowState}
  {onWindowStateChange}
  onClose={closeWidget}
>
  {#snippet headerSubtitle()}
    <span class="header-counter">
      {filteredNotifications.length}
      {filteredNotifications.length === 1 ? 'notificação' : 'notificações'}
      {#if filterType !== 'all' || filterStatus !== 'all'}<span class="header-counter-filter">(filtrada{filteredNotifications.length === 1 ? '' : 's'})</span>{/if}
    </span>
  {/snippet}

  {#snippet headerActions()}
    {@const filteredIris = filteredNotifications.map(n => n.iri)}
    {@const allFilteredSelected = filteredIris.length > 0 && filteredIris.every(iri => selectedIris[iri])}
    {@const selectedPendingCount = filteredNotifications.filter(n => isSelected(n.iri) && isPending(n)).length}
    <Button
      variant="ghost"
      size="sm"
      class="header-action-btn"
      onclick={toggleSelectAllFiltered}
      disabled={filteredIris.length === 0}
      title={allFilteredSelected ? 'Desmarcar todas exibidas' : 'Selecionar todas exibidas'}
    >
      <span class="material-symbols-outlined">
        {allFilteredSelected ? 'check_box' : 'check_box_outline_blank'}
      </span>
      <span>{allFilteredSelected ? 'Desmarcar todas' : 'Selecionar todas'}</span>
    </Button>
    {#if selectedPendingCount > 0}
      <Button
        variant="ghost"
        size="sm"
        class="header-action-btn"
        onclick={resolveSelected}
        disabled={resolving}
        title="Resolver pendentes selecionadas"
      >
        <span class="material-symbols-outlined" class:spinning={resolving}>
          {resolving ? 'progress_activity' : 'done_all'}
        </span>
        <span>Resolver selecionadas ({selectedPendingCount})</span>
      </Button>
    {/if}
  {/snippet}

  <div class="widget-content">
    {#if loading}
      <div class="state-msg">
        <span class="material-symbols-outlined spinning">progress_activity</span>
        <span>Carregando notificações…</span>
      </div>
    {:else if error}
      <div class="state-msg fail">
        <span class="material-symbols-outlined">error</span>
        <span>{error}</span>
      </div>
    {:else if notifications.length === 0}
      <div class="state-msg empty">
        <span class="material-symbols-outlined">notifications_off</span>
        <span>Sem notificações.</span>
      </div>
    {:else}
      <div class="filter-bar">
        <Select.Root type="single" bind:value={filterType}>
          <Select.Trigger class="filter-select" aria-label="Filtrar por tipo">
            <Select.Value />
          </Select.Trigger>
          <Select.Content>
            {#each TYPE_FILTERS as f}
              <Select.Item value={f.value}>{f.label}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
        <Select.Root type="single" bind:value={filterStatus}>
          <Select.Trigger class="filter-select" aria-label="Filtrar por status">
            <Select.Value />
          </Select.Trigger>
          <Select.Content>
            {#each STATUS_FILTERS as f}
              <Select.Item value={f.value}>{f.label}</Select.Item>
            {/each}
          </Select.Content>
        </Select.Root>
      </div>
      {#if filteredNotifications.length === 0}
        <div class="state-msg empty">
          <span class="material-symbols-outlined">filter_alt_off</span>
          <span>Nenhuma notificação corresponde ao filtro.</span>
        </div>
      {:else}
      <ul class="notification-list">
        {#each filteredNotifications as notif (notif.iri)}
          {@const expanded = expandedIri === notif.iri}
          {@const pending = isPending(notif)}
          {@const resolved = isResolved(notif)}
          <li
            class="notification-item type-{notif.type}"
            class:resolved
            class:expanded
            class:selected={isSelected(notif.iri)}
          >
            <div class="notif-header-row">
              <Checkbox
                class="select-checkbox"
                checked={isSelected(notif.iri)}
                onCheckedChange={() => toggleSelection(notif.iri)}
                aria-label="Selecionar notificação"
              />
              <Button
                variant="ghost"
                class="notification-row"
                onclick={() => toggleExpand(notif.iri)}
                aria-expanded={expanded}
              >
                <span class="material-symbols-outlined type-icon type-icon-{notif.type}">
                  {resolved ? 'check_circle' : TYPE_ICONS[notif.type]}
                </span>
                <div class="notif-body">
                  <span class="notif-title">{notif.title}</span>
                </div>
                {#if notif._ts}
                  <span class="meta-time" title={formatAbsoluteDate(notif.createdAt ?? notif.lastUpdatedAt)}>
                    {formatRelativeDate(notif._ts)}
                  </span>
                {/if}
                <span class="material-symbols-outlined chevron">
                  {expanded ? 'expand_less' : 'expand_more'}
                </span>
              </Button>
              {#if pending}
                <Button
                  variant="ghost"
                  size="icon"
                  class="row-action resolve"
                  onclick={(e) => resolveNotification(notif.iri, e)}
                  title="Marcar como resolvida"
                  aria-label="Marcar como resolvida"
                >
                  <span class="material-symbols-outlined">check</span>
                </Button>
              {:else if resolved}
                <Button
                  variant="ghost"
                  size="icon"
                  class="row-action reopen"
                  onclick={(e) => reopenNotification(notif.iri, e)}
                  title="Reabrir notificação"
                  aria-label="Reabrir notificação"
                >
                  <span class="material-symbols-outlined">undo</span>
                </Button>
              {/if}
            </div>
            {#if expanded}
              <div class="notif-detail">
                {#if notif.body}
                  <p class="notif-body-text">{notif.body}</p>
                {/if}
                {#if notif.sources.length > 0}
                  <div class="source-list">
                    <span class="source-list-label">Fonte:</span>
                    {#each notif.sources as source}
                      <Button
                        variant="ghost"
                        size="sm"
                        class="source-chip"
                        onclick={(e) => openSource(source, e)}
                        title="Abrir {source.label}"
                      >
                        <span class="material-symbols-outlined source-icon">{source.icon}</span>
                        <span class="source-label">{source.label}</span>
                      </Button>
                    {/each}
                  </div>
                {/if}
              </div>
            {/if}
          </li>
        {/each}
      </ul>
      {/if}
    {/if}
  </div>
</WidgetContainer>

<style>
  .widget-content {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }

  .filter-bar {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 14px;
    border-bottom: 1px solid var(--border);
    flex-shrink: 0;
  }

  :global(.filter-select) {
    flex: 1;
    min-width: 0;
    font-family: var(--font-body);
    font-size: 12px;
    height: auto;
    padding: 4px 8px;
  }

  .header-counter {
    font-size: 11px;
    color: var(--muted-foreground);
    opacity: 0.85;
  }

  .header-counter-filter {
    margin-left: 4px;
    font-style: italic;
    opacity: 0.75;
  }

  .state-msg {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 24px 16px;
    font-size: 12px;
    color: var(--muted-foreground);
    justify-content: center;
  }

  .state-msg.fail { color: var(--destructive); }
  .state-msg.empty { opacity: 0.7; }

  .state-msg .material-symbols-outlined {
    font-size: 18px;
  }

  .notification-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    flex: 1;
    overflow: auto;
  }

  .notification-item {
    border-bottom: 1px solid var(--border);
  }

  .notification-item.resolved {
    opacity: 0.55;
  }

  .notification-item.resolved .type-icon {
    color: var(--color-success);
  }

  .notification-item.selected {
    background: color-mix(in srgb, var(--primary) 10%, transparent);
  }

  .notif-header-row {
    display: flex;
    align-items: center;
    gap: 4px;
    padding-left: 14px;
  }

  :global(.select-checkbox) {
    flex-shrink: 0;
    width: 14px;
    height: 14px;
    cursor: pointer;
    margin: 0;
  }

  :global([data-slot="button"].notification-row) {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 4px 10px 0;
    height: auto;
    justify-content: flex-start;
    color: var(--foreground);
    border-radius: 0;
  }

  :global([data-slot="button"].notification-row:hover) {
    background: var(--accent);
    color: var(--foreground);
  }

  .type-icon {
    font-size: 18px;
    flex-shrink: 0;
  }

  .type-icon-error { color: var(--destructive); }
  .type-icon-warning { color: var(--color-warning); }
  .type-icon-info { color: var(--muted-foreground); }

  .notif-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }

  .notif-title {
    font-size: 12px;
    font-weight: 600;
    color: var(--foreground);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .meta-time {
    font-size: 11px;
    color: var(--muted-foreground);
    opacity: 0.85;
    white-space: nowrap;
    flex-shrink: 0;
    cursor: help;
  }

  .chevron {
    font-size: 18px;
    color: var(--primary);
    flex-shrink: 0;
  }

  :global(.row-action) {
    flex-shrink: 0;
    color: var(--primary) !important;
  }

  :global(.row-action.reopen) {
    color: var(--muted-foreground) !important;
  }

  :global(.row-action .material-symbols-outlined) {
    font-size: 16px;
  }

  :global(.header-action-btn) {
    color: var(--primary) !important;
    font-size: 11px !important;
    font-weight: 600 !important;
  }

  :global(.header-action-btn .material-symbols-outlined) {
    font-size: 16px;
  }

  :global(.header-action-btn .spinning),
  :global(.header-action-btn .material-symbols-outlined.spinning) {
    animation: spin 1s linear infinite;
  }

  .notif-detail {
    padding: 0 14px 12px 64px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  .notif-body-text {
    margin: 0;
    font-size: 12px;
    line-height: 1.5;
    color: var(--muted-foreground);
    white-space: pre-wrap;
    word-break: break-word;
  }

  .source-list {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
    font-size: 11px;
    color: var(--muted-foreground);
  }

  .source-list-label {
    text-transform: uppercase;
    letter-spacing: 0.04em;
    font-weight: 600;
    opacity: 0.7;
  }

  :global(.source-chip) {
    color: var(--primary) !important;
    font-size: 11px !important;
    padding: 2px 8px !important;
    height: auto !important;
  }

  .source-icon {
    font-size: 13px;
  }

  .source-label {
    max-width: 200px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .spinning {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }
</style>
