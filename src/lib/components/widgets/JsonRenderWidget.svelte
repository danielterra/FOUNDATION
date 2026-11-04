<script lang="ts">
  import { onDestroy, onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { createEntitySubscription } from '$lib/realtime/subscriptions';
  import type { Spec } from '@json-render/core';
  import { validateSpec } from '@json-render/core';
  import { JsonUIProvider, Renderer } from '@json-render/svelte';
  import WidgetContainer from './WidgetContainer.svelte';
  import { Button } from '$lib/components/ui/button';
  import { componentCatalog } from '$lib/json-render/catalog';
  import { registry } from '$lib/json-render/registry';

  type EntityData = {
    label?: string;
    snapshotTx?: number;
    properties?: Array<{ property: string; value: string }>;
  };
  type JsonActionBinding = { action?: string; params?: Record<string, unknown> };
  type ElementActionBindings = { type?: string; on?: Record<string, JsonActionBinding | Array<JsonActionBinding>> };
  type JsonRenderSpec = { root?: unknown; elements?: Record<string, unknown> };

  let { widgetId, entityId = '', conversationIri = null, windowState = 'normal', onWindowStateChange } = $props<{
    widgetId: string;
    entityId?: string;
    conversationIri?: string | null;
    windowState?: string;
    onWindowStateChange?: (state: string) => void;
  }>();

  let label = $state('Widget dinâmico');
  let spec = $state<Spec | null>(null);
  let loading = $state(true);
  let errorMessage = $state('');
  let notice = $state('');
  let snapshotTx = 0;
  let loadGeneration = 0;
  const initialState = {};
  const entitySubscription = createEntitySubscription((event) => {
    if (event.type === 'updated' && event.entityId === entityId && (event.tx == null || event.tx > snapshotTx)) {
      loadSpec();
    }
  });

  async function loadSpec() {
    const generation = ++loadGeneration;
    loading = true;
    errorMessage = '';
    try {
      const raw = await invoke<string>('inspector__get_entity', { entityId });
      if (generation !== loadGeneration) return;
      const entity = JSON.parse(raw) as EntityData;
      label = entity.label || 'Widget dinâmico';
      const source = entity.properties?.find((property) => property.property === 'foundation:specSource')?.value;
      if (!source) {
        spec = null;
        errorMessage = 'Esta interface ainda não tem uma especificação salva.';
        return;
      }
      const parsed: unknown = JSON.parse(source);
      if (!parsed || typeof parsed !== 'object') {
        spec = null;
        errorMessage = 'A especificação do widget precisa ser um objeto JSON.';
        return;
      }
      const sourceSpec = parsed as JsonRenderSpec;
      const catalogValidation = componentCatalog.validate(parsed);
      if (!catalogValidation.success) {
        spec = null;
        errorMessage = 'A especificação do widget não corresponde ao catálogo de componentes disponível.';
        return;
      }
      const structuralValidation = validateSpec(parsed as Spec);
      if (!structuralValidation.valid) {
        spec = null;
        errorMessage = 'A especificação está incompleta. Verifique o elemento principal e os componentes filhos.';
        return;
      }
      const allowedActions = new Set(['announce', 'setState', 'pushState', 'removeState', 'validateForm']);
      const elements = sourceSpec.elements as Record<string, ElementActionBindings>;
      const actionEvents: Record<string, string[]> = {
        Button: ['press'], DialogTrigger: ['press'], DialogClose: ['press'],
        PopoverTrigger: ['press'], PopoverClose: ['press'],
        Input: ['change'], Textarea: ['change'], Checkbox: ['change'],
        Select: ['change'], RadioGroup: ['change'], ToggleGroup: ['change'],
        AlertDialogTrigger: ['press'], AlertDialogAction: ['press'], AlertDialogCancel: ['press'],
        CollapsibleTrigger: ['press'],
      };
      for (const element of Object.values(elements)) {
        if (!element || typeof element !== 'object') continue;
        const componentType = (element as ElementActionBindings).type;
        const bindings = (element as ElementActionBindings).on ?? {};
        for (const [eventName, binding] of Object.entries(bindings)) {
          if (typeof componentType !== 'string' || !actionEvents[componentType]?.includes(eventName)) {
            spec = null;
            errorMessage = 'A interface solicita uma ação que não está liberada.';
            return;
          }
          for (const action of Array.isArray(binding) ? binding : [binding]) {
            if (!action.action || !allowedActions.has(action.action)) {
              spec = null;
              errorMessage = 'A interface solicita uma ação que não está liberada.';
              return;
            }
            const announceSchema = componentCatalog.data.actions.announce.params;
            if (action.action === 'announce' && !announceSchema.safeParse(action.params ?? {}).success) {
              spec = null;
              errorMessage = 'A interface contém parâmetros inválidos para uma ação.';
              return;
            }
            const params = action.params ?? {};
            if (action.action === 'setState' && (typeof params.statePath !== 'string' || !('value' in params))) {
              spec = null;
              errorMessage = 'A interface contém parâmetros inválidos para uma ação.';
              return;
            }
            if (action.action === 'pushState' && (typeof params.statePath !== 'string' || !('value' in params))) {
              spec = null;
              errorMessage = 'A interface contém parâmetros inválidos para uma ação.';
              return;
            }
            if (action.action === 'removeState' && (typeof params.statePath !== 'string' || typeof params.index !== 'number')) {
              spec = null;
              errorMessage = 'A interface contém parâmetros inválidos para uma ação.';
              return;
            }
          }
        }
      }
      spec = parsed as Spec;
      snapshotTx = entity.snapshotTx ?? 0;
      entitySubscription.setIris([entityId]);
      entitySubscription.setSinceTx(snapshotTx);
      entitySubscription.replayMissed();
    } catch {
      if (generation === loadGeneration) {
        spec = null;
        errorMessage = 'Não foi possível carregar a interface. Tente novamente.';
      }
    } finally {
      if (generation === loadGeneration) loading = false;
    }
  }

  function handleAction(actionName: string, params?: Record<string, unknown>) {
    if (actionName === 'announce') {
      notice = typeof params?.message === 'string' ? params.message : '';
    }
  }

  async function closeWidget() {
    try {
      await invoke('widget_blackboard__remove_widget', { widgetId });
    } catch (error) {
      console.error('Failed to remove JSON Render widget:', error);
    }
  }

  onMount(loadSpec);
  onDestroy(() => {
    loadGeneration += 1;
    entitySubscription.destroy();
  });
</script>

<WidgetContainer
  icon="dynamic_form"
  title={label}
  {windowState}
  {onWindowStateChange}
  onClose={closeWidget}
  {entityId}
  {conversationIri}
>
  <section class="json-render-body" aria-busy={loading}>
    {#if loading}
      <div class="state-message" role="status" aria-live="polite">
        <span class="material-symbols-outlined" aria-hidden="true">progress_activity</span>
        <span>Carregando interface…</span>
      </div>
    {:else if errorMessage}
      <div class="state-message error-state" role="alert">
        <span class="material-symbols-outlined" aria-hidden="true">warning</span>
        <p>{errorMessage}</p>
        <Button variant="outline" size="sm" onclick={loadSpec}>Tentar novamente</Button>
      </div>
    {:else if spec}
      {#if notice}
        <p class="local-notice" role="status" aria-live="polite">{notice}</p>
      {/if}
      <JsonUIProvider {registry} {initialState} handlers={{ announce: async (params) => { handleAction('announce', params); } }}>
        <Renderer {spec} {registry} />
      </JsonUIProvider>
    {:else}
      <div class="state-message">Esta interface ainda não tem conteúdo.</div>
    {/if}
  </section>
</WidgetContainer>

<style>
  .json-render-body {
    box-sizing: border-box;
    width: 100%;
    height: 100%;
    min-width: 0;
    overflow: auto;
    padding: var(--spacing-widget-inset);
    color: var(--text-body);
  }

  .state-message {
    display: flex;
    min-height: 8rem;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--spacing-field-group);
    color: var(--text-secondary);
    text-align: center;
    overflow-wrap: anywhere;
  }

  .error-state {
    color: var(--color-error);
  }

  .local-notice {
    margin: 0 0 var(--spacing-field-group);
    padding: var(--spacing-control-internal);
    border-inline-start: var(--size-hud-frame-line) solid var(--color-success);
    color: var(--text-body);
    overflow-wrap: anywhere;
  }
</style>
