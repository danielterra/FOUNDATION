<script>
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import ChatWindow from '$lib/components/ChatWindow.svelte';
  import WidgetManager from '$lib/components/widgets/WidgetManager.svelte';
  import Search from '$lib/components/graph/Search.svelte';
  import SettingsPanel from '$lib/components/SettingsPanel.svelte';
  import Button from '$lib/components/Button.svelte';
  import NotificationBell from '$lib/components/NotificationBell.svelte';
  import { appState } from '$lib/appState.svelte';

  let isChatOpen = $state(false);
  let chatEnabled = $state(false);
  let searchComponent = $state();
  let widgetManager = $state();
  let activeConversationIri = $state(null);
  let activeBlackboardIri = $state(null);
  let showSettings = $state(false);

  async function resolveBlackboard(conversationIri) {
    try {
      activeBlackboardIri = await invoke('widget_blackboard__resolve_blackboard', {
        conversationId: conversationIri ?? null
      });
    } catch (e) {
      console.error('Failed to resolve blackboard:', e);
      activeBlackboardIri = null;
    }
  }

  $effect(() => {
    const conv = chatEnabled ? activeConversationIri : null;
    resolveBlackboard(conv);
  });

  onMount(async () => {
    try {
      const setupDone = await invoke('setup__check');
      if (!setupDone) { goto('/'); return; }
      await checkChatEnabled();
      isChatOpen = chatEnabled;
    } catch {
      goto('/');
    }
  });

  async function checkChatEnabled() {
    const wasEnabled = chatEnabled;
    try {
      const service = await invoke('setup__get_current_ai_service');
      if (!service) { chatEnabled = false; return; }
      if (service.isLocal) {
        chatEnabled = true;
      } else {
        const key = await invoke('ai__get_api_key', { serviceIri: service.iri });
        chatEnabled = !!(key && key.trim());
      }
      if (chatEnabled && !wasEnabled) isChatOpen = true;
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
    if (!activeBlackboardIri) return;
    try {
      await invoke('widget_blackboard__add_widget', {
        widgetType: 'inspector',
        entityId,
        position: null,
        size: null,
        blackboardId: activeBlackboardIri
      });
    } catch (e) {
      console.error('Failed to open inspector:', e);
    }
  }

  function openSearch() {
    if (searchComponent) searchComponent.open();
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
      if (chatEnabled) isChatOpen = !isChatOpen;
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

<div class="main-layout">
  <Search bind:this={searchComponent} onSelectResult={handleSearchResult} />

  <div class="top-bar">
    <span class="logo">FOUNDATION</span>
    <Button icon="search" title="Buscar (/)" onclick={openSearch} />
    <Button icon="collapse_all" title="Minimizar todos os widgets" onclick={() => widgetManager?.minimizeAll()} />
    <Button icon="expand_all" title="Expandir todos os widgets" onclick={() => widgetManager?.expandAll()} />
    <span class="top-bar-spacer"></span>
    {#if chatEnabled}
      <Button icon="forum" title="Conversa (Ctrl+S)" onclick={() => isChatOpen = !isChatOpen} />
    {/if}
    <Button icon="payments" title="Consumo de IA" onclick={openAiCallHistory} />
    <NotificationBell />
    <Button icon="settings" title="Configurações" onclick={() => showSettings = true} />
  </div>

  <div class="content-area">
    <div class="canvas-area">
      <WidgetManager bind:this={widgetManager} {activeBlackboardIri} {activeConversationIri} chatOpen={chatEnabled && isChatOpen} />
    </div>

    {#if chatEnabled && isChatOpen}
      <ChatWindow bind:activeConversationIri bind:isOpen={isChatOpen} />
    {/if}
  </div>

  {#if showSettings}
    <SettingsPanel onClose={closeSettings} />
  {/if}
</div>

<style>
  .main-layout {
    width: 100vw;
    height: 100vh;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    --top-bar-height: calc(0.6rem * 2 + 1.5rem);
  }

  .top-bar {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.6rem 1.25rem;
    background: var(--color-surface-1);
    z-index: 200;
    height: var(--top-bar-height);
    flex-shrink: 0;
  }

  .logo {
    font-size: 1rem;
    font-weight: 600;
    color: var(--color-neutral-active);
    letter-spacing: 0.08em;
  }

  .top-bar-spacer {
    flex: 1;
  }

  .content-area {
    flex: 1;
    display: flex;
    overflow: hidden;
    min-height: 0;
  }

  .canvas-area {
    flex: 1;
    position: relative;
    overflow: hidden;
    padding: 3px;
  }
</style>
