<script>
  import { invoke } from '@tauri-apps/api/core';
  import { onMount } from 'svelte';
  import { goto } from '$app/navigation';
  import ChatWindow from '$lib/components/ChatWindow.svelte';
  import WidgetManager from '$lib/components/widgets/WidgetManager.svelte';
  import Search from '$lib/components/graph/Search.svelte';
  import SettingsPanel from '$lib/components/SettingsPanel.svelte';
  import Modal from '$lib/components/Modal.svelte';
  import Button from '$lib/components/Button.svelte';
  import { appState } from '$lib/appState.svelte';

  let isChatOpen = $state(false);
  let chatEnabled = $state(false);
  let searchComponent = $state();
  let widgetManager = $state();
  let activeConversationIri = $state(null);
  let showSettings = $state(false);
  let showMcpModal = $state(false);
  let mcpTab = $state('claude-desktop');

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

  const MCP_APPS = [
    { id: 'claude-desktop', label: 'Claude Desktop', icon: 'smart_toy' },
  ];

  const CLAUDE_DESKTOP_CONFIG = `{
  "mcpServers": {
    "FOUNDATION": {
      "url": "http://127.0.0.1:47178/mcp"
    }
  }
}`;

  let copied = $state(false);
  function copyConfig() {
    navigator.clipboard.writeText(CLAUDE_DESKTOP_CONFIG);
    copied = true;
    setTimeout(() => { copied = false; }, 2000);
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
    <Button icon="lan" title="Conectar via MCP" onclick={() => showMcpModal = true} />
    <Button icon="settings" title="Configurações" onclick={() => showSettings = true} />
  </div>

  <div class="canvas-area">
    <WidgetManager bind:this={widgetManager} {activeConversationIri} />
  </div>

  {#if chatEnabled}
    <div class="chat-overlay" class:hidden={!isChatOpen}>
      <ChatWindow bind:activeConversationIri />
    </div>
  {/if}

  {#if showSettings}
    <SettingsPanel onClose={closeSettings} />
  {/if}

  <Modal
    open={showMcpModal}
    title="Conectar via MCP"
    icon="lan"
    size="md"
    onClose={() => showMcpModal = false}
  >
    <p class="mcp-intro">
      O FOUNDATION expõe um servidor MCP local que qualquer app compatível pode usar.
      Escolha seu app e siga as instruções abaixo.
    </p>

    <div class="mcp-tabs">
      {#each MCP_APPS as app}
        <button
          class="mcp-tab"
          class:active={mcpTab === app.id}
          onclick={() => mcpTab = app.id}
        >
          <span class="material-symbols-outlined">{app.icon}</span>
          {app.label}
        </button>
      {/each}
      <button class="mcp-tab mcp-tab-coming-soon" disabled>
        <span class="material-symbols-outlined">more_horiz</span>
        Mais em breve
      </button>
    </div>

    {#if mcpTab === 'claude-desktop'}
      <div class="mcp-instructions">
        <ol class="mcp-steps">
          <li>Abra o <strong>Claude Desktop</strong> e vá em <strong>Configurações → Desenvolvedor</strong>.</li>
          <li>Clique em <strong>Editar configuração</strong> para abrir o arquivo <code>claude_desktop_config.json</code>.</li>
          <li>Adicione o bloco abaixo ao arquivo (merge com configurações existentes se houver):</li>
        </ol>

        <div class="mcp-code-block">
          <pre>{CLAUDE_DESKTOP_CONFIG}</pre>
          <button class="mcp-copy-btn" onclick={copyConfig}>
            <span class="material-symbols-outlined">{copied ? 'check' : 'content_copy'}</span>
            {copied ? 'Copiado!' : 'Copiar'}
          </button>
        </div>

        <ol class="mcp-steps" start="4">
          <li>Salve o arquivo e <strong>reinicie o Claude Desktop</strong>.</li>
          <li>Na conversa, você verá o ícone <span class="material-symbols-outlined mcp-inline-icon">build</span> indicando que as ferramentas do FOUNDATION estão disponíveis.</li>
        </ol>

        <div class="mcp-paths">
          <p class="mcp-paths-label">Localização do arquivo de configuração:</p>
          <div class="mcp-path-row">
            <span class="mcp-os">macOS</span>
            <code>~/Library/Application Support/Claude/claude_desktop_config.json</code>
          </div>
          <div class="mcp-path-row">
            <span class="mcp-os">Windows</span>
            <code>%APPDATA%\Claude\claude_desktop_config.json</code>
          </div>
        </div>

        {#if !chatEnabled}
          <div class="mcp-chat-hint">
            <span class="material-symbols-outlined">info</span>
            Para usar o chat interno do FOUNDATION, configure uma chave de API nas
            <button class="mcp-settings-link" onclick={() => { showMcpModal = false; showSettings = true; }}>
              Configurações
            </button>.
          </div>
        {/if}
      </div>
    {/if}
  </Modal>
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

  /* ── MCP Modal ── */

  .mcp-intro {
    font-size: 0.875rem;
    color: var(--color-neutral);
    margin: 0 0 1.25rem;
    line-height: 1.6;
  }

  .mcp-tabs {
    display: flex;
    gap: 0.5rem;
    margin-bottom: 1.5rem;
    flex-wrap: wrap;
  }

  .mcp-tab {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.5rem 0.875rem;
    background: var(--color-surface-2);
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    color: var(--color-neutral);
    font-size: 0.8125rem;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.15s, color 0.15s, border-color 0.15s;
  }

  .mcp-tab .material-symbols-outlined {
    font-size: 1rem;
  }

  .mcp-tab:hover:not(:disabled) {
    background: var(--color-surface-3);
    color: var(--color-neutral-active);
  }

  .mcp-tab.active {
    background: color-mix(in srgb, var(--color-interactive) 12%, transparent);
    border-color: color-mix(in srgb, var(--color-interactive) 40%, transparent);
    color: var(--color-interactive);
  }

  .mcp-tab-coming-soon {
    opacity: 0.4;
    cursor: default;
  }

  .mcp-instructions {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .mcp-steps {
    margin: 0;
    padding-left: 1.25rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    font-size: 0.875rem;
    color: var(--color-neutral);
    line-height: 1.6;
  }

  .mcp-steps strong {
    color: var(--color-neutral-active);
  }

  .mcp-steps code {
    font-family: monospace;
    font-size: 0.8125rem;
    background: var(--color-surface-2);
    padding: 0.1em 0.35em;
    border-radius: 3px;
    color: var(--color-neutral-active);
  }

  .mcp-code-block {
    position: relative;
    background: var(--color-surface-2);
    border-radius: var(--radius-sm);
    overflow: hidden;
  }

  .mcp-code-block pre {
    margin: 0;
    padding: 1rem 1rem 1rem 1rem;
    font-family: monospace;
    font-size: 0.8125rem;
    color: var(--color-neutral-active);
    line-height: 1.6;
    overflow-x: auto;
  }

  .mcp-copy-btn {
    position: absolute;
    top: 0.5rem;
    right: 0.5rem;
    display: flex;
    align-items: center;
    gap: 0.3rem;
    padding: 0.3rem 0.625rem;
    background: var(--color-surface-3);
    border: none;
    border-radius: var(--radius-sm);
    color: var(--color-neutral);
    font-size: 0.75rem;
    cursor: pointer;
    transition: background 0.15s, color 0.15s;
  }

  .mcp-copy-btn .material-symbols-outlined {
    font-size: 0.875rem;
  }

  .mcp-copy-btn:hover {
    background: var(--color-surface-4, var(--color-surface-3));
    color: var(--color-neutral-active);
  }

  .mcp-paths {
    background: var(--color-surface-2);
    border-radius: var(--radius-sm);
    padding: 0.875rem 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .mcp-paths-label {
    font-size: 0.75rem;
    color: var(--color-neutral);
    margin: 0 0 0.25rem;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .mcp-path-row {
    display: flex;
    align-items: baseline;
    gap: 0.75rem;
    font-size: 0.8125rem;
  }

  .mcp-os {
    color: var(--color-neutral);
    min-width: 4rem;
    font-size: 0.75rem;
  }

  .mcp-path-row code {
    font-family: monospace;
    color: var(--color-neutral-active);
    font-size: 0.8rem;
    word-break: break-all;
  }

  .mcp-inline-icon {
    font-size: 0.9rem;
    vertical-align: middle;
    color: var(--color-interactive);
  }

  .mcp-chat-hint {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.75rem 1rem;
    background: color-mix(in srgb, var(--color-interactive) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-interactive) 25%, transparent);
    border-radius: var(--radius-sm);
    font-size: 0.8125rem;
    color: var(--color-neutral);
    line-height: 1.5;
  }

  .mcp-chat-hint .material-symbols-outlined {
    font-size: 1rem;
    color: var(--color-interactive);
    flex-shrink: 0;
  }

  .mcp-settings-link {
    background: none;
    border: none;
    padding: 0;
    color: var(--color-interactive);
    font-size: inherit;
    cursor: pointer;
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  .mcp-settings-link:hover {
    color: var(--color-interactive-hover);
  }
</style>
