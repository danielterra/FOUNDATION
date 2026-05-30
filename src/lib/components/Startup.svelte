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

  let initError = $state(null);     // string com erro de inicialização, ou null
  let switchingFolder = $state(false);
  let copyFeedback = $state('');

  let recoveryRunning = $state(false);
  let recoveryStage = $state('');
  let recoveryDetail = $state('');
  let recoveryBytes = $state(0);
  let recoveryTotal = $state(0);
  let recoveryRate = $state(0);
  let recoveryStartedAt = $state(0);
  let recoveryElapsedSec = $state(0);
  let recoveryTimer = null;
  let recoveryError = $state(null);
  let recoveryResult = $state(null);
  let unlistenRecovery = null;

  const RECOVERY_STEPS = [
    { key: 'verifying', label: 'Verificando' },
    { key: 'backup',    label: 'Backup' },
    { key: 'dumping',   label: 'Extraindo' },
    { key: 'importing', label: 'Importando' },
    { key: 'finalizing',label: 'Finalizando' },
  ];

  function recoveryStepIndex(stage) {
    const i = RECOVERY_STEPS.findIndex(s => s.key === stage);
    return i < 0 ? 0 : i;
  }

  function formatBytes(n) {
    if (!n || n < 0) return '0 B';
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    if (n < 1024 * 1024 * 1024) return `${(n / (1024 * 1024)).toFixed(1)} MB`;
    return `${(n / (1024 * 1024 * 1024)).toFixed(2)} GB`;
  }

  function formatRate(bps) {
    if (!bps || bps <= 0) return '—';
    return `${formatBytes(bps)}/s`;
  }

  function formatETA(bytes, total, bps) {
    if (!total || !bps || bps <= 0 || bytes >= total) return '—';
    const remaining = (total - bytes) / bps;
    if (!isFinite(remaining)) return '—';
    if (remaining < 60) return `~${Math.ceil(remaining)}s restantes`;
    const m = Math.floor(remaining / 60);
    const s = Math.round(remaining % 60);
    return `~${m}m ${s.toString().padStart(2, '0')}s restantes`;
  }

  function recoveryPercent() {
    if (!recoveryTotal) return 0;
    return Math.min(99, Math.max(0, Math.floor((recoveryBytes / recoveryTotal) * 100)));
  }

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
      if (errorMsg.includes('state already managed')) {
        checkDatabaseStatus();
        return;
      }
      console.error('[Startup] Failed to initialize database:', err);
      stopStartupTracking();
      initError = errorMsg;
      return;
    }
    checkDatabaseStatus();
  }

  function isCorruptionError(msg) {
    return /malformed|DatabaseCorrupt|disk image/i.test(msg ?? '');
  }

  function isCloudSyncedPath(path) {
    return /iCloud|OneDrive|Dropbox|Google Drive|GoogleDrive|Box Sync|pCloud/i.test(path ?? '');
  }

  async function openDataFolder() {
    try {
      const { revealItemInDir } = await import('@tauri-apps/plugin-opener');
      await revealItemInDir(folderPath);
    } catch (e) {
      console.error('[Startup] revealItemInDir failed:', e);
    }
  }

  async function copyErrorDetails() {
    const details = `Pasta: ${folderPath}\nErro: ${initError}`;
    try {
      await navigator.clipboard.writeText(details);
      copyFeedback = 'Copiado!';
      setTimeout(() => { copyFeedback = ''; }, 2000);
    } catch {
      copyFeedback = 'Falha ao copiar';
      setTimeout(() => { copyFeedback = ''; }, 2000);
    }
  }

  async function switchFolder() {
    if (switchingFolder) return;
    switchingFolder = true;
    try {
      const { open } = await import('@tauri-apps/plugin-dialog');
      const selected = await open({ directory: true, title: 'Selecionar nova pasta de dados do Foundation' });
      if (!selected) {
        switchingFolder = false;
        return;
      }
      await invoke('settings__set_foundation_dir', { path: selected });
    } catch (e) {
      console.error('[Startup] switchFolder failed:', e);
      switchingFolder = false;
    }
  }

  async function retryInit() {
    initError = null;
    await beginStartupTracking();
    await initializeApp();
  }

  function stageLabel(stage) {
    switch (stage) {
      case 'verifying': return 'Verificando banco';
      case 'backup': return 'Preservando arquivo corrompido';
      case 'dumping': return 'Extraindo dados';
      case 'importing': return 'Importando para banco novo';
      case 'finalizing': return 'Finalizando';
      case 'rollback': return 'Restaurando backup';
      case 'done': return 'Concluído';
      default: return stage || 'Preparando';
    }
  }

  async function startRecovery() {
    if (recoveryRunning) return;
    recoveryRunning = true;
    recoveryError = null;
    recoveryResult = null;
    recoveryStage = 'verifying';
    recoveryDetail = '';
    recoveryBytes = 0;
    recoveryTotal = 0;
    recoveryRate = 0;
    recoveryStartedAt = Date.now();
    recoveryElapsedSec = 0;
    recoveryTimer = setInterval(() => {
      recoveryElapsedSec = Math.floor((Date.now() - recoveryStartedAt) / 1000);
    }, 1000);

    unlistenRecovery = await listen('recovery-progress', (event) => {
      const p = event.payload ?? {};
      if (p.stage) {
        if (p.stage !== recoveryStage) {
          recoveryBytes = 0;
          recoveryTotal = 0;
          recoveryRate = 0;
        }
        recoveryStage = p.stage;
      }
      if (p.detail !== undefined) recoveryDetail = p.detail;
      if (typeof p.bytes === 'number') recoveryBytes = p.bytes;
      if (typeof p.total_bytes === 'number') recoveryTotal = p.total_bytes;
      if (typeof p.rate_bps === 'number') recoveryRate = p.rate_bps;
    });

    try {
      recoveryResult = await invoke('recover_database');
      recoveryStage = 'done';
    } catch (e) {
      recoveryError = String(e);
    } finally {
      if (unlistenRecovery) { unlistenRecovery(); unlistenRecovery = null; }
      if (recoveryTimer) { clearInterval(recoveryTimer); recoveryTimer = null; }
      recoveryRunning = false;
    }
  }

  async function finishRecoveryAndReload() {
    window.location.reload();
  }

  async function openRecoveryWorkspace() {
    if (!recoveryResult?.workspace) return;
    try {
      const { revealItemInDir } = await import('@tauri-apps/plugin-opener');
      await revealItemInDir(recoveryResult.workspace);
    } catch (e) {
      console.error('[Startup] open workspace failed:', e);
    }
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

  {:else if initError}
    <div class="error-screen">
      <h1 class="logo">FOUNDATION</h1>
      <div class="error-card">
        <span class="material-symbols-outlined error-icon">error</span>
        <h2>
          {#if isCorruptionError(initError)}
            Banco de dados corrompido
          {:else}
            Não foi possível abrir o banco de dados
          {/if}
        </h2>

        {#if isCorruptionError(initError)}
          <p class="error-hint">
            O arquivo <code>FOUNDATION.db</code> está com a imagem em disco danificada e não pôde ser aberto.
          </p>
          {#if isCloudSyncedPath(folderPath)}
            <p class="error-warning">
              <span class="material-symbols-outlined inline-icon">cloud_sync</span>
              A pasta de dados está em um serviço de sincronização em nuvem. SQLite e iCloud/OneDrive/Dropbox não funcionam juntos com segurança — a sincronização parcial de páginas enquanto o app está aberto é a causa mais comum desse tipo de corrupção. Recomendamos mover os dados para uma pasta local.
            </p>
          {/if}
        {:else}
          <p class="error-hint">Detalhes técnicos abaixo. Tente uma das opções para recuperar o app.</p>
        {/if}

        <div class="error-detail">
          <div class="error-detail-label">Pasta de dados</div>
          <div class="error-detail-value">{folderPath}</div>
          <div class="error-detail-label">Erro</div>
          <div class="error-detail-value error-message">{initError}</div>
        </div>

        <div class="error-actions">
          {#if isCorruptionError(initError)}
            <button class="error-btn primary" onclick={startRecovery} disabled={recoveryRunning}>
              <span class="material-symbols-outlined">healing</span>
              Recuperar dados automaticamente
            </button>
            <button class="error-btn" onclick={switchFolder} disabled={switchingFolder}>
              <span class="material-symbols-outlined">drive_file_move</span>
              {switchingFolder ? 'Reiniciando…' : 'Escolher outra pasta de dados'}
            </button>
          {:else}
            <button class="error-btn primary" onclick={switchFolder} disabled={switchingFolder}>
              <span class="material-symbols-outlined">drive_file_move</span>
              {switchingFolder ? 'Reiniciando…' : 'Escolher outra pasta de dados'}
            </button>
          {/if}
          <button class="error-btn" onclick={openDataFolder}>
            <span class="material-symbols-outlined">folder_open</span>
            Abrir pasta atual no Explorer
          </button>
          <button class="error-btn" onclick={retryInit}>
            <span class="material-symbols-outlined">refresh</span>
            Tentar novamente
          </button>
          <button class="error-btn ghost" onclick={copyErrorDetails}>
            <span class="material-symbols-outlined">content_copy</span>
            {copyFeedback || 'Copiar detalhes'}
          </button>
        </div>

        {#if isCorruptionError(initError)}
          <p class="error-hint recovery-hint">
            A recuperação automática preserva o arquivo corrompido como <code>.corrupt-DATA.bak</code> antes de tentar reconstruir o banco.
          </p>
        {/if}
      </div>
    </div>

    {#if recoveryRunning || recoveryResult || recoveryError}
      <div class="recovery-modal-backdrop">
        <div class="recovery-modal">
          {#if recoveryResult && !recoveryError}
            <span class="material-symbols-outlined recovery-icon success">check_circle</span>
            <h2>Banco recuperado</h2>
            <p class="recovery-summary">
              {recoveryResult.triples_recovered?.toLocaleString('pt-BR') ?? 0} triples recuperados.
            </p>
            <p class="recovery-hint">
              O arquivo corrompido foi preservado em:
              <br /><code>{recoveryResult.workspace}</code>
            </p>
            <div class="recovery-actions">
              <button class="error-btn primary" onclick={finishRecoveryAndReload}>
                <span class="material-symbols-outlined">restart_alt</span>
                Continuar
              </button>
              <button class="error-btn" onclick={openRecoveryWorkspace}>
                <span class="material-symbols-outlined">folder_open</span>
                Abrir pasta do backup
              </button>
            </div>
          {:else if recoveryError}
            <span class="material-symbols-outlined recovery-icon error-color">error</span>
            <h2>Falha na recuperação</h2>
            <p class="recovery-error-msg">{recoveryError}</p>
            <p class="recovery-hint">O arquivo original foi restaurado. Você pode tentar novamente ou usar outra pasta de dados.</p>
            <button class="error-btn" onclick={() => { recoveryError = null; }}>Fechar</button>
          {:else}
            <span class="material-symbols-outlined recovery-icon spinning">progress_activity</span>
            <h2>Recuperando dados…</h2>

            <div class="recovery-timeline">
              {#each RECOVERY_STEPS as step, i}
                {@const idx = recoveryStepIndex(recoveryStage)}
                <div class="timeline-step" class:done={i < idx} class:active={i === idx}>
                  <span class="timeline-dot">
                    {#if i < idx}
                      <span class="material-symbols-outlined">check</span>
                    {:else if i === idx}
                      <span class="timeline-pulse"></span>
                    {:else}
                      {i + 1}
                    {/if}
                  </span>
                  <span class="timeline-label">{step.label}</span>
                </div>
              {/each}
            </div>

            <p class="recovery-stage">{stageLabel(recoveryStage)}</p>

            {#if recoveryTotal > 0}
              <div class="recovery-bar">
                <div class="recovery-bar-fill" style="width: {recoveryPercent()}%"></div>
              </div>
              <div class="recovery-metrics">
                <span>{formatBytes(recoveryBytes)} / {formatBytes(recoveryTotal)}</span>
                <span>{recoveryPercent()}%</span>
                <span>{formatRate(recoveryRate)}</span>
              </div>
              <p class="recovery-eta">{formatETA(recoveryBytes, recoveryTotal, recoveryRate)}</p>
            {:else if recoveryDetail}
              <p class="recovery-detail">{recoveryDetail}</p>
            {/if}

            <p class="recovery-elapsed">
              Tempo decorrido: <strong>{formatElapsed(recoveryElapsedSec)}</strong>
            </p>
            <p class="recovery-hint">Não feche o aplicativo. Isso pode levar alguns minutos em bancos grandes.</p>
          {/if}
        </div>
      </div>
    {/if}

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

  /* --- Error screen --- */

  .error-screen {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 2rem;
    width: 100%;
    max-width: 95vw;
  }

  .error-card {
    display: flex;
    flex-direction: column;
    align-items: stretch;
    gap: 1rem;
    background: var(--color-surface-1);
    border-radius: var(--radius-lg);
    padding: 2rem;
    width: 600px;
    max-width: 95vw;
  }

  .error-icon {
    font-size: 2.5rem;
    color: var(--color-danger);
    align-self: center;
  }

  .error-card h2 {
    font-size: 1.25rem;
    color: var(--color-neutral-active);
    margin: 0;
    text-align: center;
  }

  .error-hint {
    font-size: 0.875rem;
    color: var(--color-neutral);
    margin: 0;
    line-height: 1.5;
    text-align: center;
  }

  .error-warning {
    font-size: 0.875rem;
    color: var(--color-neutral-active);
    background: color-mix(in srgb, var(--color-danger) 12%, transparent);
    border-left: 3px solid var(--color-danger);
    padding: 0.75rem 1rem;
    margin: 0;
    line-height: 1.5;
    border-radius: var(--radius-sm);
    display: flex;
    align-items: flex-start;
    gap: 0.5rem;
  }

  .inline-icon {
    font-size: 1.125rem;
    color: var(--color-danger);
    flex-shrink: 0;
  }

  .error-detail {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 0.5rem 1rem;
    background: var(--color-surface-2);
    padding: 0.875rem 1rem;
    border-radius: var(--radius-sm);
    font-size: 0.8125rem;
  }

  .error-detail-label {
    color: var(--color-neutral);
    opacity: 0.7;
    white-space: nowrap;
  }

  .error-detail-value {
    color: var(--color-neutral-active);
    font-family: var(--font-mono, monospace);
    word-break: break-all;
    overflow-wrap: anywhere;
  }

  .error-message {
    max-height: 6rem;
    overflow-y: auto;
  }

  .error-actions {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    margin-top: 0.5rem;
  }

  .error-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    background: var(--color-surface-2);
    color: var(--color-neutral-active);
    border: none;
    padding: 0.75rem 1rem;
    font-size: 0.875rem;
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: background 0.15s;
  }

  .error-btn:hover:not(:disabled) {
    background: var(--color-surface-3);
  }

  .error-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .error-btn .material-symbols-outlined {
    font-size: 1.125rem;
  }

  .error-btn.primary {
    background: var(--color-interactive);
    color: var(--color-neutral-on-interactive);
    font-weight: 600;
  }

  .error-btn.primary:hover:not(:disabled) {
    background: var(--color-interactive-hover);
  }

  .error-btn.ghost {
    background: transparent;
    color: var(--color-neutral);
  }

  .error-btn.ghost:hover {
    background: var(--color-surface-2);
    color: var(--color-neutral-active);
  }

  .recovery-hint {
    margin-top: 0.5rem;
    font-size: 0.8125rem;
    opacity: 0.8;
  }

  .recovery-hint code,
  .error-hint code {
    background: var(--color-surface-2);
    padding: 0.125rem 0.375rem;
    border-radius: 3px;
    font-family: var(--font-mono, monospace);
    font-size: 0.8125rem;
  }

  /* --- Recovery modal --- */

  .recovery-modal-backdrop {
    position: fixed;
    inset: 0;
    background: color-mix(in srgb, var(--color-black) 70%, transparent);
    backdrop-filter: blur(4px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
  }

  .recovery-modal {
    background: var(--color-surface-1);
    border-radius: var(--radius-lg);
    padding: 2.5rem 2rem;
    width: 520px;
    max-width: 95vw;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.875rem;
    text-align: center;
  }

  .recovery-modal h2 {
    margin: 0;
    font-size: 1.25rem;
    color: var(--color-neutral-active);
  }

  .recovery-icon {
    font-size: 3rem;
  }

  .recovery-icon.success {
    color: var(--color-success, #4ade80);
  }

  .recovery-icon.error-color {
    color: var(--color-danger);
  }

  .recovery-icon.spinning {
    color: var(--color-interactive);
    animation: spin 1.4s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  .recovery-stage {
    font-size: 1rem;
    color: var(--color-neutral-active);
    margin: 0;
    font-weight: 500;
  }

  .recovery-detail {
    font-size: 0.875rem;
    color: var(--color-neutral);
    margin: 0;
    font-family: var(--font-mono, monospace);
  }

  .recovery-summary {
    font-size: 0.9375rem;
    color: var(--color-neutral-active);
    margin: 0;
  }

  .recovery-error-msg {
    font-size: 0.875rem;
    color: var(--color-danger);
    background: color-mix(in srgb, var(--color-danger) 10%, transparent);
    padding: 0.75rem 1rem;
    border-radius: var(--radius-sm);
    margin: 0;
    max-width: 100%;
    word-break: break-word;
    text-align: left;
    font-family: var(--font-mono, monospace);
    max-height: 8rem;
    overflow-y: auto;
  }

  .recovery-modal .error-btn {
    margin-top: 0.5rem;
    min-width: 200px;
  }

  .recovery-actions {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    width: 100%;
    align-items: center;
  }

  .recovery-timeline {
    display: flex;
    align-items: center;
    justify-content: space-between;
    width: 100%;
    margin: 0.5rem 0;
    position: relative;
  }

  .recovery-timeline::before {
    content: '';
    position: absolute;
    top: 0.875rem;
    left: 1rem;
    right: 1rem;
    height: 2px;
    background: var(--color-surface-2);
    z-index: 0;
  }

  .timeline-step {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.375rem;
    position: relative;
    z-index: 1;
    flex: 1;
  }

  .timeline-dot {
    width: 1.75rem;
    height: 1.75rem;
    border-radius: 50%;
    background: var(--color-surface-2);
    color: var(--color-neutral);
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 0.75rem;
    font-weight: 600;
    border: 2px solid var(--color-surface-2);
    transition: all 0.25s;
  }

  .timeline-dot .material-symbols-outlined {
    font-size: 1rem;
  }

  .timeline-step.done .timeline-dot {
    background: var(--color-interactive);
    color: var(--color-neutral-on-interactive);
    border-color: var(--color-interactive);
  }

  .timeline-step.active .timeline-dot {
    background: var(--color-surface-1);
    border-color: var(--color-interactive);
    color: var(--color-interactive);
  }

  .timeline-pulse {
    width: 0.625rem;
    height: 0.625rem;
    border-radius: 50%;
    background: var(--color-interactive);
    animation: pulse 1.2s ease-in-out infinite;
  }

  @keyframes pulse {
    0%, 100% { transform: scale(1); opacity: 1; }
    50% { transform: scale(0.6); opacity: 0.6; }
  }

  .timeline-label {
    font-size: 0.6875rem;
    color: var(--color-neutral);
    opacity: 0.75;
    text-align: center;
  }

  .timeline-step.active .timeline-label,
  .timeline-step.done .timeline-label {
    color: var(--color-neutral-active);
    opacity: 1;
  }

  .recovery-bar {
    width: 100%;
    height: 8px;
    background: var(--color-surface-2);
    border-radius: 4px;
    overflow: hidden;
    margin-top: 0.25rem;
  }

  .recovery-bar-fill {
    height: 100%;
    background: linear-gradient(90deg, var(--color-interactive), color-mix(in srgb, var(--color-interactive) 70%, white));
    transition: width 0.3s ease;
  }

  .recovery-metrics {
    display: flex;
    justify-content: space-between;
    width: 100%;
    font-size: 0.75rem;
    color: var(--color-neutral);
    font-family: var(--font-mono, monospace);
  }

  .recovery-metrics span:nth-child(2) {
    color: var(--color-neutral-active);
    font-weight: 600;
  }

  .recovery-eta {
    font-size: 0.8125rem;
    color: var(--color-neutral-active);
    margin: 0;
  }

  .recovery-elapsed {
    font-size: 0.75rem;
    color: var(--color-neutral);
    margin: 0;
    opacity: 0.8;
  }

  .recovery-elapsed strong {
    color: var(--color-neutral-active);
    font-family: var(--font-mono, monospace);
  }
</style>
