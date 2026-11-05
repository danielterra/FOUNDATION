<script>
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import ChatWindow from '$lib/components/ChatWindow.svelte';
  import WidgetManager from '$lib/components/widgets/WidgetManager.svelte';
  import Search from '$lib/components/graph/Search.svelte';
  import SettingsPanel from '$lib/components/SettingsPanel.svelte';
  import { Button } from '$lib/components/ui/button';
  import NotificationBell from '$lib/components/NotificationBell.svelte';
  import HeaderActions from '$lib/components/HeaderActions.svelte';
  import ConversationBar from '$lib/components/ConversationBar.svelte';
  import { createEntitySubscription } from '$lib/realtime/subscriptions';
  import { appState } from '$lib/appState.svelte';
  import { openEntity } from '$lib/blackboard';
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';

  let isChatOpen = $state(false);
  let providerState = $state('loading');
  let conversations = $state([]);
  let conversationsLoading = $state(false);
  let conversationsError = $state(null);
  let chatVisibilityReady = $state(false);
  let chatVisibilityError = $state(null);
  let chatVisibilitySaveError = $state(null);
  let searchComponent = $state();
  let activeConversationIri = $state(null);
  let activeBoardConversationIri = $derived(
    providerState === 'configured' && chatVisibilityReady && isChatOpen ? activeConversationIri : null
  );
  let activeBlackboardIri = $state(null);
  let blackboardResolutionId = 0;
  let chatVisibilityRequestId = 0;
  let conversationsSnapshotTx = 0;
  let conversationsLoadId = 0;
  const conversationsSub = createEntitySubscription(handleConversationEvent);
  let showSettings = $state(false);

  async function resolveBlackboard(conversationIri) {
    const resolutionId = ++blackboardResolutionId;
    activeBlackboardIri = null;
    try {
      const blackboardIri = await invoke('widget_blackboard__resolve_blackboard', {
        conversationId: conversationIri ?? null
      });
      if (resolutionId !== blackboardResolutionId) return;
      activeBlackboardIri = blackboardIri;
    } catch (e) {
      if (resolutionId !== blackboardResolutionId) return;
      console.error('Failed to resolve blackboard:', e);
      activeBlackboardIri = null;
    }
  }

  $effect(() => {
    resolveBlackboard(activeBoardConversationIri);
  });

  onMount(() => {
    initializeHome();
    return () => conversationsSub.destroy();
  });

  async function initializeHome() {
    try {
      const setupDone = await invoke('setup__check');
      if (!setupDone) { goto('/'); return; }
    } catch {
      goto('/');
      return;
    }

    await checkChatEnabled();
    if (providerState === 'configured') {
      await loadChatVisibility();
      await loadConversations();
      restoreActiveConversation();
    } else {
      activeConversationIri = null;
      isChatOpen = false;
      chatVisibilityReady = true;
    }
  }

  async function loadChatVisibility() {
    try {
      const visibility = await invoke('settings__get_chat_panel_visibility');
      if (visibility !== 'open' && visibility !== 'closed') {
        throw new Error(`Preferência de visibilidade inesperada: ${visibility}`);
      }
      isChatOpen = visibility === 'open';
      chatVisibilityReady = true;
    } catch (e) {
      chatVisibilityError = `Não foi possível carregar a preferência de visibilidade do chat: ${String(e)}`;
      isChatOpen = false;
      chatVisibilityReady = true;
    }
  }

  async function checkChatEnabled() {
    providerState = 'loading';
    try {
      const service = await invoke('setup__get_current_ai_service');
      if (!service) { providerState = 'not-configured'; return; }
      if (service.isLocal) { providerState = 'configured'; return; }
      const key = await invoke('ai__get_api_key', { serviceIri: service.iri });
      providerState = key && key.trim() ? 'configured' : 'not-configured';
    } catch (error) {
      console.error('Failed to check AI provider configuration:', error);
      providerState = 'error';
    }
  }

  function closeSettings() {
    showSettings = false;
    window.dispatchEvent(new Event('foundation:settings-closed'));
    refreshProviderState();
  }

  async function refreshProviderState() {
    const previousState = providerState;
    await checkChatEnabled();
    if (providerState === 'configured') {
      await loadChatVisibility();
      await loadConversations();
      restoreActiveConversation();
    } else {
      activeConversationIri = null;
      isChatOpen = false;
      chatVisibilityReady = true;
      if (previousState === 'configured') conversations = [];
    }
  }

  $effect(() => { appState.activeConversationIri = activeBoardConversationIri; });

  async function handleSearchResult(entityId) {
    if (!activeBlackboardIri) return false;
    try {
      await openEntity(entityId, activeBoardConversationIri);
      return true;
    } catch (e) {
      console.error('Failed to open entity:', e);
      return false;
    }
  }

  function openSearch() {
    if (searchComponent) searchComponent.open();
  }

  function toggleChat() {
    if (providerState !== 'configured' || !chatVisibilityReady) return;
    chatVisibilitySaveError = null;
    const previousVisibility = isChatOpen;
    const visibility = isChatOpen ? 'closed' : 'open';
    const requestId = ++chatVisibilityRequestId;
    isChatOpen = visibility === 'open';
    if (visibility === 'closed') {
      activeConversationIri = null;
      localStorage.removeItem('activeConversationIri');
    }
    persistChatVisibility(visibility, previousVisibility, requestId);
  }

  async function selectConversation(iri) {
    const previousVisibility = isChatOpen;
    activeConversationIri = iri;
    localStorage.setItem('activeConversationIri', iri);
    isChatOpen = true;
    chatVisibilityReady = true;
    chatVisibilitySaveError = null;
    await persistChatVisibility('open', previousVisibility, ++chatVisibilityRequestId);
  }

  async function createConversation(subject) {
    const label = subject.trim();
    if (!label) return;
    try {
      const conversation = await invoke('chat__create_conversation', { label });
      await loadConversations();
      await selectConversation(conversation.iri);
    } catch (error) {
      conversationsError = `Não foi possível criar a conversa: ${String(error)}`;
    }
  }

  async function handleConversationDeleted(iri) {
    if (activeConversationIri === iri) {
      const previousVisibility = isChatOpen;
      activeConversationIri = null;
      localStorage.removeItem('activeConversationIri');
      isChatOpen = false;
      await persistChatVisibility('closed', previousVisibility, ++chatVisibilityRequestId);
    }
    await loadConversations();
  }

  async function handleConversationEvent(event) {
    if (event.type !== 'joined-set' || event.classIri !== 'foundation:AIConversationMessage') return;
    if (event.tx != null && event.tx <= conversationsSnapshotTx) return;
    await loadConversations();
  }

  function restoreActiveConversation() {
    if (!isChatOpen || activeConversationIri || conversations.length === 0) return;
    const saved = localStorage.getItem('activeConversationIri');
    if (saved && conversations.some(conversation => conversation.iri === saved)) {
      activeConversationIri = saved;
    }
  }

  async function loadConversations() {
    if (providerState !== 'configured') return;
    const requestId = ++conversationsLoadId;
    conversationsLoading = true;
    conversationsError = null;
    try {
      const allItems = [];
      let cursor = null;
      let snapshotTx = null;
      let hasMore = true;
      while (hasMore) {
        const page = await invoke('chat__list_conversations', {
          limit: 200,
          afterCursorTx: cursor?.order_tx ?? null,
          afterCursorSubject: cursor?.after_subject ?? null,
          snapshotTx,
        });
        if (requestId !== conversationsLoadId) return;
        if (snapshotTx === null) snapshotTx = page.snapshot_tx;
        else if (snapshotTx !== page.snapshot_tx) throw new Error('O histórico mudou durante a paginação.');
        allItems.push(...page.items);
        hasMore = page.has_more;
        cursor = page.next_cursor;
        if (hasMore && !cursor) throw new Error('A paginação retornou um cursor inválido.');
      }
      if (requestId !== conversationsLoadId) return;
      conversations = allItems;
      conversationsSnapshotTx = snapshotTx ?? 0;
      conversationsSub.setCreationQueries(allItems.map(conversation => ({
        classIri: 'foundation:AIConversationMessage',
        predicate: 'foundation:partOfConversation',
        objectValue: conversation.iri,
      })));
      conversationsSub.setSinceTx(conversationsSnapshotTx);
      conversationsSub.replayMissed();
      if (activeConversationIri && !allItems.some(item => item.iri === activeConversationIri)) {
        activeConversationIri = null;
        localStorage.removeItem('activeConversationIri');
      }
    } catch (error) {
      if (requestId === conversationsLoadId) {
        conversationsError = `Não foi possível carregar as conversas: ${String(error)}`;
      }
    } finally {
      if (requestId === conversationsLoadId) conversationsLoading = false;
    }
  }

  async function persistChatVisibility(visibility, previousVisibility, requestId) {
    try {
      await invoke('settings__set_chat_panel_visibility', { visibility });
    } catch (e) {
      if (requestId !== chatVisibilityRequestId) return;
      isChatOpen = previousVisibility;
      chatVisibilitySaveError = `Não foi possível salvar a preferência de visibilidade do chat: ${String(e)}`;
    }
  }

  async function openAiCallHistory() {
    invoke('widget_blackboard__add_widget', {
      widgetType: 'ai_call_history',
      entityId: '',
      position: null,
      size: null,
      conversationId: activeBoardConversationIri,
    }).catch(e => console.error('Failed to open AI call history:', e));
  }

  function handleKeydown(event) {
    if ((event.metaKey || event.ctrlKey) && event.key === 's') {
      event.preventDefault();
      toggleChat();
    }

    if ((event.metaKey || event.ctrlKey) && event.key === 'f') {
      event.preventDefault();
      openSearch();
    }

    if (event.key === '/' && !event.metaKey && !event.ctrlKey) {
      const active = document.activeElement;
      const isTyping = active && (active.tagName === 'INPUT' || active.tagName === 'TEXTAREA' || active.isContentEditable);
      if (!isTyping) {
        event.preventDefault();
        openSearch();
      }
    }

    if (event.key === 'Escape' && !event.metaKey && !event.ctrlKey) {
      const active = document.activeElement;
      if (active && (active.tagName === 'INPUT' || active.tagName === 'TEXTAREA')) {
        active.blur();
      }
    }
  }

