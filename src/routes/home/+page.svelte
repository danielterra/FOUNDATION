<script>
  import { invoke } from '@tauri-apps/api/core';
  import ChatWindow from '$lib/components/ChatWindow.svelte';
  import WidgetManager from '$lib/components/widgets/WidgetManager.svelte';
  import Search from '$lib/components/graph/Search.svelte';

  let isChatOpen = true;
  let searchComponent = $state();

  async function handleSearchResult(entityId) {
    try {
      await invoke('widget__add', {
        widgetType: 'inspector',
        entityId,
        position: null,
        size: null
      });
    } catch (e) {
      console.error('Failed to open inspector:', e);
    }
  }

  function openSearch() {
    if (searchComponent) searchComponent.open();
  }

  function handleKeydown(event) {
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
  <div class="top-bar">
    <span class="logo">FOUNDATION</span>
    <button class="search-trigger" onclick={openSearch} aria-label="Search (/)">
      <span class="material-symbols-outlined">search</span>
    </button>
  </div>

  <Search bind:this={searchComponent} onSelectResult={handleSearchResult} />

  <div class="content-area">
    <div class="canvas-area">
      <WidgetManager />
    </div>

    <div class="chat-area">
      <ChatWindow bind:isOpen={isChatOpen} />
    </div>
  </div>
</div>

<style>
  .main-layout {
    display: flex;
    flex-direction: column;
    width: 100vw;
    height: 100vh;
    overflow: hidden;
  }

  .top-bar {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.6rem 1.25rem;
    background: color-mix(in srgb, var(--color-black) 80%, transparent);
    border-bottom: 1px solid color-mix(in srgb, var(--color-white) 8%, transparent);
    flex-shrink: 0;
    z-index: 200;
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
    border-radius: 4px;
    transition: color 0.15s, background 0.15s;
  }

  .search-trigger:hover {
    color: var(--color-neutral-active);
    background: color-mix(in srgb, var(--color-white) 8%, transparent);
  }

  .search-trigger span {
    font-size: 20px;
  }

  .content-area {
    display: flex;
    flex: 1;
    overflow: hidden;
  }

  .canvas-area {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    background: transparent;
    position: relative;
    z-index: 1;
  }

  .chat-area {
    width: 30%;
    min-width: 500px;
    height: 100%;
    background: color-mix(in srgb, var(--color-black) 40%, transparent);
    border-left: 1px solid color-mix(in srgb, var(--color-white) 15%, transparent);
    position: relative;
    z-index: 100;
    display: flex;
    flex-direction: column;
  }
</style>
