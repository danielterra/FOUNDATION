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
  import HeaderActions from '$lib/components/HeaderActions.svelte';
  import { appState } from '$lib/appState.svelte';
  import { openEntity } from '$lib/blackboard';
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';

  let isChatOpen = $state(false);
  let chatEnabled = $state(false);
  let searchComponent = $state();
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
      await openEntity(entityId, activeConversationIri);
    } catch (e) {
      console.error('Failed to open entity:', e);
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

<HeaderActions>
  <Button icon="search" title="Buscar (/)" onclick={openSearch} />
  <span class="header-spacer"></span>
  {#if chatEnabled}
    <Button icon="forum" title="Conversa (Ctrl+S)" onclick={() => isChatOpen = !isChatOpen} />
  {/if}
  <Button icon="payments" title="Consumo de IA" onclick={openAiCallHistory} />
  <NotificationBell />
  <Button icon="settings" title="Configurações" onclick={() => showSettings = true} />
</HeaderActions>

<div class="main-layout">
  <Search bind:this={searchComponent} onSelectResult={handleSearchResult} />

  <div class="content-area">
    <div class="canvas-area">
      <WidgetManager {activeBlackboardIri} {activeConversationIri} />
    </div>

    {#if chatEnabled && isChatOpen}
      <div class="chat-slot" transition:fly={{ x: 500, duration: 250, easing: cubicOut }}>
        <ChatWindow bind:activeConversationIri bind:isOpen={isChatOpen} />
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
    display: flex;
    overflow: hidden;
    min-height: 0;
    gap: 5px;
  }

  .canvas-area {
    flex: 1;
    position: relative;
    overflow: hidden;
  }

  .chat-slot {
    width: 30%;
    min-width: 500px;
    height: 100%;
    flex-shrink: 0;
    display: flex;
  }
</style>
