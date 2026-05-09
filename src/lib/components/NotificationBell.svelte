<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import Button from './Button.svelte';
  import { appState } from '$lib/appState.svelte';

  let pendingCount = $state(0);
  let unlistenEntityUpdated: (() => void) | undefined;

  type SearchEntity = { id: string; status?: { iri?: string } | null };

  async function refreshCount() {
    try {
      const resultStr = await invoke<string>('graph__search_entities', {
        query: '',
        typeIri: 'foundation:AINotification',
        limit: 500,
      });
      const list: SearchEntity[] = JSON.parse(resultStr);
      pendingCount = list.filter(n => n.status?.iri === 'foundation:Pending').length;
    } catch (err) {
      console.error('[NotificationBell] Failed to refresh count:', err);
    }
  }

  function openNotificationCenter() {
    invoke('widget_blackboard__add_widget', {
      widgetType: 'notification_center',
      entityId: 'foundation:NotificationCenter',
      position: null,
      size: null,
      conversationId: appState.activeConversationIri,
    }).catch(e => console.error('[NotificationBell] Failed to open widget:', e));
  }

  onMount(async () => {
    await refreshCount();
    unlistenEntityUpdated = await listen<{ entityId?: string }>('entity-updated', (event) => {
      const id = event.payload?.entityId ?? '';
      if (id.includes('AINotification')) refreshCount();
    });
  });

  onDestroy(() => {
    unlistenEntityUpdated?.();
  });
</script>

<div class="bell-wrapper">
  <Button
    icon="notifications"
    title={pendingCount > 0 ? `${pendingCount} notificação(ões) pendente(s)` : 'Histórico de notificações'}
    onclick={openNotificationCenter}
  />
  {#if pendingCount > 0}
    <span class="bell-badge">{pendingCount > 99 ? '99+' : pendingCount}</span>
  {/if}
</div>

<style>
  .bell-wrapper {
    position: relative;
    display: inline-flex;
  }

  .bell-badge {
    position: absolute;
    top: -2px;
    right: -2px;
    min-width: 16px;
    height: 16px;
    padding: 0 4px;
    border-radius: 8px;
    background: var(--color-danger, #ef4444);
    color: white;
    font-size: 9px;
    font-weight: 700;
    display: flex;
    align-items: center;
    justify-content: center;
    pointer-events: none;
    box-shadow: 0 0 0 2px var(--color-surface-0);
  }
</style>
