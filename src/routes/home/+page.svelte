<script>
  import { invoke } from '@tauri-apps/api/core';
  import ChatWindow from '$lib/components/ChatWindow.svelte';
  import WidgetManager from '$lib/components/widgets/WidgetManager.svelte';
  import Search from '$lib/components/graph/Search.svelte';

  let isChatOpen = $state(true);
  let searchComponent = $state();
  let widgetManager = $state();
  let activeConversationIri = $state(null);

  async function handleSearchResult(entityId) {
    try {
      await invoke('widget_blackboard__add_widget', {
        widgetType: 'inspector',
        entityId,
        position: null,
        size: null,
        conversationId: activeConversationIri
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
      isChatOpen = !isChatOpen;
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
    <button class="search-trigger" onclick={openSearch} aria-label="Search (/)">
      <span class="material-symbols-outlined">search</span>
    </button>
    <button class="top-bar-btn" onclick={() => widgetManager?.minimizeAll()} title="Minimize all widgets">
      <span class="material-symbols-outlined">collapse_all</span>
    </button>
    <button class="top-bar-btn" onclick={() => widgetManager?.expandAll()} title="Expand all widgets">
      <span class="material-symbols-outlined">expand_all</span>
    </button>
  </div>

  <div class="canvas-area">
    <WidgetManager bind:this={widgetManager} {activeConversationIri} />
  </div>

  <div class="chat-overlay" class:hidden={!isChatOpen}>
    <ChatWindow bind:activeConversationIri />
  </div>
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
    background: color-mix(in srgb, var(--color-black) 80%, transparent);
    border-bottom: 1px solid color-mix(in srgb, var(--color-white) 8%, transparent);
    z-index: 200;
    backdrop-filter: blur(8px);
  }

  .logo {
    font-size: 1rem;
    font-weight: 600;
    color: var(--color-neutral-active);
    letter-spacing: 0.08em;
  }

  .search-trigger {
    background: none;
    border: none;
    cursor: pointer;
    color: var(--color-neutral);
    padding: 0.2rem;
    display: flex;
    align-items: center;
    transition: color 0.15s, background 0.15s;
  }

  .search-trigger:hover {
    color: var(--color-neutral-active);
    background: color-mix(in srgb, var(--color-white) 8%, transparent);
  }

  .search-trigger span {
    font-size: 20px;
  }

  .top-bar-btn {
    background: none;
    border: none;
    cursor: pointer;
    color: var(--color-neutral);
    padding: 0.2rem;
    display: flex;
    align-items: center;
    transition: color 0.15s, background 0.15s;
  }

  .top-bar-btn:hover {
    color: var(--color-neutral-active);
    background: color-mix(in srgb, var(--color-white) 8%, transparent);
  }

  .top-bar-btn span {
    font-size: 20px;
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
    border-left: 1px solid color-mix(in srgb, var(--color-white) 10%, transparent);
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
