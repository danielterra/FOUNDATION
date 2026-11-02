<script lang="ts">
  import '../app.css';
  import '$lib/base.css';
  import '$lib/fonts.css';
  import '$lib/markdown.css';
  import { initializeLogging } from '$lib/logging.js';
  import { onMount, onDestroy } from 'svelte';
  const { children } = $props();
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { Button } from '$lib/components/ui/button';
  import GlobalModal from '$lib/components/GlobalModal.svelte';
  import ThinkingDots from '$lib/components/ThinkingDots.svelte';
  import AppHeader from '$lib/components/AppHeader.svelte';
  import { appState } from '$lib/appState.svelte';
  import { openEntity } from '$lib/blackboard';
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
    openEntity(taskIri, appState.activeConversationIri).catch(e => console.error('[toast] Failed to open entity:', e));
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
    if (!href || href.startsWith('#')) return;
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
          <Button variant="ghost" size="icon-sm" class="toast-close" onclick={(e) => { e.stopPropagation(); automationRuns = automationRuns.filter(r => r.executionIri !== run.executionIri); }}>
            <span class="material-symbols-outlined">close</span>
          </Button>
        {/if}
      </div>
    {/each}

    {#if retentionRunning}
      <div class="system-toast">
        <span class="material-symbols-outlined toast-icon">cleaning_services</span>
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
          <Button variant="ghost" class="toast-action-btn" onclick={() => openTaskInspector(notif.task_iri)}>
            Ver tarefa
          </Button>
        </div>
        <Button variant="ghost" size="icon-sm" class="toast-close" onclick={() => { taskNotifications = taskNotifications.filter(n => n.id !== notif.id); }}>
          <span class="material-symbols-outlined">close</span>
        </Button>
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
          <Button variant="ghost" size="icon-sm" class="toast-close" onclick={() => { imapSyncs = imapSyncs.filter(s => s.accountIri !== sync.accountIri); }}>
            <span class="material-symbols-outlined">close</span>
          </Button>
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
          <Button variant="ghost" size="icon-sm" class="toast-close" onclick={() => { imapExtractions = imapExtractions.filter(e => e.emailIri !== ext.emailIri); }}>
            <span class="material-symbols-outlined">close</span>
          </Button>
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

  .toast-stack {
    position: fixed;
    top: calc(var(--app-header-height) + var(--space-4));
    right: var(--space-4);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    width: min(var(--size-toast-max-width), calc(100vw - var(--space-6)));
    max-height: var(--size-toast-stack-max-height);
    overflow-y: auto;
    padding: var(--space-1);
    z-index: var(--z-toast);
  }

  .automation-toast {
    position: relative;
    background: var(--color-toast-surface);
    border-top: var(--size-toast-edge) solid var(--color-toast-edge);
    color: var(--text-body);
    padding: var(--space-3) var(--space-4);
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: var(--size-toast-min-width);
    max-width: var(--size-toast-max-width);
    pointer-events: auto;
    cursor: pointer;
    backdrop-filter: blur(var(--blur-widget-surface));
    box-shadow: 0 var(--size-toast-shadow-y) var(--size-toast-shadow-blur) color-mix(in srgb, var(--color-black) 55%, transparent);
    transition: background-color 160ms ease, border-color 160ms ease;
  }

  .automation-toast:hover {
    background: var(--color-surface-3);
  }

  .automation-toast:focus-visible {
    outline: var(--size-hud-grid-dot) solid var(--color-selection);
    outline-offset: var(--space-1);
  }

  .automation-toast.running {
    border-top-color: var(--color-toast-running);
  }

  .automation-toast.failed {
    border-top-color: var(--color-toast-error);
  }

  .automation-toast.done {
    border-top-color: var(--color-toast-complete);
  }

  .toast-icon {
    font-size: var(--type-icon-medium);
    flex-shrink: 0;
    color: var(--color-interactive);
  }

  .toast-thinking {
    display: flex;
    align-items: center;
    flex-shrink: 0;
    margin-top: 0;
  }

  .automation-toast.failed .toast-icon { color: var(--color-toast-error); }
  .automation-toast.done .toast-icon { color: var(--color-toast-complete); }

  .toast-body {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    font-size: var(--type-body);
    min-width: 0;
    flex: 1;
  }

  .toast-label {
    color: var(--text-primary);
    line-height: 1.45;
    word-break: break-word;
  }

  .toast-detail {
    font-size: var(--type-body-compact);
    color: var(--color-toast-detail);
    word-break: break-word;
    line-height: 1.45;
  }

  :global([data-slot="button"].toast-action-btn) {
    margin-top: var(--space-1);
    color: var(--color-interactive);
    font-family: var(--font-body);
    font-size: var(--type-label);
    padding: var(--space-1) var(--space-2);
    height: auto;
    align-self: flex-start;
  }

  :global([data-slot="button"].toast-action-btn:hover) {
    background: transparent;
    color: var(--color-interactive-hover);
  }

  .toast-error {
    font-size: var(--type-body-compact);
    color: var(--text-danger);
    word-break: break-word;
  }

  :global([data-slot="button"].toast-close) {
    color: var(--text-secondary);
    flex-shrink: 0;
    align-self: flex-start;
  }

  :global([data-slot="button"].toast-close:hover) {
    background: transparent;
    color: var(--text-primary);
  }

  :global([data-slot="button"].toast-close .material-symbols-outlined) {
    font-size: var(--type-icon-small);
  }

  .system-toast {
    background: var(--color-toast-surface);
    border-top: var(--size-toast-edge) solid var(--color-toast-edge);
    color: var(--text-body);
    padding: var(--space-2) var(--space-4);
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--type-body);
    pointer-events: none;
    backdrop-filter: blur(var(--blur-widget-surface));
    box-shadow: 0 var(--size-toast-shadow-y) var(--size-toast-shadow-blur) color-mix(in srgb, var(--color-black) 55%, transparent);
  }

  .system-toast.done {
    border-top-color: var(--color-toast-complete);
  }

  .system-toast .recalc-label {
    color: var(--text-primary);
  }

  @media (max-width: 480px) {
    .toast-stack {
      right: var(--space-2);
      width: calc(100vw - var(--space-4));
    }

    .automation-toast {
      min-width: 0;
      max-width: none;
    }
  }

  .recalc-pct {
    font-size: var(--type-caption);
    min-width: var(--size-toast-recalc-min-width);
    text-align: right;
  }
</style>
