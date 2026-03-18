<script lang="ts">
  import '$lib/fonts.css';
  import '$lib/colors.css';
  import '$lib/markdown.css';
  import { initializeLogging } from '$lib/logging.js';
  import { onMount, onDestroy } from 'svelte';
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

  let automationRuns: AutomationRun[] = [];
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

  let recalcJobs: RecalcJob[] = [];
  let unlistenRecalc: (() => void) | undefined;

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

<slot />

{#if automationRuns.length > 0}
  <div class="automation-toasts">
    {#each automationRuns as run (run.executionIri)}
      <div class="automation-toast" class:failed={run.status === 'failed'} class:done={run.status === 'completed'} class:running={run.status === 'running'}>
        <span class="material-symbols-outlined toast-icon">
          {#if run.status === 'running'}autorenew{:else if run.status === 'completed'}check_circle{:else}error{/if}
        </span>
        <div class="toast-body">
          {#if run.status === 'failed'}
            <span class="toast-label">Automation failed: <strong>{run.currentStep}</strong></span>
            {#if run.error}<span class="toast-error">{run.error}</span>{/if}
            <button class="toast-inspect" onclick={() => invoke('widget_blackboard__add_widget', { widgetType: 'inspector', entityId: run.executionIri, position: null, size: null, conversationId: null })}>
              Inspect
            </button>
          {:else if run.status === 'completed'}
            <span class="toast-label">Automation completed</span>
          {:else}
            <span class="toast-label">Running: <strong>{run.currentStep}</strong></span>
          {/if}
        </div>
      </div>
    {/each}
  </div>
{/if}

{#if recalcJobs.length > 0}
  <div class="recalc-toasts">
    {#each recalcJobs as job (job.jobId)}
      <div class="recalc-toast" class:done={job.status !== 'running'}>
        <span class="recalc-label">
          Recalculating <strong>{job.classLabel}</strong>
        </span>
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

  .automation-toasts {
    position: fixed;
    bottom: 16px;
    left: 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: 9999;
    pointer-events: none;
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
    transition: background 0.3s, border-color 0.3s;
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

  .toast-inspect {
    margin-top: 4px;
    background: none;
    border: 1px solid var(--color-danger);
    color: color-mix(in srgb, var(--color-danger) 80%, white);
    border-radius: 4px;
    padding: 2px 8px;
    font-size: 11px;
    cursor: pointer;
    width: fit-content;
  }

  .toast-inspect:hover {
    background: color-mix(in srgb, #ef4444 20%, transparent);
  }

  .recalc-toasts {
    position: fixed;
    bottom: 16px;
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    flex-direction: column;
    gap: 8px;
    z-index: 9999;
    pointer-events: none;
  }

  .recalc-toast {
    background: var(--surface-2, #2a2a2a);
    color: var(--text-1, #fff);
    border-radius: 8px;
    padding: 8px 16px;
    display: flex;
    align-items: center;
    gap: 12px;
    font-size: 13px;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
    transition: opacity 0.3s;
  }

  .recalc-toast.done {
    opacity: 0.6;
  }

  .recalc-pct {
    font-weight: 600;
    min-width: 36px;
    text-align: right;
  }
</style>
