<script lang="ts">
  import '$lib/base.css';
  import '$lib/fonts.css';
  import '$lib/colors.css';
  import '$lib/markdown.css';
  import { initializeLogging } from '$lib/logging.js';
  import { onMount, onDestroy } from 'svelte';
  const { children } = $props();
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { deleteConfirm } from '$lib/stores/deleteConfirm';
  import GlobalModal from '$lib/components/GlobalModal.svelte';
  import ThinkingDots from '$lib/components/ThinkingDots.svelte';
  import AppHeader from '$lib/components/AppHeader.svelte';
  import { appState } from '$lib/appState.svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';

  type TaskCompletedEvent = {
    task_iri: string;
    label: string;
    result_summary: string;
  };

  type TaskNotification = TaskCompletedEvent & { id: number };

  const TASK_NOTIFICATION_DURATION_MS = 8000;
  const TOAST_SUMMARY_MAX_CHARS = 80;

  function openTaskInspector(taskIri: string) {
    invoke('widget_blackboard__add_widget', {
      widgetType: 'inspector', entityId: taskIri,
      position: null, size: null, conversationId: appState.activeConversationIri ?? null
    }).catch(e => console.error('[toast] Failed to open inspector:', e));
  }

  let taskNotifications = $state<TaskNotification[]>([]);
  let unlistenTaskCompleted: (() => void) | undefined;
  let taskNotifCounter = 0;

  type AutomationStepEvent = {
    executionIri: string;
    stepIri: string;
    nodeIri: string;
    nodeLabel: string;
    status: 'started' | 'completed' | 'failed';
    error?: string;
  };

  type AutomationExecutionEvent = {
    processIri: string;
    executionIri: string;
    status?: 'completed' | 'failed';
    error?: string;
  };

  type AutomationRun = {
    executionIri: string;
    processIri: string;
    currentStep: string;
    status: 'running' | 'completed' | 'failed';
    error?: string;
  };

  let automationRuns = $state<AutomationRun[]>([]);
  let unlistenAutomationStarted: (() => void) | undefined;
  let unlistenAutomationStep: (() => void) | undefined;
  let unlistenAutomationFinished: (() => void) | undefined;

  type FormulaProgressEvent = {
    jobId: string;
    propertyIri: string;
    propertyLabel: string;
    classIri: string;
    classLabel: string;
    percent: number;
    status: 'running' | 'completed' | 'cancelled';
  };

  type RecalcJob = {
    jobId: string;
    classLabel: string;
    percent: number;
    status: 'running' | 'completed' | 'cancelled';
  };

  let recalcJobs = $state<RecalcJob[]>([]);
  let unlistenRecalc: (() => void) | undefined;

  type ImapSyncEvent = {
    accountIri: string;
    accountLabel: string;
    storedCount?: number;
    error?: string;
  };

  type ImapSyncToast = {
    accountIri: string;
    accountLabel: string;
    status: 'running' | 'done' | 'error';
    message: string;
  };

  type ImapExtractionStartedEvent = { emailIri: string; subject: string };
  type ImapExtractionFinishedEvent = { emailIri: string; subject: string; entitiesCount: number | null; error: string | null };

  type ImapExtractionToast = {
    emailIri: string;
    subject: string;
    status: 'running' | 'done' | 'error';
    message: string;
  };

  let imapSyncs = $state<ImapSyncToast[]>([]);
  let imapExtractions = $state<ImapExtractionToast[]>([]);
  let unlistenImapSyncStarted: (() => void) | undefined;
  let unlistenImapSyncFinished: (() => void) | undefined;
  let unlistenImapExtractionStarted: (() => void) | undefined;
  let unlistenImapExtractionFinished: (() => void) | undefined;

  let retentionRunning = $state(false);
  let unlistenRetentionStarted: (() => void) | undefined;
  let unlistenRetentionComplete: (() => void) | undefined;

  function handleLinkClick(event: MouseEvent) {
    const anchor = (event.target as Element).closest('a');
    if (!anchor) return;
    const href = anchor.getAttribute('href');
    if (!href || (!href.startsWith('http://') && !href.startsWith('https://'))) return;
    event.preventDefault();
    openUrl(href).catch(err => console.error('[App] Failed to open URL:', err));
  }

  onMount(async () => {
    initializeLogging();
    document.addEventListener('click', handleLinkClick, true);

    try {
      await getCurrentWindow().setDecorations(false);
    } catch (e) {
      console.error('[layout] setDecorations failed:', e);
    }

    unlistenRetentionStarted = await listen('retention-started', () => {
      retentionRunning = true;
    });

    unlistenRetentionComplete = await listen('retention-complete', () => {
      retentionRunning = false;
    });

    unlistenAutomationStarted = await listen<AutomationExecutionEvent>('automation-execution-started', (event) => {
      const { executionIri, processIri } = event.payload;
      automationRuns = [...automationRuns, { executionIri, processIri, currentStep: 'Starting…', status: 'running' }];
    });

    unlistenAutomationStep = await listen<AutomationStepEvent>('automation-step-progress', (event) => {
      const { executionIri, nodeLabel, status, error } = event.payload;
      automationRuns = automationRuns.map(r =>
        r.executionIri === executionIri
          ? { ...r, currentStep: nodeLabel, ...(status === 'failed' ? { status: 'failed', error } : {}) }
          : r
      );
    });

    unlistenAutomationFinished = await listen<AutomationExecutionEvent>('automation-execution-finished', (event) => {
      const { executionIri, status, error } = event.payload;
      automationRuns = automationRuns.map(r =>
        r.executionIri === executionIri ? { ...r, status: status ?? 'completed', error } : r
      );
      if (status === 'completed') {
        setTimeout(() => {
          automationRuns = automationRuns.filter(r => r.executionIri !== executionIri);
        }, 2500);
      }
    });

    unlistenTaskCompleted = await listen<TaskCompletedEvent>('task-completed', (event) => {
      const id = ++taskNotifCounter;
      const notif: TaskNotification = { ...event.payload, id };
      taskNotifications = [...taskNotifications, notif];
      setTimeout(() => {
        taskNotifications = taskNotifications.filter(n => n.id !== id);
      }, TASK_NOTIFICATION_DURATION_MS);
    });

    unlistenImapSyncStarted = await listen<ImapSyncEvent>('imap-sync-started', (event) => {
      const { accountIri, accountLabel } = event.payload;
      const entry: ImapSyncToast = {
        accountIri, accountLabel,
        status: 'running',
        message: `Verificando emails de ${accountLabel}`,
      };
      const existing = imapSyncs.findIndex(s => s.accountIri === accountIri);
      if (existing >= 0) imapSyncs[existing] = entry;
      else imapSyncs = [...imapSyncs, entry];
    });

    unlistenImapSyncFinished = await listen<ImapSyncEvent>('imap-sync-finished', (event) => {
      const { accountIri, accountLabel, storedCount, error } = event.payload;
      const toast: ImapSyncToast = error
        ? { accountIri, accountLabel, status: 'error', message: `Falha em ${accountLabel}: ${error}` }
        : (storedCount && storedCount > 0)
          ? { accountIri, accountLabel, status: 'done', message: `${storedCount} ${storedCount === 1 ? 'email novo' : 'emails novos'} em ${accountLabel}` }
          : { accountIri, accountLabel, status: 'done', message: `Sem novos emails em ${accountLabel}` };

      const existing = imapSyncs.findIndex(s => s.accountIri === accountIri);
      if (existing >= 0) imapSyncs[existing] = toast;
      else imapSyncs = [...imapSyncs, toast];

      const dismissAfter = error ? 6000 : (storedCount && storedCount > 0 ? 4000 : 2000);
      setTimeout(() => {
        imapSyncs = imapSyncs.filter(s => !(s.accountIri === accountIri && s.status !== 'running'));
      }, dismissAfter);
    });

    unlistenImapExtractionStarted = await listen<ImapExtractionStartedEvent>('imap-extraction-started', (event) => {
      const { emailIri, subject } = event.payload;
      const entry: ImapExtractionToast = {
        emailIri, subject,
        status: 'running',
        message: `Extraindo dados: ${subject || '(sem assunto)'}`,
      };
      if (!imapExtractions.some(e => e.emailIri === emailIri)) {
        imapExtractions = [...imapExtractions, entry];
      }
    });

    unlistenImapExtractionFinished = await listen<ImapExtractionFinishedEvent>('imap-extraction-finished', (event) => {
      const { emailIri, subject, entitiesCount, error } = event.payload;
      const toast: ImapExtractionToast = error
        ? { emailIri, subject, status: 'error', message: `Erro extraindo "${subject}": ${error}` }
        : (entitiesCount && entitiesCount > 0)
          ? { emailIri, subject, status: 'done', message: `${entitiesCount} ${entitiesCount === 1 ? 'entidade extraída' : 'entidades extraídas'} de "${subject}"` }
          : { emailIri, subject, status: 'done', message: `Sem entidades em "${subject}"` };

      const idx = imapExtractions.findIndex(e => e.emailIri === emailIri);
      if (idx >= 0) imapExtractions[idx] = toast;
      else imapExtractions = [...imapExtractions, toast];

      const dismissAfter = error ? 6000 : (entitiesCount && entitiesCount > 0 ? 4000 : 1500);
      setTimeout(() => {
        imapExtractions = imapExtractions.filter(e => !(e.emailIri === emailIri && e.status !== 'running'));
      }, dismissAfter);
    });

    unlistenRecalc = await listen<FormulaProgressEvent>('formula-recalc-progress', (event) => {
      const { jobId, classLabel, percent, status } = event.payload;

      const existing = recalcJobs.find(j => j.jobId === jobId);
      if (existing) {
        existing.percent = percent;
        existing.status = status;
        recalcJobs = recalcJobs;
      } else {
        recalcJobs = [...recalcJobs, { jobId, classLabel, percent, status }];
      }

      if (status === 'completed' || status === 'cancelled') {
        setTimeout(() => {
          recalcJobs = recalcJobs.filter(j => j.jobId !== jobId);
        }, 2000);
      }
    });
  });

  onDestroy(() => {
    document.removeEventListener('click', handleLinkClick, true);
    unlistenTaskCompleted?.();
    unlistenRecalc?.();
    unlistenRetentionStarted?.();
    unlistenRetentionComplete?.();
    unlistenAutomationStarted?.();
    unlistenAutomationStep?.();
    unlistenAutomationFinished?.();
    unlistenImapSyncStarted?.();
    unlistenImapSyncFinished?.();
    unlistenImapExtractionStarted?.();
    unlistenImapExtractionFinished?.();
  });
