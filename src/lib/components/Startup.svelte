<script>
  import { onMount, onDestroy } from 'svelte';
  import { goto } from "$app/navigation";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import SetupWizard from "$lib/components/SetupWizard.svelte";
  import ImportProgress from "$lib/components/ImportProgress.svelte";
  import Activity from "$lib/components/Activity.svelte";

  // null = verificando, true = configurada, false = precisa configurar
  let folderConfigured = $state(null);
  let folderPath = $state('');
  let folderSaving = $state(false);
  let folderError = $state('');

  let setupComplete = $state(null); // null = verificando, true = done, false = not done
  let importing = $state(null);     // null = verificando, true = importing, false = already imported

  let startupStage = $state('Verificando banco de dados');
  let startupStartedAt = $state(0);
  let startupElapsedSec = $state(0);
  let startupTimer = null;
  let unlistenStartupProgress = null;

  function formatElapsed(sec) {
    const m = Math.floor(sec / 60);
    const s = sec % 60;
    return m > 0 ? `${m}m ${s.toString().padStart(2, '0')}s` : `${s}s`;
  }

  async function beginStartupTracking() {
    startupStartedAt = Date.now();
    startupElapsedSec = 0;
    startupTimer = setInterval(() => {
      startupElapsedSec = Math.floor((Date.now() - startupStartedAt) / 1000);
    }, 1000);
    unlistenStartupProgress = await listen('import-progress', (event) => {
      const payload = event.payload;
      if (payload && payload.stage) startupStage = payload.stage;
    });
  }

  function stopStartupTracking() {
    if (startupTimer) { clearInterval(startupTimer); startupTimer = null; }
    if (unlistenStartupProgress) { unlistenStartupProgress(); unlistenStartupProgress = null; }
  }

  onDestroy(() => { stopStartupTracking(); });

  onMount(async () => {
    try {
      const configured = await invoke('settings__is_folder_configured');
      folderPath = await invoke('settings__get_foundation_dir');
      if (!configured) {
        folderConfigured = false;
      } else {
        folderConfigured = true;
        await beginStartupTracking();
        await initializeApp();
      }
    } catch {
      folderConfigured = true;
      await beginStartupTracking();
      await initializeApp();
    }
  });

  async function selectFolder() {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const selected = await open({ directory: true, title: 'Selecionar pasta de dados do Foundation' });
    if (selected) folderPath = selected;
  }

  async function confirmFolder() {
    if (!folderPath.trim()) return;
    folderSaving = true;
    folderError = '';
    try {
      await invoke('settings__save_foundation_dir', { path: folderPath });
      folderConfigured = true;
      await initializeApp();
    } catch (e) {
      folderError = String(e);
      folderSaving = false;
    }
  }

  async function initializeApp() {
    try {
      await invoke('initialize_app');
    } catch (err) {
      const errorMsg = String(err);
      if (!errorMsg.includes('state already managed')) {
        console.error('[Startup] Failed to initialize database:', err);
      }
    }
    checkDatabaseStatus();
  }

  async function checkDatabaseStatus() {
    try {
      const isSetupDone = await invoke('setup__check');
      stopStartupTracking();
      importing = false;
      if (isSetupDone) {
        setupComplete = true;
        goto("/home");
      } else {
        setupComplete = false;
      }
    } catch (error) {
      const errorMsg = String(error);

      if (errorMsg.includes('state not managed')) {
        if (importing === null) {
          stopStartupTracking();
          importing = true;
        }
        const unlisten = await listen('import-complete', async () => {
          unlisten();
          await checkDatabaseStatus();
        });
      } else {
        stopStartupTracking();
        importing = true;
      }
    }
  }

  async function handleImportComplete() {
    importing = false;

    try {
      const isDone = await invoke('setup__check');

      if (isDone) {
        setupComplete = true;
        goto("/home");
      } else {
        setupComplete = false;
      }
    } catch (error) {
      const errorMsg = String(error);

      if (errorMsg.includes('state not managed') || errorMsg.includes('conn')) {
        setTimeout(() => checkDatabaseStatus(), 500);
      } else {
        setupComplete = false;
      }
    }
  }

  function handleSetupComplete(event) {
    setupComplete = true;
    goto("/home");
  }
</script>

