<script>
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import WidgetContainer from './WidgetContainer.svelte';

  let { widgetId, entityId = '', windowState = 'normal', onWindowStateChange } = $props();

  let connectorLabel = $state('');
  let authType = $state('api_key');
  let isConfigured = $state(false);
  let saving = $state(false);
  let testing = $state(false);
  let testResult = $state(null);
  let saveError = $state(null);

  // Form fields
  let apiKeyValue = $state('');
  let tokenValue = $state('');
  let username = $state('');
  let password = $state('');

  async function loadConnector() {
    if (!entityId) return;
    try {
      const resultStr = await invoke('inspector__get_entity', { entityId });
      const data = JSON.parse(resultStr);
      connectorLabel = data?.label ?? entityId;

      const summary = await invoke('connector__get_credential_summary', { connectorIri: entityId });
      isConfigured = summary.is_configured;
      if (summary.auth_type) authType = summary.auth_type;
    } catch {
      connectorLabel = entityId;
    }
  }

  async function saveCredential() {
    if (saving) return;
    saving = true;
    saveError = null;
    testResult = null;
    try {
      let credential = { auth_type: authType };
      if (authType === 'api_key') credential.value = apiKeyValue;
      else if (authType === 'token') credential.value = tokenValue;
      else if (authType === 'username_password') {
        credential.username = username;
        credential.password = password;
      }
      await invoke('connector__save_credential', { connectorIri: entityId, credential });
      isConfigured = true;
    } catch (err) {
      saveError = String(err);
    } finally {
      saving = false;
    }
  }

  async function testAuth() {
    if (testing) return;
    testing = true;
    testResult = null;
    try {
      const msg = await invoke('connector__test_auth', { connectorIri: entityId });
      testResult = { ok: true, message: msg };
    } catch (err) {
      testResult = { ok: false, message: String(err) };
    } finally {
      testing = false;
    }
  }

  async function openInspector() {
    await invoke('widget_blackboard__add_widget', {
      widgetType: 'inspector',
      entityId,
      position: null,
      size: null,
      conversationId: null,
    }).catch(() => {});
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
  icon="vpn_key"
  title={connectorLabel || 'Connector Credentials'}
  {windowState}
  {onWindowStateChange}
  onClose={closeWidget}
>
  {#snippet headerActions()}
    {#if isConfigured}
      <span class="configured-badge" title="Credentials configured">
        <span class="material-symbols-outlined">check_circle</span>
      </span>
    {/if}
    <button class="action-btn" onclick={openInspector} title="Open inspector">
      <span class="material-symbols-outlined">info</span>
    </button>
  {/snippet}

  <div class="widget-content">
    <div class="field-group">
      <label class="field-label" for="field-auth-type">Auth Type</label>
      <select id="field-auth-type" class="field-select" bind:value={authType}>
        <option value="api_key">API Key</option>
        <option value="token">Bearer Token</option>
        <option value="username_password">Username / Password</option>
      </select>
    </div>

    {#if authType === 'api_key'}
      <div class="field-group">
        <label class="field-label" for="field-api-key">API Key</label>
        <input
          id="field-api-key"
          class="field-input"
          type="password"
          placeholder="Enter API key…"
          bind:value={apiKeyValue}
        />
      </div>
    {:else if authType === 'token'}
      <div class="field-group">
        <label class="field-label" for="field-bearer-token">Bearer Token</label>
        <input
          id="field-bearer-token"
          class="field-input"
          type="password"
          placeholder="Enter token…"
          bind:value={tokenValue}
        />
      </div>
    {:else if authType === 'username_password'}
      <div class="field-group">
        <label class="field-label" for="field-username">Username</label>
        <input
          id="field-username"
          class="field-input"
          type="text"
          placeholder="Enter username…"
          bind:value={username}
        />
      </div>
      <div class="field-group">
        <label class="field-label" for="field-password">Password</label>
        <input
          id="field-password"
          class="field-input"
          type="password"
          placeholder="Enter password…"
          bind:value={password}
        />
      </div>
    {/if}

    {#if saveError}
      <div class="status-error">
        <span class="material-symbols-outlined">error</span>
        <span>{saveError}</span>
      </div>
    {/if}

    {#if testResult}
      <div class="status-result" class:ok={testResult.ok} class:fail={!testResult.ok}>
        <span class="material-symbols-outlined">{testResult.ok ? 'check_circle' : 'cancel'}</span>
        <span>{testResult.message}</span>
      </div>
    {/if}

    <div class="action-row">
      <button class="btn btn-primary" onclick={saveCredential} disabled={saving}>
        {#if saving}
          <span class="material-symbols-outlined spinning">progress_activity</span>
        {:else}
          <span class="material-symbols-outlined">save</span>
        {/if}
        Save
      </button>
      <button class="btn btn-secondary" onclick={testAuth} disabled={testing || !isConfigured}>
        {#if testing}
          <span class="material-symbols-outlined spinning">progress_activity</span>
        {:else}
          <span class="material-symbols-outlined">wifi_tethering</span>
        {/if}
        Test
      </button>
    </div>
  </div>
</WidgetContainer>

<style>
  .configured-badge .material-symbols-outlined {
    font-size: 18px;
    color: var(--color-success, #22c55e);
  }

  .widget-content {
    flex: 1;
    overflow: auto;
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .field-group {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .field-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--color-neutral);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .field-input,
  .field-select {
    background: color-mix(in srgb, var(--color-white) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-white) 15%, transparent);
    padding: 8px 10px;
    font-size: 14px;
    color: var(--color-neutral-active);
    outline: none;
    transition: border-color 0.15s;
  }

  .field-input:focus,
  .field-select:focus {
    border-color: var(--color-interactive);
  }

  .field-select option {
    background: #1e293b;
    color: var(--color-neutral-active);
  }

  .status-error,
  .status-result {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    padding: 8px 10px;
    font-size: 12px;
  }

  .status-error {
    background: color-mix(in srgb, var(--color-error, #ef4444) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-error, #ef4444) 30%, transparent);
    color: var(--color-error, #ef4444);
  }

  .status-result.ok {
    background: color-mix(in srgb, var(--color-success, #22c55e) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-success, #22c55e) 30%, transparent);
    color: var(--color-success, #22c55e);
  }

  .status-result.fail {
    background: color-mix(in srgb, var(--color-error, #ef4444) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-error, #ef4444) 30%, transparent);
    color: var(--color-error, #ef4444);
  }

  .status-error .material-symbols-outlined,
  .status-result .material-symbols-outlined {
    font-size: 16px;
    flex-shrink: 0;
    margin-top: 1px;
  }

  .action-row {
    display: flex;
    gap: 8px;
    margin-top: auto;
    padding-top: 4px;
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
    border: 1px solid color-mix(in srgb, var(--color-white) 20%, transparent);
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
