<script lang="ts">
  import '$lib/fonts.css';
  import '$lib/colors.css';
  import '$lib/markdown.css';
  import { initializeLogging } from '$lib/logging.js';
  import { onMount, onDestroy } from 'svelte';
  const { children } = $props();
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { openUrl } from '@tauri-apps/plugin-opener';

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

  let retentionRunning = $state(false);
  let unlistenRetentionStarted: (() => void) | undefined;
  let unlistenRetentionComplete: (() => void) | undefined;

  const backgroundVideos = [
    '/background-space.mp4',
    '/background-code.mp4',
    '/background-dust.mp4',
    '/background-edges.mp4',
    '/background-particles.mp4'
  ];

  const selectedVideo = backgroundVideos[Math.floor(Math.random() * backgroundVideos.length)];

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

    unlistenRetentionStarted = await listen('retention-started', () => {
      retentionRunning = true;
    });

    unlistenRetentionComplete = await listen('retention-complete', () => {
      retentionRunning = false;
    });

    try {
      await invoke('initialize_app');
    } catch (err) {
      console.error('[App] Failed to initialize database:', err);
      alert('Failed to initialize database. Please check permissions and try again.');
    }

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
    unlistenRecalc?.();
    unlistenRetentionStarted?.();
    unlistenRetentionComplete?.();
    unlistenAutomationStarted?.();
    unlistenAutomationStep?.();
    unlistenAutomationFinished?.();
  });
</script>

<video
  autoplay
  loop
  muted
  playsinline
  class="background-video"
>
  <source src={selectedVideo} type="video/mp4" />
</video>

{@render children()}

{#if automationRuns.length > 0 || retentionRunning || recalcJobs.length > 0}
  <div class="toast-stack">
    {#each automationRuns as run (run.executionIri)}
      <div
        class="automation-toast"
        class:failed={run.status === 'failed'}
        class:done={run.status === 'completed'}
        class:running={run.status === 'running'}
        role="button"
        tabindex="0"
        onclick={() => invoke('widget_blackboard__add_widget', { widgetType: 'workflow_execution', entityId: run.executionIri, position: null, size: null, conversationId: null }).catch(e => console.error('[toast] Failed to open widget:', e))}
        onkeydown={(e) => e.key === 'Enter' && invoke('widget_blackboard__add_widget', { widgetType: 'workflow_execution', entityId: run.executionIri, position: null, size: null, conversationId: null }).catch(e => console.error('[toast] Failed to open widget:', e))}
      >
        <span class="material-symbols-outlined toast-icon" class:spinning={run.status === 'running'}>
          {#if run.status === 'running'}progress_activity{:else if run.status === 'completed'}check_circle{:else}error{/if}
        </span>
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
  }

  .background-video {
    position: fixed;
    top: 0;
    left: 0;
    width: 100vw;
    height: 100vh;
    object-fit: cover;
    z-index: 0;
    opacity: 0.2;
    pointer-events: none;
  }

  .spinning {
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
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
    background: color-mix(in srgb, var(--color-black) 85%, transparent);
    backdrop-filter: blur(12px);
    border: 1px solid color-mix(in srgb, var(--color-white) 20%, transparent);
    color: var(--color-neutral);
    border-radius: 10px;
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
    background: color-mix(in srgb, var(--color-transition) 10%, transparent);
    border-color: color-mix(in srgb, var(--color-transition) 30%, transparent);
    animation: toast-pulse 1.5s ease-in-out infinite;
  }

  .automation-toast.failed {
    border-color: var(--color-danger);
  }

  .automation-toast.done {
    border-color: var(--color-success);
  }

  @keyframes toast-pulse {
    0%, 100% { background: color-mix(in srgb, var(--color-transition) 10%, transparent); }
    50% { background: color-mix(in srgb, var(--color-transition) 18%, transparent); }
  }

  .toast-icon {
    font-size: 18px;
    flex-shrink: 0;
    margin-top: 1px;
  }

  .automation-toast.running .toast-icon { color: var(--color-transition); }
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
    border-radius: 4px;
    align-self: flex-start;
  }

  .toast-close .material-symbols-outlined {
    font-size: 16px;
  }

  .system-toast {
    background: color-mix(in srgb, var(--color-black) 85%, transparent);
    backdrop-filter: blur(12px);
    border: 1px solid color-mix(in srgb, var(--color-white) 15%, transparent);
    color: var(--color-neutral);
    border-radius: 10px;
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