</script>

<div class="app-shell">
  <AppHeader />
  <main class="app-main">
    {@render children()}
  </main>
</div>

{#if $deleteConfirm}
  {@const dialog = $deleteConfirm}
  {@const hasCascade = dialog.cascade_items.length > 0}
  {@const hasImpact = hasCascade || dialog.backlink_count > 0}
  <div class="global-overlay" role="dialog" aria-modal="true">
    <div class="global-dialog">
      <span class="material-symbols-outlined dialog-icon">delete_forever</span>
      <p class="dialog-title">Excluir "{dialog.entityLabel}"?</p>
      {#if hasImpact}
        <div class="dialog-impact">
          {#if hasCascade}
            <p class="impact-heading">Também será excluído:</p>
            <ul class="impact-list">
              {#each dialog.cascade_items as item}
                <li>
                  <span class="impact-item-label">{item.label}</span>
                  {#if item.type_label}<span class="impact-item-type">{item.type_label}</span>{/if}
                </li>
              {/each}
            </ul>
          {/if}
          {#if dialog.backlink_count > 0}
            <p class="backlink-note">
              {dialog.backlink_count} {dialog.backlink_count === 1 ? 'referência' : 'referências'} a esta entidade {dialog.backlink_count === 1 ? 'será removida' : 'serão removidas'} de outras entidades (que não serão excluídas).
            </p>
          {/if}
          <p class="impact-warning">Esta ação não pode ser desfeita pela interface.</p>
        </div>
      {:else}
        <p class="dialog-warning">Esta ação não pode ser desfeita pela interface.</p>
      {/if}
      <div class="dialog-actions">
        <button class="dialog-cancel-btn" onclick={dialog.onCancel}>Cancelar</button>
        <button class="dialog-confirm-btn" onclick={dialog.onConfirm}>Excluir</button>
      </div>
    </div>
  </div>
{/if}

<GlobalModal />

{#if automationRuns.length > 0 || retentionRunning || recalcJobs.length > 0 || taskNotifications.length > 0 || imapSyncs.length > 0 || imapExtractions.length > 0}
  <div class="toast-stack">
    {#each automationRuns as run (run.executionIri)}
      <div
        class="automation-toast"
        class:failed={run.status === 'failed'}
        class:done={run.status === 'completed'}
        class:running={run.status === 'running'}
        role="button"
        tabindex="0"
        onclick={() => invoke('widget_blackboard__add_widget', { widgetType: 'workflow_execution', entityId: run.executionIri, position: null, size: null, conversationId: appState.activeConversationIri }).catch(e => console.error('[toast] Failed to open widget:', e))}
        onkeydown={(e) => e.key === 'Enter' && invoke('widget_blackboard__add_widget', { widgetType: 'workflow_execution', entityId: run.executionIri, position: null, size: null, conversationId: appState.activeConversationIri }).catch(e => console.error('[toast] Failed to open widget:', e))}
      >
        {#if run.status === 'running'}
          <div class="toast-thinking"><ThinkingDots /></div>
        {:else}
          <span class="material-symbols-outlined toast-icon">
            {run.status === 'completed' ? 'check_circle' : 'error'}
          </span>
        {/if}
        <div class="toast-body">
          {#if run.status === 'failed'}
            <span class="toast-label">Automation failed: <strong>{run.currentStep}</strong></span>
            {#if run.error}<span class="toast-error">{run.error}</span>{/if}
          {:else if run.status === 'completed'}
            <span class="toast-label">Automation completed</span>
          {:else}
            <span class="toast-label">Running: <strong>{run.currentStep}</strong></span>
          {/if}
        </div>
        {#if run.status !== 'running'}
          <button class="toast-close" onclick={(e) => { e.stopPropagation(); automationRuns = automationRuns.filter(r => r.executionIri !== run.executionIri); }}>
            <span class="material-symbols-outlined">close</span>
          </button>
        {/if}
      </div>
    {/each}

    {#if retentionRunning}
      <div class="system-toast">
        <span class="material-symbols-outlined" style="font-size:15px;opacity:0.7">cleaning_services</span>
        <span>Cleaning up old data…</span>
      </div>
    {/if}

    {#each taskNotifications as notif (notif.id)}
      <div class="automation-toast done task-toast">
        <span class="material-symbols-outlined toast-icon">task_alt</span>
        <div class="toast-body">
          <span class="toast-label"><strong>{notif.label}</strong> concluída</span>
          {#if notif.result_summary}
            <span class="toast-detail">{notif.result_summary.slice(0, TOAST_SUMMARY_MAX_CHARS)}{notif.result_summary.length > TOAST_SUMMARY_MAX_CHARS ? '…' : ''}</span>
          {/if}
          <button
            class="toast-action-btn"
            onclick={() => openTaskInspector(notif.task_iri)}
          >
            Ver tarefa
          </button>
        </div>
        <button class="toast-close" onclick={() => { taskNotifications = taskNotifications.filter(n => n.id !== notif.id); }}>
          <span class="material-symbols-outlined">close</span>
        </button>
      </div>
    {/each}

    {#each imapSyncs as sync (sync.accountIri)}
      <div class="automation-toast" class:failed={sync.status === 'error'} class:done={sync.status === 'done'} class:running={sync.status === 'running'}>
        {#if sync.status === 'running'}
          <div class="toast-thinking"><ThinkingDots /></div>
        {:else}
          <span class="material-symbols-outlined toast-icon">
            {sync.status === 'done' ? 'mark_email_read' : 'error'}
          </span>
        {/if}
        <div class="toast-body">
          <span class="toast-label">{sync.message}</span>
        </div>
        {#if sync.status !== 'running'}
          <button class="toast-close" onclick={() => { imapSyncs = imapSyncs.filter(s => s.accountIri !== sync.accountIri); }}>
            <span class="material-symbols-outlined">close</span>
          </button>
        {/if}
      </div>
    {/each}

    {#each imapExtractions as ext (ext.emailIri)}
      <div class="automation-toast" class:failed={ext.status === 'error'} class:done={ext.status === 'done'} class:running={ext.status === 'running'}>
        {#if ext.status === 'running'}
          <div class="toast-thinking"><ThinkingDots /></div>
        {:else}
          <span class="material-symbols-outlined toast-icon">
            {ext.status === 'done' ? 'psychology' : 'error'}
          </span>
        {/if}
        <div class="toast-body">
          <span class="toast-label">{ext.message}</span>
        </div>
        {#if ext.status !== 'running'}
          <button class="toast-close" onclick={() => { imapExtractions = imapExtractions.filter(e => e.emailIri !== ext.emailIri); }}>
            <span class="material-symbols-outlined">close</span>
          </button>
        {/if}
      </div>
    {/each}

    {#each recalcJobs as job (job.jobId)}
      <div class="system-toast" class:done={job.status !== 'running'}>
        <span class="recalc-label">Recalculating <strong>{job.classLabel}</strong></span>
        <span class="recalc-pct">{job.percent}%</span>
      </div>
    {/each}
  </div>
{/if}

<style>
  :global(body) {
    margin: 0;
    padding: 0;
    overflow: hidden;
    background: var(--color-surface-0);
  }

  .app-shell {
    width: 100vw;
    height: 100vh;
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }

  .app-main {
    flex: 1;
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    padding: 5px;
  }

  .global-overlay {
    position: fixed;
    inset: 0;
    background: color-mix(in srgb, var(--color-black) 70%, transparent);
    backdrop-filter: blur(6px);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 99999;
  }

  .global-dialog {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 12px;
    padding: 28px 24px;
    background: var(--color-surface-3);
    border-radius: var(--radius);
    max-width: 340px;
    width: 90vw;
    text-align: center;
    box-shadow: 0 16px 48px rgba(0, 0, 0, 0.6);
  }

  .dialog-icon {
    font-size: 36px;
    color: #ef4444;
  }

  .dialog-title {
    font-family: var(--font-body);
    font-size: 14px;
    font-weight: 600;
    color: var(--color-neutral-active);
    margin: 0;
    word-break: break-word;
  }

  .dialog-warning {
    font-family: var(--font-body);
    font-size: 12px;
    color: var(--color-neutral);
    margin: 0;
    opacity: 0.7;
  }

  .dialog-impact {
    width: 100%;
    background: color-mix(in srgb, #ef4444 8%, transparent);
    padding: 10px 14px;
    text-align: left;
  }

  .impact-heading {
    font-family: var(--font-body);
    font-size: 11px;
    font-weight: 700;
    color: color-mix(in srgb, #ef4444 80%, white);
    margin: 0 0 6px 0;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }

  .impact-list {
    margin: 0 0 8px 0;
    padding-left: 16px;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }

  .impact-list li {
    font-family: var(--font-body);
    font-size: 12px;
    color: var(--color-neutral-active);
    line-height: 1.4;
    display: flex;
    align-items: baseline;
    gap: 6px;
  }

  .impact-item-label {
    font-weight: 500;
  }

  .impact-item-type {
    font-size: 10px;
    color: var(--color-neutral);
    opacity: 0.8;
    font-style: italic;
  }

  .backlink-note {
    font-family: var(--font-body);
    font-size: 11px;
    color: var(--color-neutral);
    margin: 0 0 6px 0;
    line-height: 1.4;
    opacity: 0.85;
  }

  .impact-warning {
    font-family: var(--font-body);
    font-size: 11px;
    color: var(--color-neutral);
    margin: 0;
    opacity: 0.7;
  }

  .dialog-actions {
    display: flex;
    gap: 8px;
    margin-top: 4px;
  }

  .dialog-cancel-btn {
    padding: 7px 18px;
    background: color-mix(in srgb, var(--color-white) 8%, transparent);
    color: var(--color-neutral-active);
    font-family: var(--font-body);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }

  .dialog-confirm-btn {
    padding: 7px 18px;
    background: color-mix(in srgb, #ef4444 20%, transparent);
    color: #ef4444;
    font-family: var(--font-body);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }

  .dialog-confirm-btn:active {
    background: color-mix(in srgb, #ef4444 35%, transparent);
    color: var(--color-neutral-active);
  }

  .toast-stack {
    position: fixed;
    bottom: 16px;
    left: 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: 9999;
  }

  .automation-toast {
    background: var(--color-surface-2);
    border-radius: var(--radius);
    color: var(--color-neutral);
    padding: 10px 14px;
    display: flex;
    align-items: flex-start;
    gap: 10px;
    min-width: 220px;
    max-width: 340px;
    pointer-events: auto;
    cursor: pointer;
  }

  .automation-toast.running {
    background: color-mix(in srgb, var(--color-transition) 10%, var(--color-surface-2));
    animation: toast-pulse 1.5s ease-in-out infinite;
  }

  @keyframes toast-pulse {
    0%, 100% { background: color-mix(in srgb, var(--color-transition) 10%, var(--color-surface-2)); }
    50% { background: color-mix(in srgb, var(--color-transition) 18%, var(--color-surface-2)); }
  }

  .toast-icon {
    font-size: 18px;
    flex-shrink: 0;
    margin-top: 1px;
  }

  .toast-thinking {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    margin-top: 3px;
  }

  .automation-toast.failed .toast-icon { color: var(--color-danger); }
  .automation-toast.done .toast-icon { color: var(--color-success); }

  .toast-body {
    display: flex;
    flex-direction: column;
    gap: 3px;
    font-size: 12px;
    min-width: 0;
    flex: 1;
  }

  .toast-label {
    line-height: 1.4;
    word-break: break-word;
  }

  .toast-detail {
    font-size: 11px;
    color: var(--color-neutral);
    opacity: 0.8;
    word-break: break-word;
    line-height: 1.4;
  }

  .toast-action-btn {
    margin-top: 4px;
    background: none;
    color: var(--color-success);
    font-family: var(--font-body);
    font-size: 11px;
    font-weight: 600;
    padding: 3px 8px;
    cursor: pointer;
    align-self: flex-start;
  }

  .toast-error {
    font-size: 11px;
    color: color-mix(in srgb, var(--color-danger) 80%, white);
    word-break: break-word;
  }

  .toast-close {
    background: none;
    border: none;
    padding: 2px;
    cursor: pointer;
    color: var(--color-neutral-disabled);
    flex-shrink: 0;
    display: flex;
    align-items: center;
    align-self: flex-start;
  }

  .toast-close .material-symbols-outlined {
    font-size: 16px;
  }

  .system-toast {
    background: var(--color-surface-2);
    border-radius: var(--radius);
    color: var(--color-neutral);
    padding: 8px 14px;
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    pointer-events: none;
    animation: toast-pulse 1.5s ease-in-out infinite;
  }

  .system-toast.done {
    animation: none;
    opacity: 0.6;
  }

  .recalc-pct {
    font-weight: 600;
    min-width: 36px;
    text-align: right;
  }
</style>
