<script>
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import ChatWindow from '$lib/components/ChatWindow.svelte';
  import WidgetManager from '$lib/components/widgets/WidgetManager.svelte';
  import Search from '$lib/components/graph/Search.svelte';
  import SettingsPanel from '$lib/components/SettingsPanel.svelte';
  import Button from '$lib/components/Button.svelte';
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
    <Button icon="settings" title="Configurações" onclick={() => showSettings = true} />
  </div>

  <div class="canvas-area">
    <WidgetManager bind:this={widgetManager} {activeBlackboardIri} {activeConversationIri} />
  </div>

  {#if chatEnabled}
    <div class="chat-overlay" class:hidden={!isChatOpen}>
      <ChatWindow bind:activeConversationIri />
    </div>
  {/if}

  {#if showSettings}
    <SettingsPanel onClose={closeSettings} />
  {/if}
</div>

<style>
  .main-layout {
    width: 100vw;
    height: 100vh;
    overflow: hidden;
    position: relative;
  }

  .top-bar {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.6rem 1.25rem;
    background: var(--color-surface-1);
    z-index: 200;
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

  .canvas-area {
    width: 100%;
    height: 100%;
    position: relative;
    z-index: 1;
  }

  .chat-overlay {
    position: fixed;
    top: 0;
    right: 0;
    width: 30%;
    min-width: 500px;
    height: 100vh;
    z-index: 250;
    display: flex;
    flex-direction: column;
    transition: transform 0.25s cubic-bezier(0.4, 0, 0.2, 1), opacity 0.25s;
  }

  .chat-overlay.hidden {
    transform: translateX(100%);
    opacity: 0;
    pointer-events: none;
  }
</style>
