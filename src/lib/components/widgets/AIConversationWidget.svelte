<script>
  import { onMount, onDestroy, tick } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { createEntitySubscription } from '$lib/realtime/subscriptions';
  import WidgetContainer from './WidgetContainer.svelte';
  import ChatMessageBubble from '../ChatMessageBubble.svelte';

  let { widgetId, entityId = '', windowState = 'normal', onWindowStateChange } = $props();

  let messages = $state([]);
  let label = $state('');
  let loading = $state(true);
  let error = $state(null);
  let messagesEl = $state(null);

  let loadPending = false;
  let reloadWhenDone = false;

  async function loadMessages() {
    if (!entityId) return;
    if (loadPending) {
      reloadWhenDone = true;
      console.debug(`[CONV_WIDGET] ${entityId} queued (loadPending=true, label=${!!label})`);
      return;
    }
    loadPending = true;
    console.debug(`[CONV_WIDGET] ${entityId} loadMessages start (label=${!!label})`);
    try {
      const result = await invoke('chat__get_recent_messages', {
        limit: 200,
        conversationId: entityId,
      });
      messages = result.messages ?? [];
      if (!label) {
        console.debug(`[CONV_WIDGET] ${entityId} fetching label via inspector__get_entity`);
        const conv = await invoke('inspector__get_entity', { entityId });
        label = JSON.parse(conv)?.label ?? '';
      }
    } catch (err) {
      error = err?.toString() ?? 'Falha ao carregar mensagens';
    } finally {
      loading = false;
      loadPending = false;
      if (reloadWhenDone) {
        reloadWhenDone = false;
        console.debug(`[CONV_WIDGET] ${entityId} reloadWhenDone → scheduling next`);
        loadMessages();
      }
    }
    await tick();
    scrollToBottom();
  }

  function scrollToBottom() {
    if (messagesEl) messagesEl.scrollTop = messagesEl.scrollHeight;
  }

  async function closeWidget() {
    await invoke('widget_blackboard__remove_widget', { widgetId });
  }

  const entitySub = createEntitySubscription((event) => {
    if (event.type === 'referenced') loadMessages();
  });

  onMount(async () => {
    await loadMessages();
  });

  onDestroy(() => {
    entitySub.destroy();
  });

  $effect(() => {
    entitySub.setIris(entityId ? [entityId] : []);
  });
</script>

<WidgetContainer
  icon="chat"
  title={label || 'Conversa'}
  {windowState}
  {onWindowStateChange}
  onClose={closeWidget}
  canExpand={true}
>
  <div class="messages-scroll" bind:this={messagesEl}>
    {#if loading}
      <div class="state-center">
        <span class="material-symbols-outlined spinning">progress_activity</span>
      </div>
    {:else if error}
      <div class="state-center error">{error}</div>
    {:else if messages.length === 0}
      <div class="state-center muted">
        <span class="material-symbols-outlined">chat_bubble</span>
        <span>Nenhuma mensagem</span>
      </div>
    {:else}
      <div class="messages-inner">
        {#each messages as unit (unit.iri)}
          <ChatMessageBubble {unit} conversationId={entityId} />
        {/each}
      </div>
    {/if}
  </div>
</WidgetContainer>

<style>
  .messages-scroll {
    flex: 1;
    overflow-y: auto;
    overflow-x: hidden;
    min-height: 0;
    height: 100%;
  }

  .messages-scroll::-webkit-scrollbar {
    width: 6px;
  }

  .messages-scroll::-webkit-scrollbar-track {
    background: transparent;
  }

  .messages-scroll::-webkit-scrollbar-thumb {
    background: color-mix(in srgb, var(--color-white) 20%, transparent);
  }

  .messages-inner {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 12px;
  }

  .state-center {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    gap: 12px;
    padding: 32px;
  }

  .state-center.muted {
    color: var(--color-neutral);
    opacity: 0.5;
  }

  .state-center.muted .material-symbols-outlined {
    font-size: 40px;
  }

  .state-center.error {
    color: var(--color-error);
    font-size: 13px;
  }

  .spinning {
    animation: spin 1s linear infinite;
    font-size: 32px;
    color: var(--color-neutral);
    opacity: 0.5;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to   { transform: rotate(360deg); }
  }
</style>