</script>

<svelte:window onkeydown={handleKeydown} />

<HeaderActions>
  <Button variant="ghost" size="icon" title="Buscar (/)" onclick={openSearch}><span class="material-symbols-outlined">search</span></Button>
  <span class="header-spacer"></span>
  {#if providerState === 'configured'}
    <ConversationBar bind:conversations bind:activeConversationIri onSwitch={selectConversation} onDelete={handleConversationDeleted} onCreate={createConversation} onRefresh={loadConversations} {conversationsLoading} />
    {#if chatVisibilityReady}
      <Button variant="ghost" size="icon" title="Conversa (Ctrl+S)" aria-label="Conversa" aria-expanded={isChatOpen} aria-controls="home-chat-panel" onclick={toggleChat}><span class="material-symbols-outlined">forum</span></Button>
    {/if}
  {/if}
  <Button variant="ghost" size="icon" title="Consumo de IA" onclick={openAiCallHistory}><span class="material-symbols-outlined">payments</span></Button>
  <NotificationBell />
  <Button variant="ghost" size="icon" title="Configurações" onclick={() => showSettings = true}><span class="material-symbols-outlined">settings</span></Button>
</HeaderActions>

<div class="main-layout">
  <Search bind:this={searchComponent} onSelectResult={handleSearchResult} />

  <div class="content-area">
    <div class="workspace-layer" class:workspace-suspended={showSettings} aria-hidden={showSettings} inert={showSettings}>
      {#if chatVisibilityError || chatVisibilitySaveError || conversationsError || providerState === 'error'}
        <div class="chat-visibility-error" role="alert" aria-live="assertive">
          {chatVisibilityError ?? chatVisibilitySaveError ?? conversationsError ?? 'Não foi possível verificar o provedor de IA.'}
        </div>
      {/if}
      <div class="canvas-area">
        <WidgetManager {activeBlackboardIri} activeConversationIri={activeBoardConversationIri} />
      </div>

      {#if providerState === 'configured' && chatVisibilityReady && isChatOpen}
        <div id="home-chat-panel" class="chat-slot" role="region" aria-label="Conversa" transition:fly={{ x: 500, duration: 250, easing: cubicOut }}>
          <ChatWindow bind:activeConversationIri />
        </div>
      {/if}
    </div>

    {#if showSettings}
      <SettingsPanel onClose={closeSettings} />
    {/if}
  </div>
</div>

<style>
  .main-layout {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .header-spacer {
    flex: 1;
  }

  .content-area {
    flex: 1;
    position: relative;
    overflow: hidden;
    min-height: 0;
  }

  .workspace-layer {
    position: absolute;
    inset: 0;
  }

  .workspace-suspended {
    visibility: hidden;
  }

  .canvas-area {
    position: absolute;
    inset: 0;
    overflow: hidden;
    background: var(--color-black);
  }

  .chat-slot {
    position: absolute;
    right: 8px;
    bottom: 8px;
    max-height: calc(100% - 16px);
    width: min(500px, 36vw, calc(100% - 16px));
    min-width: min(320px, calc(100% - 16px));
    display: flex;
    z-index: calc(var(--z-widget-max) + 1);
  }

  .chat-visibility-error {
    position: absolute;
    z-index: calc(var(--z-widget-max) + 2);
    top: var(--space-4);
    left: 50%;
    max-width: min(42rem, calc(100% - 2 * var(--space-4)));
    padding: var(--space-3) var(--space-4);
    transform: translateX(-50%);
    border: 1px solid color-mix(in srgb, var(--color-error) 55%, transparent);
    border-radius: var(--radius);
    background: var(--color-surface-2);
    color: var(--color-error);
    font-size: var(--type-body-compact);
  }

  :global(.chat-slot .chat-panel) {
    box-shadow: 0 8px 32px color-mix(in srgb, var(--color-black) 40%, transparent);
  }
</style>
