<script>
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import WidgetContainer from './WidgetContainer.svelte';

  let { widgetId, entityId = '', conversationIri = null, windowState = 'normal', onWindowStateChange } = $props();

  let connectorLabel = $state('');
  let exporting = $state(false);
  let importing = $state(false);
  let exportJson = $state(null);
  let importText = $state('');
  let importMode = $state(false);
  let message = $state(null);

  async function loadConnector() {
    if (!entityId) return;
    try {
      const resultStr = await invoke('inspector__get_entity', { entityId });
      const data = JSON.parse(resultStr);
      connectorLabel = data?.label ?? entityId;
    } catch {
      connectorLabel = entityId;
    }
  }

  async function exportPackage() {
    if (exporting) return;
    exporting = true;
    message = null;
    try {
      const pkg = await invoke('connector__export_package', {
        connectorIri: entityId,
        author: null,
      });
      exportJson = JSON.stringify(pkg, null, 2);
      message = { ok: true, text: 'Package exported — copy the JSON below.' };
    } catch (err) {
      message = { ok: false, text: String(err) };
    } finally {
      exporting = false;
    }
  }

  async function copyJson() {
    if (!exportJson) return;
    await navigator.clipboard.writeText(exportJson);
    message = { ok: true, text: 'Copied to clipboard.' };
  }

  async function importPackage() {
    if (importing || !importText.trim()) return;
    importing = true;
    message = null;
    try {
      const pkg = JSON.parse(importText);
      const iri = await invoke('connector__import_package', { package: pkg });
      message = { ok: true, text: `Connector imported: ${iri}` };
      importText = '';
      importMode = false;
    } catch (err) {
      message = { ok: false, text: String(err) };
    } finally {
      importing = false;
    }
  }

  async function openCredentials() {
    try {
      await invoke('widget_blackboard__add_widget', {
        widgetType: 'connector_credential',
        entityId,
        position: null,
        size: null,
        conversationId: conversationIri,
      });
    } catch (err) {
      message = { ok: false, text: String(err) };
    }
  }

  async function closeWidget() {
    try {
      await invoke('widget_blackboard__remove_widget', { widgetId });
    } catch (err) {
      console.error('Failed to remove widget:', err);
    }
  }

  onMount(async () => {
    await loadConnector();
  });
</script>

<WidgetContainer
  icon="package_2"
  title={connectorLabel || 'Connector Manager'}
  {windowState}
  {onWindowStateChange}
  onClose={closeWidget}
