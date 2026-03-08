<script>
  import { onMount, onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';

  let { widgetId, entityId = '' } = $props();

  let processLabel = $state('');
  let running = $state(false);
  let steps = $state([]);
  let error = $state(null);
  let unlistenEntityUpdated = null;

  async function loadProcess() {
    if (!entityId) return;
    try {
      const resultStr = await invoke('entity__get', { entityId });
      const data = JSON.parse(resultStr);
      processLabel = data?.label ?? entityId;
    } catch {
      processLabel = entityId;
    }
  }

  async function executeProcess() {
    if (running) return;
    running = true;
    error = null;
    steps = [];
    try {
      await invoke('process__execute', { processIri: entityId });
    } catch (err) {
      error = String(err);
    } finally {
      running = false;
    }
  }

  async function closeWidget() {
    try {
      await invoke('widget__remove', { widgetId });
    } catch (err) {
      console.error('Failed to remove widget:', err);
    }
  }

  onMount(async () => {
    await loadProcess();

    if (entityId) {
      unlistenEntityUpdated = await listen('entity-updated', async (event) => {
        if (event.payload.entityId === entityId) {
          await loadProcess();
        }
      });
    }
  });

  onDestroy(() => {
    if (unlistenEntityUpdated) unlistenEntityUpdated();
  });
</script>

<div class="process-status-widget">
  <div class="widget-header">
    <div class="header-left">
      <span class="material-symbols-outlined header-icon">autorenew</span>
      <span class="header-title">{processLabel || 'Process'}</span>
    </div>
    <div class="header-actions">
      <button
        class="run-btn"
        onclick={executeProcess}
        disabled={running}
        title="Run process"
      >
        <span class="material-symbols-outlined">{running ? 'progress_activity' : 'play_arrow'}</span>
      </button>
      <button class="close-btn" onclick={closeWidget}>
        <span class="material-symbols-outlined">close</span>
      </button>
    </div>
  </div>

  <div class="widget-content">
    {#if error}
      <div class="status-error">
        <span class="material-symbols-outlined">error</span>
        <span>{error}</span>
      </div>
    {:else if running}
      <div class="status-running">
        <span class="material-symbols-outlined spinning">progress_activity</span>
        <span>Running…</span>
      </div>
    {:else if steps.length === 0}
      <div class="status-idle">
        <span class="material-symbols-outlined">play_circle</span>
        <span>Press Run to execute this process</span>
      </div>
    {:else}
      <ul class="step-list">
        {#each steps as step}
          <li class="step-item" class:step-ok={step.status === 'ok'} class:step-err={step.status === 'error'}>
            <span class="step-icon material-symbols-outlined">
              {step.status === 'ok' ? 'check_circle' : 'cancel'}
            </span>
            <div class="step-body">
              <span class="step-label">{step.label}</span>
              {#if step.output}
                <span class="step-output">{step.output}</span>
              {/if}
            </div>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</div>

<style>
  .process-status-widget {
    width: 100%;
    height: 100%;
    display: flex;
    flex-direction: column;
    background: color-mix(in srgb, var(--color-black) 85%, transparent);
    backdrop-filter: blur(20px);
    border: 1px solid color-mix(in srgb, var(--color-white) 20%, transparent);
    border-radius: 12px;
    overflow: hidden;
    box-shadow: 0 8px 32px color-mix(in srgb, var(--color-black) 40%, transparent);
  }

  .widget-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 16px;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    border-bottom: 1px solid color-mix(in srgb, var(--color-white) 15%, transparent);
    flex-shrink: 0;
  }

  .header-left {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .header-icon {
    font-size: 22px;
    color: var(--color-interactive);
  }

  .header-title {
    font-family: var(--font-title);
    font-size: 11px;
    font-weight: 600;
    color: var(--color-neutral-active);
  }

  .header-actions {
    display: flex;
    align-items: center;
    gap: 4px;
  }

  .run-btn,
  .close-btn {
    background: none;
    border: none;
    padding: 4px;
    cursor: pointer;
    color: var(--color-interactive);
    border-radius: 4px;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.2s;
  }

  .run-btn:hover:not(:disabled),
  .close-btn:hover {
    background: color-mix(in srgb, var(--color-interactive) 15%, transparent);
    color: var(--color-neutral-active);
  }

  .run-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .run-btn .material-symbols-outlined,
  .close-btn .material-symbols-outlined {
    font-size: 20px;
  }

  .widget-content {
    flex: 1;
    overflow: auto;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .status-idle,
  .status-running,
  .status-error {
    display: flex;
    align-items: center;
    gap: 10px;
    font-size: 12px;
    color: var(--color-neutral);
    padding: 12px 0;
  }

  .status-error {
    color: var(--color-error, #ef4444);
  }

  .status-error .material-symbols-outlined,
  .status-idle .material-symbols-outlined,
  .status-running .material-symbols-outlined {
    font-size: 20px;
    flex-shrink: 0;
  }

  .spinning {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  .step-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .step-item {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 8px 10px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--color-white) 4%, transparent);
    font-size: 12px;
  }

  .step-item.step-ok {
    border-left: 2px solid var(--color-success, #22c55e);
  }

  .step-item.step-err {
    border-left: 2px solid var(--color-error, #ef4444);
  }

  .step-icon {
    font-size: 16px;
    flex-shrink: 0;
    margin-top: 1px;
  }

  .step-ok .step-icon {
    color: var(--color-success, #22c55e);
  }

  .step-err .step-icon {
    color: var(--color-error, #ef4444);
  }

  .step-body {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }

  .step-label {
    color: var(--color-neutral-active);
    font-weight: 500;
  }

  .step-output {
    color: var(--color-neutral);
    font-family: var(--font-mono, monospace);
    font-size: 11px;
    word-break: break-all;
  }
</style>
