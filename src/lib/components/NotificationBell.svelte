<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { createEntitySubscription } from '$lib/realtime/subscriptions';
  import { Button } from '$lib/components/ui/button';
  import { appState } from '$lib/appState.svelte';

  let pendingCount = $state(0);
  const entitySub = createEntitySubscription((event) => {
    if (event.type !== 'updated') return;
    clearTimeout(refreshTimer);
    refreshTimer = setTimeout(refreshCount, 300);
  });
  let refreshTimer: ReturnType<typeof setTimeout> | undefined;

  async function refreshCount() {
    try {
      pendingCount = await invoke<number>('notification__count_pending');
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
    entitySub.setPatterns(['AINotification']);
  });

  onDestroy(() => {
    entitySub.destroy();
    clearTimeout(refreshTimer);
  });
</script>

<div class="bell-wrapper">
  <Button variant="ghost" size="icon"
    title={pendingCount > 0 ? `${pendingCount} notificação(ões) pendente(s)` : 'Histórico de notificações'}
    onclick={openNotificationCenter}
  ><span class="material-symbols-outlined">notifications</span></Button>
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
    border-radius: var(--radius);
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