>
  <div class="widget-content">
    {#if message}
      <div class="status-msg" class:ok={message.ok} class:fail={!message.ok}>
        <span class="material-symbols-outlined">{message.ok ? 'check_circle' : 'error'}</span>
        <span>{message.text}</span>
      </div>
    {/if}

    {#if !importMode}
      <div class="action-grid">
        <button class="action-card" onclick={exportPackage} disabled={exporting}>
          <span class="material-symbols-outlined action-icon">upload</span>
          <span class="action-label">{exporting ? 'Exporting…' : 'Export Package'}</span>
          <span class="action-desc">Share connector as JSON</span>
        </button>
        <button class="action-card" onclick={() => { importMode = true; exportJson = null; message = null; }}>
          <span class="material-symbols-outlined action-icon">download</span>
          <span class="action-label">Import Package</span>
          <span class="action-desc">Install connector from JSON</span>
        </button>
        <button class="action-card" onclick={openCredentials}>
          <span class="material-symbols-outlined action-icon">vpn_key</span>
          <span class="action-label">Credentials</span>
          <span class="action-desc">Configure authentication</span>
        </button>
      </div>

      {#if exportJson}
        <div class="export-section">
          <div class="export-header">
            <span class="export-title">Package JSON</span>
            <button class="copy-btn" onclick={copyJson}>
              <span class="material-symbols-outlined">content_copy</span>
              Copy
            </button>
          </div>
          <pre class="json-preview">{exportJson}</pre>
        </div>
      {/if}
    {:else}
      <div class="import-section">
        <label class="field-label" for="field-connector-json">Paste connector package JSON</label>
        <textarea
          id="field-connector-json"
          class="json-input"
          placeholder={"{ \"schema_version\": \"1.0\", ... }"}
          bind:value={importText}
          spellcheck="false"
        ></textarea>
        <div class="import-actions">
          <button class="btn btn-primary" onclick={importPackage} disabled={importing || !importText.trim()}>
            {#if importing}
              <span class="material-symbols-outlined spinning">progress_activity</span>
            {:else}
              <span class="material-symbols-outlined">download</span>
            {/if}
            Import
          </button>
          <button class="btn btn-secondary" onclick={() => { importMode = false; message = null; }}>
            Cancel
          </button>
        </div>
      </div>
    {/if}
  </div>
</WidgetContainer>

<style>
  .widget-content {
    flex: 1;
    overflow: auto;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .status-msg {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 8px 10px;
    font-size: 12px;
  }

  .status-msg.ok {
    background: color-mix(in srgb, var(--color-success, #22c55e) 10%, transparent);
    color: var(--color-success, #22c55e);
  }

  .status-msg.fail {
    background: color-mix(in srgb, var(--color-error, #ef4444) 10%, transparent);
    color: var(--color-error, #ef4444);
  }

  .status-msg .material-symbols-outlined {
    font-size: 16px;
    flex-shrink: 0;
    margin-top: 1px;
  }

  .action-grid {
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    gap: 8px;
  }

  .action-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 6px;
    padding: 14px 8px;
    background: color-mix(in srgb, var(--color-white) 6%, transparent);
    cursor: pointer;
    transition: background 0.2s;
    text-align: center;
  }

  .action-card:hover:not(:disabled) {
    background: color-mix(in srgb, var(--color-interactive) 12%, transparent);
  }

  .action-card:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .action-icon {
    font-size: 24px;
    color: var(--color-interactive);
  }

  .action-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--color-neutral-active);
  }

  .action-desc {
    font-size: 10px;
    color: var(--color-neutral);
  }

  .export-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .export-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }

  .export-title {
    font-size: 11px;
    font-weight: 600;
    color: var(--color-neutral);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .copy-btn {
    display: flex;
    align-items: center;
    gap: 4px;
    background: none;
    border: none;
    padding: 4px 8px;
    font-size: 11px;
    color: var(--color-interactive);
    cursor: pointer;
    transition: background 0.2s;
  }

  .copy-btn:hover {
    background: color-mix(in srgb, var(--color-interactive) 10%, transparent);
  }

  .copy-btn .material-symbols-outlined {
    font-size: 14px;
  }

  .json-preview {
    background: color-mix(in srgb, var(--color-black) 60%, transparent);
    padding: 10px;
    font-family: var(--font-mono, monospace);
    font-size: 11px;
    color: var(--color-neutral);
    overflow: auto;
    max-height: 200px;
    white-space: pre;
    word-break: break-all;
  }

  .import-section {
    display: flex;
    flex-direction: column;
    gap: 8px;
    flex: 1;
  }

  .field-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--color-neutral);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .json-input {
    flex: 1;
    min-height: 160px;
    background: color-mix(in srgb, var(--color-white) 6%, transparent);
    border: none;
    padding: 10px;
    font-family: var(--font-mono, monospace);
    font-size: 12px;
    color: var(--color-neutral-active);
    outline: none;
    resize: vertical;
  }

  .import-actions {
    display: flex;
    gap: 8px;
  }

  .btn {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 16px;
    border: none;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition: all 0.2s;
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .btn .material-symbols-outlined {
    font-size: 16px;
  }

  .btn-primary {
    background: var(--color-interactive);
    color: white;
  }

  .btn-primary:hover:not(:disabled) {
    filter: brightness(1.1);
  }

  .btn-secondary {
    background: color-mix(in srgb, var(--color-white) 10%, transparent);
    color: var(--color-neutral-active);
  }

  .btn-secondary:hover:not(:disabled) {
    background: color-mix(in srgb, var(--color-white) 15%, transparent);
  }

  .spinning {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }
</style>
