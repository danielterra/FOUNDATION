<script lang="ts">
  import '$lib/fonts.css';
  import '$lib/colors.css';
  import '$lib/markdown.css';
  import { initializeLogging } from '$lib/logging.js';
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';

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

  onMount(async () => {
    initializeLogging();

    try {
      await invoke('initialize_app');
    } catch (err) {
      console.error('[App] Failed to initialize database:', err);
      alert('Failed to initialize database. Please check permissions and try again.');
    }

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
    unlistenRecalc?.();
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
