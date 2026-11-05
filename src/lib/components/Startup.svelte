<script>
  import { onMount, onDestroy } from 'svelte';
  import { goto } from "$app/navigation";
  import { invoke } from "@tauri-apps/api/core";
  import { listen } from "@tauri-apps/api/event";
  import SetupWizard from "$lib/components/SetupWizard.svelte";
  import { Button } from '$lib/components/ui/button';
  import { Input } from '$lib/components/ui/input';

  // null = verificando, true = configurada, false = precisa configurar
  let folderConfigured = $state(null);
  let folderPath = $state('');
  let folderSaving = $state(false);
  let folderError = $state('');

  let setupComplete = $state(null); // null = verificando, true = done, false = not done

  let initError = $state(null);     // string com erro de inicialização, ou null
  let startupFailure = $state(null);
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

  let startupStage = $state('Aguardando serviço local');
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
    stopStartupTracking();
    startupStartedAt = Date.now();
    startupElapsedSec = 0;
    unlistenStartupProgress = await listen('import-progress', (event) => {
      const payload = event.payload;
      if (payload && payload.stage) startupStage = stageLabel(payload.stage);
    });
    startupTimer = setInterval(() => {
      startupElapsedSec = Math.floor((Date.now() - startupStartedAt) / 1000);
    }, 1000);
  }

  function stopStartupTracking() {
    if (startupTimer) { clearInterval(startupTimer); startupTimer = null; }
    if (unlistenStartupProgress) { unlistenStartupProgress(); unlistenStartupProgress = null; }
  }

  onDestroy(() => { stopStartupTracking(); });

  onMount(() => {
    loadStartupConfiguration();
  });

  async function loadStartupConfiguration() {
    startupFailure = null;
    try {
      const configured = await invoke('settings__is_folder_configured');
      folderPath = await invoke('settings__get_foundation_dir');
      if (!configured) {
        folderConfigured = false;
      } else {
        folderConfigured = true;
        await initializeApp();
      }
    } catch (error) {
      startupFailure = { kind: 'settings', message: String(error) };
      folderConfigured = true;
    }
  }

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
    startupFailure = null;
    initError = null;
    setupComplete = null;
    folderSaving = false;

    try {
      startupStage = 'Conectando ao serviço local';
      await beginStartupTracking();
    } catch (err) {
      stopStartupTracking();
      startupFailure = { kind: 'listener', message: String(err) };
      return;
    }

    try {
      await invoke('initialize_app');
    } catch (err) {
      const errorMsg = String(err);
      stopStartupTracking();
      if (isCorruptionError(errorMsg)) {
        initError = errorMsg;
      } else {
        startupFailure = { kind: 'initialize', message: errorMsg };
      }
      return;
    }

    startupStage = 'Verificando configuração';
    try {
      const isSetupDone = await invoke('setup__check');
      stopStartupTracking();
      if (isSetupDone) {
        setupComplete = true;
        await goto('/home');
      } else {
        setupComplete = false;
      }
    } catch (error) {
      stopStartupTracking();
      startupFailure = { kind: 'setup-check', message: String(error) };
    }
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
    const retrySettings = startupFailure?.kind === 'settings' || folderConfigured === null;
    initError = null;
    startupFailure = null;
    if (retrySettings) {
      await loadStartupConfiguration();
    } else {
      await initializeApp();
    }
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

  function startupFailureTitle(kind) {
    switch (kind) {
      case 'settings': return 'Não foi possível consultar a configuração';
      case 'listener': return 'Não foi possível acompanhar a inicialização';
      case 'setup-check': return 'Não foi possível verificar a configuração inicial';
      default: return 'Não foi possível iniciar o serviço local';
    }
  }

  function startupFailureLabel(kind) {
    switch (kind) {
      case 'settings': return 'Leitura das configurações';
      case 'listener': return 'Conexão do acompanhamento';
      case 'setup-check': return 'Verificação do assistente inicial';
      default: return 'Inicialização do serviço local';
    }
  }

  function startupFailureHint(kind) {
    switch (kind) {
      case 'settings': return 'O Foundation não conseguiu ler as configurações locais necessárias para iniciar.';
      case 'listener': return 'O acompanhamento das etapas de inicialização não foi registrado. Tente novamente para conectar ao serviço local.';
      case 'setup-check': return 'O serviço local iniciou, mas não foi possível confirmar se a configuração inicial está concluída.';
      default: return 'O serviço local não confirmou que a inicialização foi concluída.';
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

  function handleSetupComplete(event) {
    setupComplete = true;
    goto("/home");
  }
</script>

<main class="startup-container">
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
          <Input
            type="text"
            bind:value={folderPath}
            placeholder="/caminho/para/pasta"
          />
          <Button variant="ghost" size="icon" onclick={selectFolder} title="Selecionar pasta">
            <span class="material-symbols-outlined">folder_open</span>
          </Button>
        </div>
        {#if folderError}
          <p class="folder-error">{folderError}</p>
        {/if}
        <Button
          class="folder-confirm-btn"
          variant="default"
          onclick={confirmFolder}
          disabled={folderSaving || !folderPath.trim()}
        >
          {folderSaving ? 'Iniciando…' : 'Continuar'}
        </Button>
      </div>
    </div>

  {:else if startupFailure}
    <div class="error-screen" role="alert">
      <h1 class="logo">FOUNDATION</h1>
      <div class="error-card">
        <span class="material-symbols-outlined error-icon">error</span>
        <h2>{startupFailureTitle(startupFailure.kind)}</h2>
        <p class="error-hint">{startupFailureHint(startupFailure.kind)}</p>
        <div class="error-detail">
          <div class="error-detail-label">Etapa</div>
          <div class="error-detail-value">{startupFailureLabel(startupFailure.kind)}</div>
          <div class="error-detail-label">Detalhe</div>
          <div class="error-detail-value error-message">{startupFailure.message}</div>
        </div>
        <div class="error-actions">
          <Button variant="default" class="error-action-btn" onclick={retryInit}>
            <span class="material-symbols-outlined">refresh</span>
            Tentar novamente
          </Button>
        </div>
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
            <Button variant="default" class="error-action-btn" onclick={startRecovery} disabled={recoveryRunning}>
              <span class="material-symbols-outlined">healing</span>
              Recuperar dados automaticamente
            </Button>
            <Button variant="secondary" class="error-action-btn" onclick={switchFolder} disabled={switchingFolder}>
              <span class="material-symbols-outlined">drive_file_move</span>
              {switchingFolder ? 'Reiniciando…' : 'Escolher outra pasta de dados'}
            </Button>
          {:else}
            <Button variant="default" class="error-action-btn" onclick={switchFolder} disabled={switchingFolder}>
              <span class="material-symbols-outlined">drive_file_move</span>
              {switchingFolder ? 'Reiniciando…' : 'Escolher outra pasta de dados'}
            </Button>
          {/if}
          <Button variant="secondary" class="error-action-btn" onclick={openDataFolder}>
            <span class="material-symbols-outlined">folder_open</span>
            Abrir pasta atual no Explorer
          </Button>
          <Button variant="secondary" class="error-action-btn" onclick={retryInit}>
            <span class="material-symbols-outlined">refresh</span>
            Tentar novamente
          </Button>
          <Button variant="ghost" class="error-action-btn" onclick={copyErrorDetails}>
            <span class="material-symbols-outlined">content_copy</span>
            {copyFeedback || 'Copiar detalhes'}
          </Button>
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
              <Button variant="default" class="error-action-btn" onclick={finishRecoveryAndReload}>
                <span class="material-symbols-outlined">restart_alt</span>
                Continuar
              </Button>
              <Button variant="secondary" class="error-action-btn" onclick={openRecoveryWorkspace}>
                <span class="material-symbols-outlined">folder_open</span>
                Abrir pasta do backup
              </Button>
            </div>
          {:else if recoveryError}
            <span class="material-symbols-outlined recovery-icon error-color">error</span>
            <h2>Falha na recuperação</h2>
            <p class="recovery-error-msg">{recoveryError}</p>
            <p class="recovery-hint">O arquivo original foi restaurado. Você pode tentar novamente ou usar outra pasta de dados.</p>
            <Button variant="secondary" class="error-action-btn" onclick={() => { recoveryError = null; }}>Fechar</Button>
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

  {:else if setupComplete === null}
    <section class="startup-screen">
      <div class="startup-grid" aria-hidden="true"></div>
      <div class="startup-panel">
        <div class="startup-panel-header">
          <span class="startup-system-label"><span class="startup-signal"></span> SEQUÊNCIA DE INICIALIZAÇÃO</span>
          <span class="startup-local-label">NÚCLEO LOCAL</span>
        </div>

        <div class="startup-copy">
          <p class="startup-kicker">STATUS DO PROCESSO</p>
          <h1 aria-live="polite" aria-atomic="true">{startupStage}</h1>
        </div>

        <div class="startup-progress" role="progressbar" aria-label="Inicialização em andamento" aria-valuetext={startupStage}><span></span></div>

        <div class="startup-panel-footer">
          <span>PREPARANDO A SESSÃO</span>
          <span class="startup-elapsed" aria-hidden="true">{formatElapsed(startupElapsedSec)}</span>
        </div>
      </div>
    </section>

  {:else if setupComplete === false}
    <SetupWizard onComplete={handleSetupComplete} />

  {:else}
    <p class="redirecting">Carregando…</p>
  {/if}
</main>

<style>
  .startup-container {
    display: flex;
    align-items: center;
    justify-content: center;
    height: 100%;
    width: 100%;
    background: var(--color-black);
  }

  .startup-screen {
    position: relative;
    isolation: isolate;
    display: grid;
    place-items: center;
    width: 100%;
    height: 100%;
    overflow: hidden;
    background: var(--color-black);
  }

  .startup-grid {
    position: absolute;
    z-index: -1;
    inset: 0;
    background-image: radial-gradient(circle, var(--color-hud-line-subtle) var(--size-hud-grid-dot), transparent var(--size-hud-grid-dot));
    background-size: var(--size-hud-grid) var(--size-hud-grid);
    opacity: var(--opacity-startup-grid);
  }

  .startup-panel {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
    width: calc(100% - var(--space-6) * 2);
    min-width: 0;
    padding: var(--space-5);
    background: color-mix(in srgb, var(--color-surface-1) var(--opacity-widget-surface), transparent);
    backdrop-filter: blur(var(--blur-widget-surface));
  }

  .startup-panel::before {
    position: absolute;
    inset: 0;
    background:
      linear-gradient(var(--color-interactive), var(--color-interactive)) top left / var(--size-hud-frame-run) var(--size-hud-frame-line) no-repeat,
      linear-gradient(var(--color-interactive), var(--color-interactive)) top left / var(--size-hud-frame-line) var(--size-hud-frame-run) no-repeat,
      linear-gradient(var(--color-interactive), var(--color-interactive)) top right / var(--size-hud-frame-run) var(--size-hud-frame-line) no-repeat,
      linear-gradient(var(--color-interactive), var(--color-interactive)) top right / var(--size-hud-frame-line) var(--size-hud-frame-run) no-repeat,
      linear-gradient(var(--color-interactive), var(--color-interactive)) bottom left / var(--size-hud-frame-run) var(--size-hud-frame-line) no-repeat,
      linear-gradient(var(--color-interactive), var(--color-interactive)) bottom left / var(--size-hud-frame-line) var(--size-hud-frame-run) no-repeat,
      linear-gradient(var(--color-interactive), var(--color-interactive)) bottom right / var(--size-hud-frame-run) var(--size-hud-frame-line) no-repeat,
      linear-gradient(var(--color-interactive), var(--color-interactive)) bottom right / var(--size-hud-frame-line) var(--size-hud-frame-run) no-repeat;
    content: '';
    pointer-events: none;
  }

  .startup-panel-header,
  .startup-panel-footer {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: var(--space-3);
    min-width: 0;
  }

  .startup-system-label,
  .startup-local-label,
  .startup-panel-footer {
    font-family: var(--font-mono);
    font-size: var(--type-overline);
    letter-spacing: var(--tracking-hud-label);
    color: var(--color-neutral-secondary);
  }

  .startup-system-label {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--color-interactive);
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .startup-signal {
    width: var(--size-hud-marker);
    height: var(--size-hud-marker);
    background: var(--color-interactive);
    animation: startup-signal 1.8s ease-in-out infinite;
  }

  .startup-copy {
    display: flex;
    flex-direction: column;
    gap: var(--spacing-label-to-value);
  }

  .startup-kicker {
    margin: 0;
    color: var(--color-neutral-secondary);
    font-size: var(--type-caption);
    letter-spacing: var(--tracking-hud-label);
  }

  .startup-copy h1 {
    margin: 0;
    color: var(--color-neutral-active);
    font-size: var(--type-heading);
    line-height: 1.4;
    overflow-wrap: anywhere;
  }

  .startup-progress {
    position: relative;
    height: var(--size-startup-progress-height);
    overflow: hidden;
    background: var(--color-surface-3);
  }

  .startup-progress span {
    position: absolute;
    inset-block: 0;
    left: calc(-1 * var(--size-startup-progress-sweep));
    width: var(--size-startup-progress-sweep);
    background: linear-gradient(90deg, transparent, var(--color-interactive), transparent);
    animation: startup-sweep 2.4s ease-in-out infinite;
  }

  .startup-elapsed {
    color: var(--color-neutral-active);
    font-variant-numeric: tabular-nums;
  }

  @keyframes startup-signal {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.35; }
  }

  @keyframes startup-sweep {
    to { left: 100%; }
  }

  @media (prefers-reduced-motion: reduce) {
    .startup-signal,
    .startup-progress span {
      animation: none;
    }

    .startup-progress span {
      left: 0;
      width: 100%;
      opacity: 0.55;
    }
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
    border-radius: var(--radius);
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

  .folder-row :global([data-slot="input"]) {
    flex: 1;
    min-width: 0;
  }

  .folder-error {
    font-size: 0.8125rem;
    color: var(--color-danger);
    margin: 0;
    text-align: center;
  }

  :global(.folder-confirm-btn) {
    width: 100% !important;
    margin-top: 0.5rem;
    text-transform: uppercase;
    letter-spacing: 0.125rem;
    padding-top: 0.875rem !important;
    padding-bottom: 0.875rem !important;
    font-size: 1rem !important;
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
    border-radius: var(--radius);
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
    border-radius: var(--radius);
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
    border-radius: var(--radius);
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

  :global(.error-action-btn) {
    width: 100% !important;
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
    border-radius: var(--radius);
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
    border-radius: var(--radius);
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
    color: var(--color-success);
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
    border-radius: var(--radius);
    margin: 0;
    max-width: 100%;
    word-break: break-word;
    text-align: left;
    font-family: var(--font-mono, monospace);
    max-height: 8rem;
    overflow-y: auto;
  }

  .recovery-modal :global(.error-action-btn) {
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
    border-radius: var(--radius);
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
