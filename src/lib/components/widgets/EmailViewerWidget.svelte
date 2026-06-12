<script>
  import { onDestroy } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { createEntitySubscription } from '$lib/realtime/subscriptions';
  import WidgetContainer from './WidgetContainer.svelte';
  import SafeHtmlFrame from './inspector/SafeHtmlFrame.svelte';

  let {
    entityId, widgetId, windowState = 'normal',
    onWindowStateChange, conversationIri = null
  } = $props();

  let emailData = $state(null);
  let loading = $state(true);
  let error = $state(null);
  let snapshotTx = 0;

  const entitySub = createEntitySubscription((event) => {
    const { type, entityId: updatedId } = event;
    if (type === 'updated' && updatedId === entityId) {
      loadEmail();
      return;
    }
    if (type === 'deleted' && updatedId === entityId) {
      closeWidget();
    }
  });

  function extractProp(properties, iri) {
    return properties?.find(p => p.property === iri && !p.is_empty)?.value ?? null;
  }

  function extractObjProp(properties, iri) {
    const p = properties?.find(p => p.property === iri && !p.is_empty);
    if (!p) return null;
    return p.valueLabel ?? p.value ?? null;
  }

  async function loadEmail() {
    loading = true;
    error = null;
    try {
      const [resultStr, snapTx] = await Promise.all([
        invoke('inspector__get_entity', { entityId }),
        invoke('chat__get_conversation_snapshot_tx', { conversationId: entityId }).catch(() => 0),
      ]);
      const data = JSON.parse(resultStr);
      const props = data?.properties ?? [];

      const bodyHtml = extractProp(props, 'foundation:emailBodyHtml');
      const bodyText = extractProp(props, 'foundation:emailBody');
      const sentAt = extractProp(props, 'foundation:sentAt') ?? extractProp(props, 'foundation:receivedAt');

      emailData = {
        subject: extractProp(props, 'foundation:emailSubject') ?? data.label ?? entityId,
        from: extractObjProp(props, 'foundation:emailFrom'),
        to: extractObjProp(props, 'foundation:emailTo'),
        sentAt,
        bodyHtml,
        bodyText,
      };

      snapshotTx = /** @type {number} */ (snapTx);
      entitySub.setIris([entityId]);
      entitySub.setSinceTx(snapshotTx);
      entitySub.replayMissed();
    } catch (err) {
      error = `Falha ao carregar email: ${entityId}`;
      console.error('Failed to load email entity:', err);
    } finally {
      loading = false;
    }
  }

  async function closeWidget() {
    try {
      await invoke('widget_blackboard__remove_widget', { widgetId });
    } catch (err) {
      console.error('Failed to remove widget:', err);
    }
  }

  function formatDate(value) {
    if (!value) return null;
    const ms = Number(value);
    if (!isNaN(ms) && ms > 0) return new Date(ms).toLocaleString('pt-BR');
    const d = new Date(value);
    return isNaN(d.getTime()) ? value : d.toLocaleString('pt-BR');
  }

  $effect(() => {
    if (entityId) entitySub.setIris([entityId]);
  });

  $effect(() => {
    if (entityId) loadEmail();
  });

  onDestroy(() => {
    entitySub.destroy();
  });
</script>

<div class="email-viewer-wrapper">
  <WidgetContainer
    icon="mail"
    title={emailData?.subject ?? 'Email'}
    {windowState}
    {onWindowStateChange}
    onClose={closeWidget}
    {entityId}
    {conversationIri}
  >
    {#snippet children()}
      <div class="email-body">
        {#if loading}
          <div class="state-message">
            <span class="material-symbols-outlined spinning">progress_activity</span>
            <span>Carregando email…</span>
          </div>
        {:else if error}
          <div class="state-message state-error">
            <span class="material-symbols-outlined">error</span>
            <span>{error}</span>
          </div>
        {:else if !emailData}
          <div class="state-message">
            <span class="material-symbols-outlined">mail_off</span>
            <span>Email não encontrado.</span>
          </div>
        {:else}
          <div class="email-meta">
            {#if emailData.from}
              <div class="meta-row">
                <span class="meta-label">De</span>
                <span class="meta-value">{emailData.from}</span>
              </div>
            {/if}
            {#if emailData.to}
              <div class="meta-row">
                <span class="meta-label">Para</span>
                <span class="meta-value">{emailData.to}</span>
              </div>
            {/if}
            {#if emailData.sentAt}
              <div class="meta-row">
                <span class="meta-label">Data</span>
                <span class="meta-value">{formatDate(emailData.sentAt)}</span>
              </div>
            {/if}
          </div>
          <div class="email-content">
            {#if emailData.bodyHtml}
              <SafeHtmlFrame value={emailData.bodyHtml} mode="fill" />
            {:else if emailData.bodyText}
              <pre class="email-text">{emailData.bodyText}</pre>
            {:else}
              <div class="state-message">
                <span class="material-symbols-outlined">mail_off</span>
                <span>Corpo do email vazio.</span>
              </div>
            {/if}
          </div>
        {/if}
      </div>
    {/snippet}
  </WidgetContainer>
</div>

<style>
  .email-viewer-wrapper {
    display: flex;
    flex-direction: column;
    height: 100%;
  }

  .email-body {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: hidden;
  }

  .state-message {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 8px;
    padding: 32px 16px;
    color: var(--color-neutral-disabled);
    font-size: 0.9rem;
  }

  .state-message .material-symbols-outlined {
    font-size: 2rem;
  }

  .state-error {
    color: var(--color-error);
  }

  @keyframes spin {
    to { transform: rotate(360deg); }
  }

  .spinning {
    display: inline-block;
    animation: spin 1s linear infinite;
  }

  .email-meta {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 12px 16px;
    background: var(--color-surface-2);
    border-bottom: 1px solid color-mix(in srgb, var(--color-neutral) 15%, transparent);
    flex-shrink: 0;
  }

  .meta-row {
    display: flex;
    gap: 8px;
    font-size: 0.82rem;
    line-height: 1.4;
  }

  .meta-label {
    color: var(--color-neutral-disabled);
    min-width: 40px;
    flex-shrink: 0;
  }

  .meta-value {
    color: var(--color-neutral);
    word-break: break-word;
  }

  .email-content {
    flex: 1;
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .email-text {
    flex: 1;
    margin: 0;
    padding: 16px;
    overflow: auto;
    white-space: pre-wrap;
    word-break: break-word;
    font-family: inherit;
    font-size: 0.88rem;
    color: var(--color-neutral);
    background: var(--color-surface-1);
  }
</style>
