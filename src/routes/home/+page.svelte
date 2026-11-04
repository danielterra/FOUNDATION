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
  import { appState } from '$lib/appState.svelte';
  import { openEntity } from '$lib/blackboard';
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';

  let isChatOpen = $state(false);
  let chatEnabled = $state(false);
  let chatVisibilityReady = $state(false);
  let chatVisibilityError = $state(null);
  let chatVisibilitySaveError = $state(null);
  let chatVisibilitySaveInProgress = $state(false);
  let searchComponent = $state();
  let activeConversationIri = $state(null);
  let activeBlackboardIri = $state(null);
  let blackboardResolutionId = 0;
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
    const conv = chatEnabled ? activeConversationIri : null;
    resolveBlackboard(conv);
  });

  onMount(initializeHome);

  async function initializeHome() {
    try {
      const setupDone = await invoke('setup__check');
      if (!setupDone) { goto('/'); return; }
    } catch {
      goto('/');
      return;
    }

    await loadChatVisibility();
    await checkChatEnabled();
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
    }
  }

  async function checkChatEnabled() {
    try {
      const service = await invoke('setup__get_current_ai_service');
      if (!service) { chatEnabled = false; return; }
      if (service.isLocal) {
        chatEnabled = true;
      } else {
        const key = await invoke('ai__get_api_key', { serviceIri: service.iri });
        chatEnabled = !!(key && key.trim());
      }
    } catch {
      chatEnabled = true;
    }
  }

  function closeSettings() {
    showSettings = false;
    window.dispatchEvent(new Event('foundation:settings-closed'));
    checkChatEnabled();
  }

  $effect(() => { appState.activeConversationIri = activeConversationIri; });

  async function handleSearchResult(entityId) {
    if (!activeBlackboardIri) return false;
    try {
      await openEntity(entityId, activeConversationIri);
      return true;
    } catch (e) {
      console.error('Failed to open entity:', e);
      return false;
    }
  }

  function openSearch() {
    if (searchComponent) searchComponent.open();
  }

  async function toggleChat() {
    if (!chatEnabled || !chatVisibilityReady || chatVisibilitySaveInProgress) return;
    chatVisibilitySaveError = null;
    const visibility = isChatOpen ? 'closed' : 'open';
    await persistChatVisibility(visibility);
  }

  async function persistChatVisibility(visibility) {
    if (chatVisibilitySaveInProgress || !chatVisibilityReady) return;
    chatVisibilitySaveInProgress = true;
    try {
      await invoke('settings__set_chat_panel_visibility', { visibility });
      isChatOpen = visibility === 'open';
    } catch (e) {
      chatVisibilitySaveError = `Não foi possível salvar a preferência de visibilidade do chat: ${String(e)}`;
    } finally {
      chatVisibilitySaveInProgress = false;
    }
  }

  async function openAiCallHistory() {
    invoke('widget_blackboard__add_widget', {
      widgetType: 'ai_call_history',
      entityId: '',
      position: null,
      size: null,
      conversationId: appState.activeConversationIri ?? null,
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
  {#if chatEnabled && chatVisibilityReady}
    <Button variant="ghost" size="icon" title={chatVisibilitySaveInProgress ? 'Salvando preferência…' : 'Conversa (Ctrl+S)'} aria-label="Conversa" aria-expanded={isChatOpen} aria-controls="home-chat-panel" aria-busy={chatVisibilitySaveInProgress} disabled={chatVisibilitySaveInProgress} onclick={toggleChat}><span class="material-symbols-outlined">forum</span></Button>
  {/if}
  <Button variant="ghost" size="icon" title="Consumo de IA" onclick={openAiCallHistory}><span class="material-symbols-outlined">payments</span></Button>
  <NotificationBell />
  <Button variant="ghost" size="icon" title="Configurações" onclick={() => showSettings = true}><span class="material-symbols-outlined">settings</span></Button>
</HeaderActions>

<div class="main-layout">
  <Search bind:this={searchComponent} onSelectResult={handleSearchResult} />

  <div class="content-area">
    {#if chatVisibilityError || chatVisibilitySaveError}
      <div class="chat-visibility-error" role="alert" aria-live="assertive">
        {chatVisibilityError ?? chatVisibilitySaveError}
      </div>
    {/if}
    <div class="canvas-area">
      <WidgetManager {activeBlackboardIri} {activeConversationIri} />
    </div>

    {#if chatEnabled && chatVisibilityReady && isChatOpen}
      <div id="home-chat-panel" class="chat-slot" role="region" aria-label="Conversa" transition:fly={{ x: 500, duration: 250, easing: cubicOut }}>
        <ChatWindow bind:activeConversationIri />
      </div>
    {/if}
  </div>

  {#if showSettings}
    <SettingsPanel onClose={closeSettings} />
  {/if}
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