<main class="container">
  {#if folderConfigured === null}
    <p class="redirecting">Iniciando…</p>

  {:else if folderConfigured === false}
    <div class="folder-screen">
      <h1 class="logo">FOUNDATION</h1>
      <div class="folder-card">
        <span class="material-symbols-outlined folder-icon">folder_open</span>
        <h2>Onde salvar seus dados?</h2>
        <p class="folder-hint">Escolha a pasta onde o FOUNDATION vai armazenar seu banco de dados e arquivos.</p>
        <div class="folder-row">
          <input
            class="folder-input"
            type="text"
            bind:value={folderPath}
            placeholder="/caminho/para/pasta"
          />
          <button class="folder-browse-btn" onclick={selectFolder}>
            <span class="material-symbols-outlined">folder_open</span>
          </button>
        </div>
        {#if folderError}
          <p class="folder-error">{folderError}</p>
        {/if}
        <button
          class="folder-confirm-btn"
          onclick={confirmFolder}
          disabled={folderSaving || !folderPath.trim()}
        >
          {folderSaving ? 'Iniciando…' : 'Continuar'}
        </button>
      </div>
    </div>

  {:else if importing === null}
    <Activity message={`${startupStage} · ${formatElapsed(startupElapsedSec)}`} progress={null} />

  {:else if importing === true}
    <ImportProgress onComplete={handleImportComplete} />

  {:else if setupComplete === null}
    <p class="redirecting">Verificando configuração…</p>

  {:else if setupComplete === false}
    <SetupWizard onComplete={handleSetupComplete} />

  {:else}
    <p class="redirecting">Carregando…</p>
  {/if}
</main>

<style>
  .container {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100vh;
    background: var(--color-black);
  }

  .redirecting {
    color: var(--color-neutral);
    font-size: 1.2em;
  }

  /* --- Folder selection screen --- */

  .folder-screen {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2.5rem;
  }

  .logo {
    font-size: 1.5rem;
    color: var(--color-neutral);
    margin: 0;
    text-transform: uppercase;
    letter-spacing: 0.125rem;
    font-weight: 600;
  }

  .folder-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1.25rem;
    background: var(--color-surface-1);
    border-radius: var(--radius-lg);
    padding: 2.5rem 2rem;
    width: 480px;
    max-width: 95vw;
  }

  .folder-icon {
    font-size: 2.5rem;
    color: var(--color-interactive);
    opacity: 0.8;
  }

  .folder-card h2 {
    font-size: 1.125rem;
    color: var(--color-neutral-active);
    margin: 0;
    text-align: center;
  }

  .folder-hint {
    font-size: 0.875rem;
    color: var(--color-neutral);
    text-align: center;
    margin: 0;
    line-height: 1.5;
    opacity: 0.8;
  }

  .folder-row {
    display: flex;
    gap: 0.5rem;
    width: 100%;
  }

  .folder-input {
    flex: 1;
    background: var(--color-surface-2);
    color: var(--color-neutral-active);
    border: none;
    padding: 0.625rem 0.875rem;
    font-size: 0.875rem;
    border-radius: var(--radius-sm);
    min-width: 0;
  }

  .folder-input:focus {
    outline: none;
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--color-interactive) 30%, transparent);
  }

  .folder-browse-btn {
    background: var(--color-surface-2);
    color: var(--color-neutral);
    border: none;
    padding: 0.625rem 0.75rem;
    border-radius: var(--radius-sm);
    cursor: pointer;
    display: flex;
    align-items: center;
    transition: background 0.15s;
  }

  .folder-browse-btn:hover {
    background: var(--color-surface-3);
    color: var(--color-neutral-active);
  }

  .folder-browse-btn .material-symbols-outlined {
    font-size: 1.125rem;
  }

  .folder-error {
    font-size: 0.8125rem;
    color: var(--color-danger);
    margin: 0;
    text-align: center;
  }

  .folder-confirm-btn {
    background: var(--color-interactive);
    color: var(--color-neutral-on-interactive);
    border: none;
    padding: 0.875rem 3rem;
    font-size: 1rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.125rem;
    cursor: pointer;
    transition: all 0.2s;
    margin-top: 0.5rem;
    width: 100%;
  }

  .folder-confirm-btn:hover:not(:disabled) {
    background: var(--color-interactive-hover);
    transform: translateY(-1px);
    box-shadow: 0 4px 16px color-mix(in srgb, var(--color-interactive) 40%, transparent);
  }

  .folder-confirm-btn:disabled {
    background: var(--color-interactive-disabled);
    cursor: not-allowed;
    transform: none;
  }
</style>
