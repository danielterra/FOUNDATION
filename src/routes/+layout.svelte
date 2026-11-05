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
  const AUTOMATION_SUCCESS_TOAST_DURATION_MS = 8000;
  const TOAST_SUMMARY_MAX_CHARS = 80;
  const automationSuccessTimers = new Map<string, ReturnType<typeof setTimeout>>();

  function openTaskInspector(taskIri: string) {
    openEntity(taskIri, appState.activeConversationIri).catch(e => console.error('[toast] Failed to open entity:', e));
  }

  function openAutomationUpdate(event: MouseEvent) {
    const button = event.currentTarget as HTMLElement;
    const stepIri = button.dataset.stepIri;
    if (stepIri) {
      openEntity(stepIri, appState.activeConversationIri)
        .catch(e => console.error('[toast] Failed to open failed step:', e));
      return;
    }

    const executionIri = button.dataset.executionIri;
    if (executionIri) {
      invoke('widget_blackboard__add_widget', {
        widgetType: 'workflow_execution',
        entityId: executionIri,
        position: null,
        size: null,
        conversationId: appState.activeConversationIri,
      }).catch(e => console.error('[toast] Failed to open execution widget:', e));
      return;
    }

    const sourceEntityIri = button.dataset.sourceEntityIri;
    if (sourceEntityIri) {
      openEntity(sourceEntityIri, appState.activeConversationIri)
        .catch(e => console.error('[toast] Failed to open source entity:', e));
    }
  }

  function dismissAutomationUpdate(event: MouseEvent) {
    const sourceUpdateId = (event.currentTarget as HTMLElement).dataset.sourceUpdateId;
    if (!sourceUpdateId) return;
    const timer = automationSuccessTimers.get(sourceUpdateId);
    if (timer) clearTimeout(timer);
    automationSuccessTimers.delete(sourceUpdateId);
    automationRuns = automationRuns.filter(run => run.sourceUpdateId !== sourceUpdateId);
    recalcJobs = recalcJobs.filter(job => job.sourceUpdateId !== sourceUpdateId);
  }

  function materializationDescription(run: AutomationRun) {
    if (run.materializationStatus === 'failed') return run.reason || 'Falha ao atualizar os dados necessários';
    if (run.materializationStatus === 'cancelled') return run.reason || 'Atualização dos dados cancelada';
    if (run.materializationStatus === 'completed') return 'Dados atualizados';
    if (run.materializationStatus === 'waiting') return 'Aguardando confirmação dos dados necessários';
    if (run.phase === 'query-properties') return 'Atualizando propriedades relacionadas';
    if (run.phase === 'calculated-properties') return 'Atualizando propriedades calculadas';
    return 'Aguardando etapa';
  }

  function hasDeterminateProgress(run: AutomationRun) {
    return run.completedCount !== undefined
      && run.totalCount !== undefined
      && run.totalCount > 0
      && run.completedCount >= 0
      && run.completedCount <= run.totalCount;
  }

  function requireSourceUpdateId(sourceUpdateId: string | undefined, eventName: string) {
    if (sourceUpdateId) return sourceUpdateId;
    console.error(`[toast] ${eventName} is missing sourceUpdateId`);
    return null;
  }

  function handleAutomationStarted(event: { payload: AutomationExecutionEvent }) {
    const { executionIri, processIri, sourceEntityIri } = event.payload;
    const sourceUpdateId = requireSourceUpdateId(event.payload.sourceUpdateId, 'automation-execution-started');
    if (!sourceUpdateId) return;
    if (automationRuns.some(run => run.executionIri === executionIri)) return;
    const timer = automationSuccessTimers.get(sourceUpdateId);
    if (timer) clearTimeout(timer);
    automationSuccessTimers.delete(sourceUpdateId);
    automationRuns = [...automationRuns, {
      sourceUpdateId,
      sourceEntityIri,
      executionIri,
      processIri,
      currentStep: 'Preparando automação',
      status: 'running',
    }];
  }

  function handleAutomationStep(event: { payload: AutomationStepEvent }) {
    const payload = event.payload;
    const sourceUpdateId = requireSourceUpdateId(payload.sourceUpdateId, 'automation-step-progress');
    if (!sourceUpdateId) return;
    const run = automationRuns.find(item => item.executionIri === payload.executionIri);
    if (run && run.sourceUpdateId !== sourceUpdateId) {
      console.error(`[toast] Execution ${payload.executionIri} changed sourceUpdateId`);
      return;
    }
    automationRuns = automationRuns.map(run => {
      if (run.executionIri !== payload.executionIri) return run;

      if (payload.phase) {
        return {
          ...run,
          currentStepIri: payload.stepIri,
          currentStep: payload.nodeLabel ?? run.currentStep,
          phase: payload.phase,
          materializationStatus: payload.status as AutomationRun['materializationStatus'],
          completedCount: payload.completedCount,
          totalCount: payload.totalCount,
          reason: payload.reason ?? payload.error,
          iterationProgress: payload.iterationIndex !== undefined && payload.iterationTotal !== undefined
            ? { current: payload.iterationIndex + 1, total: payload.iterationTotal }
            : run.iterationProgress,
        };
      }

      return {
        ...run,
        currentStepIri: payload.stepIri,
        currentStep: payload.nodeLabel ?? run.currentStep,
        phase: undefined,
        materializationStatus: undefined,
        completedCount: undefined,
        totalCount: undefined,
        reason: undefined,
        iterationProgress: payload.iterationIndex !== undefined && payload.iterationTotal !== undefined
          ? { current: payload.iterationIndex + 1, total: payload.iterationTotal }
          : payload.status === 'started' || payload.status === 'completed' || payload.status === 'failed'
            ? undefined
            : run.iterationProgress,
        ...(payload.status === 'failed' && payload.error ? { error: payload.error } : {}),
      };
    });
  }

  function handleAutomationFinished(event: { payload: AutomationFinishedEvent }) {
    const { executionIri, status, error, sourceUpdateId } = event.payload;
    const run = automationRuns.find(item => item.executionIri === executionIri);
    if (!run || run.status !== 'running') return;

    const updateId = requireSourceUpdateId(sourceUpdateId, 'automation-execution-finished');
    if (!updateId || updateId !== run.sourceUpdateId) {
      console.error(`[toast] Execution ${executionIri} finished with a mismatched sourceUpdateId`);
      return;
    }

    automationRuns = automationRuns.map(item => item.executionIri === executionIri
      ? { ...item, status, error: error ?? item.error }
      : item);

    const hasPendingRuns = automationRuns.some(item => item.sourceUpdateId === updateId && item.status === 'running');
    const hasPendingJobs = recalcJobs.some(item => item.sourceUpdateId === updateId && item.status === 'running');
    if (!hasPendingRuns && !hasPendingJobs && status === 'completed') scheduleAutomationUpdateDismiss(updateId);
  }

  let taskNotifications = $state<TaskNotification[]>([]);
  let unlistenTaskCompleted: (() => void) | undefined;
  let taskNotifCounter = 0;

  type AutomationStepEvent = {
    executionIri: string;
    sourceUpdateId: string;
    sourceEntityIri?: string;
    sourceTx?: number;
    eventKey?: string;
    stepIri: string;
    nodeIri: string;
    nodeLabel?: string;
    status: 'started' | 'completed' | 'failed' | 'waiting' | 'running' | 'cancelled';
    phase?: 'query-properties' | 'calculated-properties';
    entityIri?: string;
    propertyIri?: string;
    completedCount?: number;
    totalCount?: number;
    iterationIndex?: number;
    iterationTotal?: number;
    reason?: string;
    error?: string;
  };

  type AutomationExecutionEvent = {
    processIri: string;
    executionIri: string;
    sourceUpdateId: string;
    sourceEntityIri?: string;
    sourceTx?: number;
  };

  type AutomationFinishedEvent = {
    executionIri: string;
    sourceUpdateId: string;
    sourceEntityIri?: string;
    sourceTx?: number;
    status: 'completed' | 'failed' | 'cancelled';
    error?: string;
  };

  type AutomationRun = {
    sourceUpdateId: string;
    sourceEntityIri?: string;
    executionIri: string;
    processIri: string;
    currentStep: string;
    currentStepIri?: string;
    status: 'running' | 'completed' | 'failed' | 'cancelled';
    error?: string;
    phase?: 'query-properties' | 'calculated-properties';
    materializationStatus?: 'waiting' | 'running' | 'completed' | 'failed' | 'cancelled';
    completedCount?: number;
    totalCount?: number;
    iterationProgress?: { current: number; total: number };
    reason?: string;
  };

  let automationRuns = $state<AutomationRun[]>([]);
  let unlistenAutomationStarted: (() => void) | undefined;
  let unlistenAutomationStep: (() => void) | undefined;
  let unlistenAutomationFinished: (() => void) | undefined;

  type FormulaProgressEvent = {
    jobId: string;
    sourceUpdateId: string;
    sourceEntityIri?: string;
    sourceTx?: number;
    propertyIri: string;
    propertyLabel: string;
    classIri: string;
    classLabel: string;
    percent: number;
    status: 'running' | 'completed' | 'failed' | 'cancelled';
    error?: string;
  };

  type RecalcJob = {
    jobId: string;
    sourceUpdateId: string;
    sourceEntityIri?: string;
    classLabel: string;
    percent: number;
    status: 'running' | 'completed' | 'failed' | 'cancelled';
    error?: string;
  };

  type AutomationUpdate = {
    sourceUpdateId: string;
    sourceEntityIri?: string;
    runs: AutomationRun[];
    jobs: RecalcJob[];
  };

  function groupAutomationUpdates(): AutomationUpdate[] {
    const groups = new Map<string, AutomationUpdate>();
    for (const run of automationRuns) {
      const group = groups.get(run.sourceUpdateId) ?? {
        sourceUpdateId: run.sourceUpdateId,
        sourceEntityIri: run.sourceEntityIri,
        runs: [],
        jobs: [],
      };
      group.runs.push(run);
      groups.set(run.sourceUpdateId, group);
    }
    for (const job of recalcJobs) {
      const group = groups.get(job.sourceUpdateId) ?? {
        sourceUpdateId: job.sourceUpdateId,
        sourceEntityIri: job.sourceEntityIri,
        runs: [],
        jobs: [],
      };
      group.jobs.push(job);
      groups.set(job.sourceUpdateId, group);
    }
    return [...groups.values()];
  }

  let automationUpdates = $derived.by(groupAutomationUpdates);

  function automationUpdateStatus(update: AutomationUpdate): AutomationRun['status'] {
    if (update.runs.some(run => run.status === 'failed') || update.jobs.some(job => job.status === 'failed')) return 'failed';
    if (update.runs.some(run => run.status === 'cancelled') || update.jobs.some(job => job.status === 'cancelled')) return 'cancelled';
    if (update.runs.some(run => run.status === 'running') || update.jobs.some(job => job.status === 'running')) return 'running';
    return 'completed';
  }

  function updateCurrentStep(update: AutomationUpdate) {
    const activeRuns = update.runs.filter(run => run.status === 'running');
    if (activeRuns.length === 1) return activeRuns[0].currentStep;
    if (activeRuns.length > 1) return `${activeRuns.length} execuções relacionadas em andamento`;
    if (update.jobs.some(job => job.status === 'running')) return 'Atualizando propriedades derivadas';
    return 'Automação finalizada';
  }

  function updateError(update: AutomationUpdate) {
    return update.runs.find(run => run.status === 'failed')?.error
      ?? update.runs.find(run => run.status === 'failed')?.reason
      ?? update.jobs.find(job => job.status === 'failed')?.error;
  }

  function updateExecutionIri(update: AutomationUpdate) {
    return update.runs.find(run => run.status === 'failed')?.executionIri
      ?? update.runs.find(run => run.status === 'running')?.executionIri
      ?? update.runs[0]?.executionIri;
  }

  function failedStepIri(update: AutomationUpdate) {
    return update.runs.find(run => run.status === 'failed')?.currentStepIri;
  }

  function scheduleAutomationUpdateDismiss(sourceUpdateId: string) {
    if (automationSuccessTimers.has(sourceUpdateId)) return;
    const timer = setTimeout(() => {
      automationSuccessTimers.delete(sourceUpdateId);
      automationRuns = automationRuns.filter(item => item.sourceUpdateId !== sourceUpdateId);
      recalcJobs = recalcJobs.filter(item => item.sourceUpdateId !== sourceUpdateId);
    }, AUTOMATION_SUCCESS_TOAST_DURATION_MS);
    automationSuccessTimers.set(sourceUpdateId, timer);
  }

  function settleFormulaUpdate(sourceUpdateId: string) {
    const hasPendingRuns = automationRuns.some(item => item.sourceUpdateId === sourceUpdateId && item.status === 'running');
    const hasPendingJobs = recalcJobs.some(item => item.sourceUpdateId === sourceUpdateId && item.status === 'running');
    const hasFailure = automationRuns.some(item => item.sourceUpdateId === sourceUpdateId && item.status !== 'completed')
      || recalcJobs.some(item => item.sourceUpdateId === sourceUpdateId && (item.status === 'failed' || item.status === 'cancelled'));
    if (!hasPendingRuns && !hasPendingJobs && !hasFailure) scheduleAutomationUpdateDismiss(sourceUpdateId);
  }

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

    unlistenAutomationStarted = await listen<AutomationExecutionEvent>('automation-execution-started', handleAutomationStarted);
    unlistenAutomationStep = await listen<AutomationStepEvent>('automation-step-progress', handleAutomationStep);
    unlistenAutomationFinished = await listen<AutomationFinishedEvent>('automation-execution-finished', handleAutomationFinished);

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
      const { jobId, classLabel, percent, status, sourceEntityIri, error } = event.payload;
      const sourceUpdateId = requireSourceUpdateId(event.payload.sourceUpdateId, 'formula-recalc-progress');
      if (!sourceUpdateId) return;
      const timer = automationSuccessTimers.get(sourceUpdateId);
      if (timer) clearTimeout(timer);
      automationSuccessTimers.delete(sourceUpdateId);

      const existing = recalcJobs.find(j => j.jobId === jobId && j.sourceUpdateId === sourceUpdateId);
      if (existing) {
        existing.percent = percent;
        existing.status = status;
        existing.error = error;
        recalcJobs = recalcJobs;
      } else {
        recalcJobs = [...recalcJobs, { jobId, sourceUpdateId, sourceEntityIri, classLabel, percent, status, error }];
      }

      if (status !== 'running') settleFormulaUpdate(sourceUpdateId);
    });
  });

  onDestroy(() => {
    for (const timer of automationSuccessTimers.values()) clearTimeout(timer);
    automationSuccessTimers.clear();
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

{#if automationUpdates.length > 0 || retentionRunning || taskNotifications.length > 0 || imapSyncs.length > 0 || imapExtractions.length > 0}
  <div class="toast-stack">
    {#each automationUpdates as update (update.sourceUpdateId)}
      {@const status = automationUpdateStatus(update)}
      {@const executionIri = updateExecutionIri(update)}
      {@const error = updateError(update)}
      {@const activeRun = update.runs.find(run => run.status === 'running')}
      <section
        class="automation-toast"
        class:failed={status === 'failed'}
        class:cancelled={status === 'cancelled'}
        class:done={status === 'completed'}
        class:running={status === 'running'}
        aria-live={status === 'failed' || status === 'cancelled' ? 'assertive' : 'polite'}
        aria-atomic="true"
      >
        {#if status === 'running'}
          <div class="toast-thinking"><ThinkingDots /></div>
        {:else}
          <span class="material-symbols-outlined toast-icon">
            {status === 'completed' ? 'check_circle' : status === 'cancelled' ? 'cancel' : 'error'}
          </span>
        {/if}
        <div class="toast-body">
          {#if status === 'failed'}
            <span class="toast-label">Falha ao atualizar dados necessários</span>
            {#if error}<span class="toast-error">{error}</span>{/if}
          {:else if status === 'cancelled'}
            <span class="toast-label">Atualização cancelada; etapas dependentes não foram iniciadas</span>
            {#if error}<span class="toast-detail">{error}</span>{/if}
          {:else if status === 'completed'}
            <span class="toast-label">Atualização concluída</span>
          {:else}
            <span class="toast-label">{updateCurrentStep(update)}</span>
            {#if activeRun?.iterationProgress}
              <span class="toast-detail">Item {activeRun.iterationProgress.current} de {activeRun.iterationProgress.total}</span>
            {/if}
            {#if activeRun?.phase}
              <span class="toast-detail">{materializationDescription(activeRun)}</span>
              {#if activeRun.materializationStatus === 'waiting' || activeRun.materializationStatus === 'running'}
                {#if hasDeterminateProgress(activeRun)}
                  <progress class="automation-progress" max={activeRun.totalCount} value={activeRun.completedCount} aria-label="Progresso da atualização dos dados"></progress>
                  <span class="toast-detail">{activeRun.completedCount} de {activeRun.totalCount}</span>
                {:else}
                  <progress class="automation-progress" aria-label={activeRun.materializationStatus === 'waiting' ? 'Aguardando confirmação dos dados necessários' : 'Atualizando os dados necessários'}></progress>
                {/if}
              {/if}
            {/if}
          {/if}
          {#if update.jobs.length > 0}
            <div class="toast-progress-list" aria-label="Progresso das propriedades derivadas">
              {#each update.jobs as job (`${job.jobId}:${job.sourceUpdateId}`)}
                <div class="toast-progress-item">
                  <span class="toast-detail">{job.classLabel}</span>
                  {#if job.status === 'running'}
                    <progress class="automation-progress" max="100" value={job.percent} aria-label={`Atualizando ${job.classLabel}`}></progress>
                    <span class="toast-detail">{job.percent}%</span>
                  {:else if job.status === 'completed'}
                    <span class="toast-detail">Concluído</span>
                  {:else if job.status === 'failed'}
                    <span class="toast-error">{job.error ?? 'Falha ao recalcular'}</span>
                  {:else}
                    <span class="toast-detail">Cancelado</span>
                  {/if}
                </div>
              {/each}
            </div>
          {/if}
        </div>
        <Button variant="ghost" class="automation-action-btn" data-step-iri={status === 'failed' ? failedStepIri(update) : undefined} data-execution-iri={executionIri} data-source-entity-iri={update.sourceEntityIri} onclick={openAutomationUpdate}>
          {status === 'failed' ? 'Inspecionar' : 'Abrir execução'}
        </Button>
        {#if status !== 'running'}
          <Button variant="ghost" size="icon-sm" class="toast-close" data-source-update-id={update.sourceUpdateId} aria-label="Fechar aviso da atualização" onclick={dismissAutomationUpdate}>
            <span class="material-symbols-outlined">close</span>
          </Button>
        {/if}
      </section>
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
    cursor: default;
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

  .automation-toast.cancelled {
    border-top-color: var(--color-warning);
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
  .automation-toast.cancelled .toast-icon { color: var(--color-warning); }

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

  :global([data-slot="button"].automation-action-btn) {
    color: var(--color-interactive);
    font-family: var(--font-body);
    font-size: var(--type-label);
    min-height: var(--size-touch-target);
    flex-shrink: 0;
    white-space: nowrap;
  }

  :global([data-slot="button"].automation-action-btn:hover) {
    background: color-mix(in srgb, var(--color-interactive) 15%, transparent);
    color: var(--color-interactive-hover);
  }

  .automation-progress {
    width: 100%;
    height: var(--size-startup-progress-height);
    accent-color: var(--color-transition);
  }

  .toast-progress-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin-top: var(--space-1);
  }

  .toast-progress-item {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--space-1) var(--space-2);
  }

  .toast-progress-item .toast-detail:first-child {
    grid-column: 1 / -1;
  }

  .toast-progress-item .automation-progress {
    grid-column: 1;
  }

  .toast-progress-item .toast-detail:last-child,
  .toast-progress-item .toast-error:last-child {
    grid-column: 2;
    grid-row: 2;
    text-align: right;
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

</style>
