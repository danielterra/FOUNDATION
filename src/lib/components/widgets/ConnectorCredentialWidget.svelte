<script>
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import WidgetContainer from './WidgetContainer.svelte';
  import { Button } from '$lib/components/ui/button';
  import * as Select from '$lib/components/ui/select';
  import { Input } from '$lib/components/ui/input';

  let { widgetId, entityId = '', windowState = 'normal', onWindowStateChange } = $props();

  let connectorLabel = $state('');
  let authType = $state('api_key');
  let isConfigured = $state(false);
  let saving = $state(false);
  let testing = $state(false);
  let testResult = $state(null);
  let saveError = $state(null);

  // Form fields — existing auth types
  let apiKeyValue = $state('');
  let tokenValue = $state('');
  let username = $state('');
  let password = $state('');

  // Form fields — OAuth2
  let oauth2ClientId = $state('');
  let oauth2ClientSecret = $state('');
  let oauth2TokenUrl = $state('');
  let oauth2Scopes = $state('');
  let oauth2ScopeInput = $state('');
  let oauth2ShowSecret = $state(false);
  // IRI da credencial OAuth2 salva (retornado pelo save, usado para test/summary)
  let oauth2CredentialIri = $state('');

  function oauth2ScopeList() {
    return oauth2Scopes
      .split(' ')
      .map(s => s.trim())
      .filter(s => s.length > 0);
  }

  function addScope() {
    const trimmed = oauth2ScopeInput.trim();
    if (!trimmed) return;
    const existing = oauth2ScopeList();
    if (!existing.includes(trimmed)) {
      oauth2Scopes = [...existing, trimmed].join(' ');
    }
    oauth2ScopeInput = '';
  }

  function removeScope(scope) {
    oauth2Scopes = oauth2ScopeList().filter(s => s !== scope).join(' ');
  }

  async function loadConnector() {
    if (!entityId) return;
    try {
      const resultStr = await invoke('inspector__get_entity', { entityId });
      const data = JSON.parse(resultStr);
      connectorLabel = data?.label ?? entityId;

      const summary = await invoke('connector__get_credential_summary', { connectorIri: entityId });
      isConfigured = summary.is_configured;
      if (summary.auth_type) authType = summary.auth_type;

      if (summary.auth_type === 'oauth2' && summary.credential_iri) {
        oauth2CredentialIri = summary.credential_iri;
        const oauth2Summary = await invoke('connector__get_oauth2_summary', {
          credIri: summary.credential_iri,
        });
        oauth2ClientId = oauth2Summary.client_id ?? '';
        oauth2ClientSecret = oauth2Summary.client_secret ?? '';
        oauth2TokenUrl = oauth2Summary.token_url ?? '';
        oauth2Scopes = oauth2Summary.scopes ?? '';
      }
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
      if (authType === 'api_key') {
        credential.value = apiKeyValue;
      } else if (authType === 'token') {
        credential.value = tokenValue;
      } else if (authType === 'username_password') {
        credential.username = username;
        credential.password = password;
      } else if (authType === 'oauth2') {
        credential.client_id = oauth2ClientId;
        credential.client_secret = oauth2ClientSecret;
        credential.token_url = oauth2TokenUrl;
        if (oauth2Scopes.trim()) credential.scopes = oauth2Scopes.trim();
      }
      const result = await invoke('connector__save_credential', { connectorIri: entityId, credential });
      if (authType === 'oauth2') {
        oauth2CredentialIri = String(result);
      }
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
      if (authType === 'oauth2') {
        await invoke('connector__test_oauth2_auth', { credIri: oauth2CredentialIri });
        testResult = { ok: true, message: 'Autenticação OAuth2 bem-sucedida' };
      } else {
        const msg = await invoke('connector__test_auth', { connectorIri: entityId });
        testResult = { ok: true, message: msg };
      }
    } catch (err) {
      testResult = { ok: false, message: String(err) };
    } finally {
      testing = false;
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
  icon="vpn_key"
  title={connectorLabel || 'Connector Credentials'}
  {windowState}
  {onWindowStateChange}
  onClose={closeWidget}
  {entityId}
>
  {#snippet headerActions()}
    {#if isConfigured}
      <span class="configured-badge" title="Credentials configured">
        <span class="material-symbols-outlined">check_circle</span>
      </span>
    {/if}
  {/snippet}

  <div class="widget-content">
    <div class="field-group">
      <label class="field-label" for="field-auth-type">Auth Type</label>
      <Select.Root
        type="single"
        value={authType}
        onValueChange={(v) => { if (v) authType = v; }}
      >
        <Select.Trigger id="field-auth-type" class="select-trigger-override">
          <Select.Value />
        </Select.Trigger>
        <Select.Content>
          <Select.Item value="api_key">API Key</Select.Item>
          <Select.Item value="token">Bearer Token</Select.Item>
          <Select.Item value="username_password">Username / Password</Select.Item>
          <Select.Item value="oauth2">OAuth2</Select.Item>
        </Select.Content>
      </Select.Root>
    </div>

    {#if authType === 'api_key'}
      <div class="field-group">
        <label class="field-label" for="field-api-key">API Key</label>
        <Input
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
        <Input
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
        <Input
          id="field-username"
          class="field-input"
          type="text"
          placeholder="Enter username…"
          bind:value={username}
        />
      </div>
      <div class="field-group">
        <label class="field-label" for="field-password">Password</label>
        <Input
          id="field-password"
          class="field-input"
          type="password"
          placeholder="Enter password…"
          bind:value={password}
        />
      </div>
    {:else if authType === 'oauth2'}
      <div class="field-group">
        <label class="field-label" for="field-oauth2-client-id">Client ID</label>
        <Input
          id="field-oauth2-client-id"
          class="field-input"
          type="text"
          placeholder="Client ID…"
          bind:value={oauth2ClientId}
          disabled={saving}
        />
      </div>
      <div class="field-group">
        <label class="field-label" for="field-oauth2-client-secret">Client Secret</label>
        <div class="secret-row">
          <Input
            id="field-oauth2-client-secret"
            class="field-input secret-input"
            type={oauth2ShowSecret ? 'text' : 'password'}
            placeholder={isConfigured && !oauth2ClientSecret ? '••••••••' : 'Client Secret…'}
            bind:value={oauth2ClientSecret}
            disabled={saving}
            autocomplete="new-password"
          />
          <Button
            variant="ghost"
            size="icon"
            type="button"
            onclick={() => oauth2ShowSecret = !oauth2ShowSecret}
            aria-label={oauth2ShowSecret ? 'Ocultar client secret' : 'Mostrar client secret'}
          >
            <span class="material-symbols-outlined">
              {oauth2ShowSecret ? 'visibility_off' : 'visibility'}
            </span>
          </Button>
        </div>
      </div>
      <div class="field-group">
        <label class="field-label" for="field-oauth2-token-url">Token URL</label>
        <Input
          id="field-oauth2-token-url"
          class="field-input"
          type="text"
          placeholder="https://auth.example.com/token"
          bind:value={oauth2TokenUrl}
          disabled={saving}
        />
      </div>
      <div class="field-group">
        <label class="field-label" for="field-oauth2-scope-input">Scopes</label>
        <div class="scopes-field">
          {#if oauth2ScopeList().length > 0}
            <div class="scope-chips">
              {#each oauth2ScopeList() as scope (scope)}
                <span class="scope-chip">
                  <span class="scope-chip-label">{scope}</span>
                  <Button
                    variant="ghost"
                    size="icon"
                    type="button"
                    class="scope-chip-remove"
                    onclick={() => removeScope(scope)}
                    aria-label="Remover scope {scope}"
                    disabled={saving}
                  >
                    <span class="material-symbols-outlined">close</span>
                  </Button>
                </span>
              {/each}
            </div>
          {/if}
          <div class="scope-add-row">
            <Input
              id="field-oauth2-scope-input"
              class="field-input scope-add-input"
              type="text"
              placeholder="Adicionar scope…"
              bind:value={oauth2ScopeInput}
              disabled={saving}
              onkeydown={(e) => {
                if (e.key === 'Enter') { e.preventDefault(); addScope(); }
              }}
            />
            <Button
              variant="ghost"
              size="icon"
              type="button"
              class="scope-add-btn"
              onclick={addScope}
              disabled={saving || !oauth2ScopeInput.trim()}
              aria-label="Adicionar scope"
            >
              <span class="material-symbols-outlined">add</span>
            </Button>
          </div>
        </div>
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
      <Button variant="default" onclick={saveCredential} disabled={saving}>
        {#if saving}
          <span class="material-symbols-outlined spinning">progress_activity</span>
        {:else}
          <span class="material-symbols-outlined">save</span>
        {/if}
        Save
      </Button>
      <Button variant="secondary" onclick={testAuth} disabled={testing || !isConfigured}>
        {#if testing}
          <span class="material-symbols-outlined spinning">progress_activity</span>
        {:else}
          <span class="material-symbols-outlined">wifi_tethering</span>
        {/if}
        Test
      </Button>
    </div>
  </div>
</WidgetContainer>

<style>
  .configured-badge .material-symbols-outlined {
    font-size: 18px;
    color: var(--color-success);
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

  :global([data-slot="input"].field-input) {
    background: color-mix(in srgb, var(--color-white) 8%, transparent);
    border: none;
    box-shadow: none;
    border-radius: 0;
    padding: 8px 10px;
    font-size: 14px;
    color: var(--color-neutral-active);
    height: auto;
  }

  :global(.select-trigger-override) {
    background: color-mix(in srgb, var(--color-white) 8%, transparent);
    border: none;
    border-radius: 0;
    color: var(--color-neutral-active);
    font-size: 14px;
    height: auto;
    padding: 8px 10px;
    box-shadow: none;
  }

  :global(.select-trigger-override:hover) {
    background: color-mix(in srgb, var(--color-white) 14%, transparent);
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
    background: color-mix(in srgb, var(--color-error) 10%, transparent);
    color: var(--color-error);
  }

  .status-result.ok {
    background: color-mix(in srgb, var(--color-success) 10%, transparent);
    color: var(--color-success);
  }

  .status-result.fail {
    background: color-mix(in srgb, var(--color-error) 10%, transparent);
    color: var(--color-error);
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

  .spinning {
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    from { transform: rotate(0deg); }
    to { transform: rotate(360deg); }
  }

  .secret-row {
    display: flex;
    align-items: center;
    gap: 0;
  }

  :global([data-slot="input"].secret-input) {
    flex: 1;
    min-width: 0;
  }

  .secret-row :global([data-slot="button"]) {
    color: var(--color-neutral);
    flex-shrink: 0;
    background: color-mix(in srgb, var(--color-white) 8%, transparent);
    border-radius: 0;
  }

  .secret-row :global([data-slot="button"]:hover) {
    color: var(--color-neutral-active);
    background: color-mix(in srgb, var(--color-white) 14%, transparent);
  }

  .secret-row :global([data-slot="button"] .material-symbols-outlined) {
    font-size: 18px !important;
  }

  .scopes-field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .scope-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }

  .scope-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 6px 3px 8px;
    background: color-mix(in srgb, var(--color-interactive) 18%, transparent);
    color: var(--color-interactive);
  }

  .scope-chip-label {
    font-size: 12px;
    font-weight: 600;
  }

  :global(.scope-chip-remove) {
    color: var(--color-interactive) !important;
    opacity: 0.7;
    padding: 0 !important;
    width: auto !important;
    height: auto !important;
    min-width: 0 !important;
  }

  :global(.scope-chip-remove:hover) {
    opacity: 1 !important;
  }

  :global(.scope-chip-remove .material-symbols-outlined) {
    font-size: 14px !important;
  }

  .scope-add-row {
    display: flex;
    gap: 0;
    align-items: center;
  }

  :global([data-slot="input"].scope-add-input) {
    flex: 1;
    min-width: 0;
  }

  :global(.scope-add-btn) {
    color: var(--color-interactive) !important;
    background: color-mix(in srgb, var(--color-interactive) 20%, transparent) !important;
    border-radius: 0 !important;
    flex-shrink: 0;
  }

  :global(.scope-add-btn:hover:not(:disabled)) {
    background: color-mix(in srgb, var(--color-interactive) 35%, transparent) !important;
  }

  :global(.scope-add-btn .material-symbols-outlined) {
    font-size: 18px !important;
  }
</style>
