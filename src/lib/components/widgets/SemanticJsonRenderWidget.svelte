<script lang="ts">
  import { onDestroy, onMount, setContext } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { createEntitySubscription } from '$lib/realtime/subscriptions';
  import type { Spec } from '@json-render/core';
  import { JsonUIProvider, Renderer } from '@json-render/svelte';
  import WidgetContainer from './WidgetContainer.svelte';
  import { Button } from '$lib/components/ui/button';
  import { registry } from '$lib/json-render/semantic/registry';
import { buildRuntimeSemanticSpec, createSemanticLoadController, formatSemanticSpecLoadError, isSemanticRequestCurrent, partitionSemanticComponentCompatibility, resolveSemanticRefreshSnapshot, resolveSemanticWidgetSource, semanticComponentContractVersionMap, semanticPaginationAdvanced, semanticWidgetEventAction, semanticWidgetLoadIdentity, shouldContinueSemanticPagination, shouldReloadSemanticTemplate, SEMANTIC_COMPONENT_CONTRACT_VERSIONS, type SemanticComponentCompatibility } from '$lib/json-render/semantic/runtimeSpec';
  import { validateSemanticSpec } from '$lib/json-render/semantic/spec';
  import type { SemanticBindingRequest } from '$lib/json-render/semantic/runtimeSpec';
  import {
    ENTITY_BINDING_CONTEXT,
    type BoundPropertyItem,
    type BoundPropertyState,
    type EntityBindingContext,
  } from '$lib/json-render/semantic/entityBindingContext';

  type PropertyValue = {
    property: string;
    value: string;
    propertyLabel?: string;
    isObjectProperty?: boolean;
    valueEntityKind?: 'class' | 'individual' | null;
    valueLabel?: string | null;
    valueIcon?: string | null;
  };
  type EntityData = { id: string; label?: string; snapshotTx?: number; properties?: PropertyValue[] };
  type PropertyCursor = { entityIri: string; predicate: string; snapshotTx: number; valueKey: string; valueTx: number; datatype?: string | null; language?: string | null; objectType: string } | null;
  type SnapshotItem = { valueType: string; value: string; datatype?: string | null; language?: string | null; entityKind?: 'class' | 'individual' | null; label?: string | null; icon?: string | null; status?: BoundPropertyItem['valueStatus']; projectedProperties?: SnapshotPage[] | null };
  type SnapshotPage = { entityIri: string; propertyIri: string; propertyLabel: string; unitIri?: string | null; unitLabel?: string | null; items: SnapshotItem[]; hasMore: boolean; nextCursor?: PropertyCursor; snapshotTx?: number };
  type SnapshotResult = { entityIri: string; snapshotTx: number; properties: SnapshotPage[] };
  type PropertyBinding = { source: 'entity'; propertyIri: string; projections?: Record<string, string> };
  type Element = { type?: string; props?: Record<string, unknown>; bindings?: Record<string, PropertyBinding>; [key: string]: unknown };
  type RuntimeSpec = Spec & { elements: Record<string, Element> };
  type ComponentCompatibility = SemanticComponentCompatibility & {
    contractIri: string | null;
    pinnedVersion: number | null;
    targetVersion: number | null;
    reason: string;
  };
  type SemanticSpecResponse = {
    specIri: string;
    specSource: string;
    rendersEntity: string | null;
    snapshotTx: number;
    componentCompatibility: ComponentCompatibility[];
  };

  let { widgetId, entityId = '', specTemplateIri = null, conversationIri = null, windowState = 'normal', onWindowStateChange } = $props<{
    widgetId: string;
    entityId?: string;
    specTemplateIri?: string | null;
    conversationIri?: string | null;
    windowState?: string;
    onWindowStateChange?: (state: string) => void;
  }>();

  let label = $state('Interface semântica');
  let spec = $state<RuntimeSpec | null>(null);
  let loading = $state(true);
  let errorMessage = $state('');
  let notice = $state('');
  let componentCompatibility = $state<ComponentCompatibility[]>([]);
  let componentContractVersions = $state<Record<string, number | null>>({});
  let compatibility = $derived(partitionSemanticComponentCompatibility(componentCompatibility));
  let bindingStates = $state<Record<string, BoundPropertyState>>({});
  let sourceEntityIri = '';
  let sourceSnapshotTx = 0;
  let templateEntityIri = '';
  let templateSnapshotTx = 0;
  let sourceRefreshGeneration = 0;
  let propertyLabelByIri: Record<string, string> = {};
  let bindingTargets: Record<string, string[]> = {};
  let referencedEntityIris = new Set<string>();
  let activeLoadIdentity = '';
  let subscriptionReplayKey = '';
  let mounted = $state(false);
  const loadInstanceId = Math.random().toString(36).slice(2, 8);

  const specLoadController = createSemanticLoadController({
    onLoadingChange: (value) => {
      loading = value;
      console.info('[SemanticJsonRenderWidget] Render loading state changed:', {
        loadInstanceId,
        widgetId,
        entityId,
        value,
        generation: specLoadController.generation,
      });
    },
    onUnexpectedError: (error, generation) => {
      if (!specLoadController.isCurrent(generation)) return;
      spec = null;
      errorMessage = formatSemanticSpecLoadError(error);
      console.error('[SemanticJsonRenderWidget] Unhandled specification load failure:', { generation, reason: errorMessage });
    },
  });

  const entityBindingContext: EntityBindingContext = {
    get: (propertyIri) => bindingStates[propertyIri],
    loadMore: (propertyIri) => { void loadMoreProperty(propertyIri); },
    retry: (propertyIri) => { void loadProperty(propertyIri, true); },
    openEntity: (iri) => { void import('$lib/blackboard').then(({ openEntity }) => openEntity(iri, conversationIri)); },
  };
  setContext(ENTITY_BINDING_CONTEXT, entityBindingContext);
  setContext(SEMANTIC_COMPONENT_CONTRACT_VERSIONS, componentContractVersions);

  $effect(() => {
    console.info('[SemanticJsonRenderWidget] Render state observed:', {
      loadInstanceId,
      widgetId,
      entityId,
      loading,
      hasSpec: spec != null,
      errorMessage: errorMessage || null,
      identity: semanticWidgetLoadIdentity(entityId, specTemplateIri),
    });
  });

  const entitySubscription = createEntitySubscription((event) => {
    const action = semanticWidgetEventAction({
      eventType: event.type,
      eventEntityIri: event.entityId,
      templateEntityIri,
      targetEntityIri: sourceEntityIri,
      referencedEntityIris,
      changedPredicates: event.changedPredicates,
      watchedPredicates: Object.values(bindingTargets).flat(),
    });
    if (action === 'reload-template' && shouldReloadSemanticTemplate(event.tx, templateSnapshotTx)) {
      console.info('[SemanticJsonRenderWidget] Template event requested a reload:', {
        eventTx: event.tx ?? null,
        templateSnapshotTx,
        entityIri: event.entityId,
      });
      void loadSpec();
      return;
    }
    if (action === 'target-deleted') {
      errorMessage = 'A entidade ligada a esta interface foi removida.';
      spec = null;
      return;
    }
    if (action === 'refresh-target') void refreshSourceSnapshot(event.tx);
  });

  function jsonProperties(entity: EntityData) {
    const properties = entity.properties ?? [];
    return properties;
  }

  function normalizeSnapshotItems(items: SnapshotItem[]): BoundPropertyItem[] {
    return items.map((item) => ({
      isObjectProperty: item.valueType === 'iri',
      value: item.value,
      valueEntityKind: item.entityKind ?? undefined,
      valueLabel: item.label,
      valueIcon: item.icon,
      valueStatus: item.status,
      datatype: item.datatype,
      projectedProperties: item.projectedProperties?.map((page) => ({
        ...page,
        items: normalizeSnapshotItems(page.items ?? []),
      })) ?? null,
    }));
  }

  function normalizeSnapshotPage(page: SnapshotPage): BoundPropertyState {
    return {
      propertyIri: page.propertyIri,
      propertyLabel: page.propertyLabel,
      entityIri: page.entityIri,
      unitIri: page.unitIri,
      unitLabel: page.unitLabel,
      items: normalizeSnapshotItems(page.items ?? []),
      hasMore: Boolean(page.hasMore),
      cursor: page.nextCursor ?? null,
      loading: false,
      loadingMore: false,
      error: '',
      snapshotTx: page.snapshotTx ?? sourceSnapshotTx,
    };
  }

  function readProperty(entity: EntityData, iri: string) {
    return jsonProperties(entity).find((property) => property.property === iri)?.value ?? '';
  }

  function extractBindings(parsed: RuntimeSpec) {
    const requests: SemanticBindingRequest[] = [];
    for (const [elementId, element] of Object.entries(parsed.elements)) {
      for (const [target, binding] of Object.entries(element.bindings ?? {})) {
        if (binding?.source === 'entity' && binding.propertyIri) {
          requests.push({ elementId, target, propertyIri: binding.propertyIri, projections: binding.projections });
        }
      }
    }
    return requests;
  }

  function validateCompatibility(response: SemanticSpecResponse): ComponentCompatibility[] {
    if (!Array.isArray(response.componentCompatibility)) {
      throw new Error('O diagnóstico de compatibilidade dos componentes não foi retornado.');
    }
    return response.componentCompatibility.map((entry) => {
      if (!entry.componentId || !['current', 'update_available', 'update_required', 'missing_contract'].includes(entry.status)) {
        throw new Error('O diagnóstico de compatibilidade contém um estado inválido.');
      }
      return {
        ...entry,
        componentId: entry.componentId,
        reason: typeof entry.reason === 'string' ? entry.reason : '',
      };
    });
  }

  function setSubscriptionIris() {
    const iris = new Set([templateEntityIri, sourceEntityIri, ...referencedEntityIris]);
    const sortedIris = [...iris].filter(Boolean).sort();
    const snapshots = [templateSnapshotTx, sourceSnapshotTx].filter((tx) => tx > 0);
    const sinceTx = snapshots.length ? Math.min(...snapshots) : 0;
    const replayKey = JSON.stringify([sortedIris, sinceTx]);
    if (replayKey === subscriptionReplayKey) return;
    subscriptionReplayKey = replayKey;
    entitySubscription.setIris(sortedIris);
    entitySubscription.setSinceTx(sinceTx);
    console.debug('[SemanticJsonRenderWidget] Replaying entity events for subscription snapshot:', { iris: sortedIris, sinceTx });
    void entitySubscription.synchronize()
      .then(() => entitySubscription.replayMissed())
      .catch((error) => console.error('[SemanticJsonRenderWidget] Could not synchronize entity subscription:', error));
  }

  async function loadProperty(
    propertyIri: string,
    retry = false,
    generation = specLoadController.generation,
    refreshGeneration = sourceRefreshGeneration,
  ) {
    if (!sourceEntityIri || !propertyIri) return;
    const previous = bindingStates[propertyIri];
    bindingStates[propertyIri] = {
      propertyIri,
      propertyLabel: propertyLabelByIri[propertyIri] ?? propertyIri,
      items: retry ? previous?.items ?? [] : [],
      hasMore: retry ? previous?.hasMore ?? false : false,
      cursor: retry ? previous?.cursor ?? null : null,
      loading: !retry || !previous?.items.length,
      loadingMore: false,
      error: '',
      snapshotTx: sourceSnapshotTx,
    };
    try {
      const bindingRequest = extractBindings(spec as RuntimeSpec).find((request) => request.propertyIri === propertyIri);
      const projectionPropertyIris = bindingRequest?.projections ? Object.values(bindingRequest.projections) : undefined;
      const raw = await invoke<string>('widget_inspector__get_entity_property_snapshot', {
        entityIri: sourceEntityIri,
        propertyIris: [propertyIri],
        propertyProjections: projectionPropertyIris?.length ? { [propertyIri]: projectionPropertyIris } : undefined,
        snapshotTx: sourceSnapshotTx,
        pageSize: 20,
      });
      const snapshot = JSON.parse(raw) as SnapshotResult;
      const page = snapshot.properties?.[0];
      if (!page) throw new Error('A propriedade ligada não foi retornada no snapshot.');
      const normalized = normalizeSnapshotPage(page);
      if (!isSemanticRequestCurrent(generation, specLoadController.generation, refreshGeneration, sourceRefreshGeneration)) return;
      const current = bindingStates[propertyIri];
      bindingStates[propertyIri] = {
        ...current,
        ...normalized,
        propertyLabel: normalized.propertyLabel || current.propertyLabel,
        loading: false,
        loadingMore: false,
        snapshotTx: snapshot.snapshotTx,
      };
      syncReferenceSubscriptions();
    } catch (error) {
      if (!isSemanticRequestCurrent(generation, specLoadController.generation, refreshGeneration, sourceRefreshGeneration)) return;
      const current = bindingStates[propertyIri];
      bindingStates[propertyIri] = {
        ...current,
        loading: false,
        loadingMore: false,
        error: error instanceof Error ? error.message : 'Não foi possível carregar este dado.',
      };
    }
  }

  async function loadMoreProperty(propertyIri: string, generation = specLoadController.generation, refreshGeneration = sourceRefreshGeneration) {
    const current = bindingStates[propertyIri];
    if (!current?.hasMore || !current.cursor || current.loadingMore) return;
    bindingStates[propertyIri] = { ...current, loadingMore: true, error: '' };
    try {
      const bindingRequest = extractBindings(spec as RuntimeSpec).find((request) => request.propertyIri === propertyIri);
      const projectionPropertyIris = bindingRequest?.projections ? Object.values(bindingRequest.projections) : undefined;
      const raw = await invoke<string>('widget_inspector__get_entity_property_page', {
        entityIri: sourceEntityIri,
        propertyIri,
        projectionPropertyIris,
        snapshotTx: current.snapshotTx,
        cursor: current.cursor,
        pageSize: 20,
      });
      const page = JSON.parse(raw) as SnapshotPage;
      if (!isSemanticRequestCurrent(generation, specLoadController.generation, refreshGeneration, sourceRefreshGeneration)) return;
      const normalized = normalizeSnapshotPage(page);
      bindingStates[propertyIri] = {
        ...bindingStates[propertyIri],
        items: [...current.items, ...normalized.items],
        hasMore: normalized.hasMore,
        cursor: page.nextCursor ?? null,
        loadingMore: false,
      };
      syncReferenceSubscriptions();
    } catch (error) {
      if (!isSemanticRequestCurrent(generation, specLoadController.generation, refreshGeneration, sourceRefreshGeneration)) return;
      bindingStates[propertyIri] = {
        ...bindingStates[propertyIri],
        loadingMore: false,
        error: error instanceof Error ? error.message : 'Não foi possível carregar mais valores.',
      };
    }
  }

  async function refreshBindings(generation = specLoadController.generation, refreshGeneration: number) {
    const propertyIris = [...new Set(Object.values(bindingTargets).flat())];
    const visibleValueCounts = Object.fromEntries(propertyIris.map((propertyIri) => [propertyIri, bindingStates[propertyIri]?.items.length ?? 0]));
    await Promise.all(propertyIris.map(async (propertyIri) => {
      await loadProperty(propertyIri, true, generation, refreshGeneration);
      if (!isSemanticRequestCurrent(generation, specLoadController.generation, refreshGeneration, sourceRefreshGeneration)) return;
      let current = bindingStates[propertyIri];
      while (current && shouldContinueSemanticPagination({
        loadedCount: current.items.length,
        targetCount: visibleValueCounts[propertyIri],
        hasMore: current.hasMore,
        error: current.error,
      })) {
        const previousPage = { items: current.items, cursor: current.cursor };
        await loadMoreProperty(propertyIri, generation, refreshGeneration);
        if (!isSemanticRequestCurrent(generation, specLoadController.generation, refreshGeneration, sourceRefreshGeneration)) return;
        const next = bindingStates[propertyIri];
        if (!semanticPaginationAdvanced(previousPage, next)) return;
        current = next;
      }
    }));
  }

  async function refreshSourceSnapshot(eventTx?: number) {
    if (!sourceEntityIri) return;
    const snapshot = resolveSemanticRefreshSnapshot(sourceSnapshotTx, eventTx);
    if (snapshot.kind === 'ignore') return;
    const generation = specLoadController.generation;
    const refreshGeneration = ++sourceRefreshGeneration;
    try {
      if (snapshot.kind === 'event') {
        sourceSnapshotTx = snapshot.snapshotTx;
      } else {
        const raw = await invoke<string>('inspector__get_entity', { entityId: sourceEntityIri });
        if (generation !== specLoadController.generation || refreshGeneration !== sourceRefreshGeneration) return;
        const entity = JSON.parse(raw) as EntityData;
        sourceSnapshotTx = entity.snapshotTx ?? sourceSnapshotTx;
        propertyLabelByIri = Object.fromEntries(jsonProperties(entity).map((property) => [property.property, property.propertyLabel ?? property.property]));
      }
      await refreshBindings(generation, refreshGeneration);
      if (generation !== specLoadController.generation || refreshGeneration !== sourceRefreshGeneration) return;
      setSubscriptionIris();
    } catch {
      if (generation !== specLoadController.generation || refreshGeneration !== sourceRefreshGeneration) return;
      errorMessage = 'Não foi possível atualizar os dados ligados à entidade.';
    }
  }

  function syncReferenceSubscriptions() {
    referencedEntityIris = new Set(Object.values(bindingStates).flatMap((state) => [
      ...state.items.filter((item) => item.isObjectProperty).map((item) => item.value),
      ...state.items.flatMap((item) => (item.projectedProperties ?? []).flatMap((page) => page.items.filter((child) => child.isObjectProperty).map((child) => child.value))),
    ]));
    setSubscriptionIris();
  }

  function loadSpec(supersede = false) {
    return specLoadController.request(async (generation) => {
      const reason = supersede ? 'new-identity-or-initial-mount' : 'template-event-or-retry';
      try {
        await performSpecLoad(generation, reason);
      } finally {
        console.info('[SemanticJsonRenderWidget] Specification load settled:', {
          loadInstanceId,
          widgetId,
          entityId,
          specTemplateIri,
          generation,
          current: specLoadController.isCurrent(generation),
          result: errorMessage ? 'error' : spec ? 'content' : 'empty',
          errorMessage: errorMessage || null,
        });
      }
    }, supersede);
  }

  async function readLoadData<T>(generation: number, command: string, args: Record<string, unknown>, purpose: string) {
    console.debug('[SemanticJsonRenderWidget] Read started:', { generation, command, purpose, args });
    try {
      const result = await invoke<string>(command, args);
      console.debug('[SemanticJsonRenderWidget] Read completed:', { generation, command, purpose });
      return JSON.parse(result) as T;
    } catch (error) {
      console.error('[SemanticJsonRenderWidget] Read failed:', {
        generation,
        command,
        purpose,
        error: formatSemanticSpecLoadError(error),
      });
      throw error;
    }
  }

  async function performSpecLoad(generation: number, reason: string) {
    console.info('[SemanticJsonRenderWidget] Specification load started:', { loadInstanceId, widgetId, generation, reason, entityId, specTemplateIri });
    errorMessage = '';
    notice = '';
    componentCompatibility = [];
    for (const componentId of Object.keys(componentContractVersions)) delete componentContractVersions[componentId];
    try {
      const initialSource = resolveSemanticWidgetSource({ entityId, specTemplateIri });
      templateEntityIri = initialSource.templateEntityIri;
      const response = await readLoadData<SemanticSpecResponse>(generation, 'widget_blackboard__get_semantic_spec', { specIri: templateEntityIri }, 'semantic-spec');
      if (!specLoadController.isCurrent(generation)) {
        console.info('[SemanticJsonRenderWidget] Stale specification result ignored:', { generation, currentGeneration: specLoadController.generation });
        return;
      }
      if (response.specIri !== templateEntityIri || typeof response.specSource !== 'string') {
        throw new Error('A resposta não corresponde à especificação solicitada.');
      }
      templateSnapshotTx = response.snapshotTx ?? 0;
      componentCompatibility = validateCompatibility(response);
      const declaredTargetIri = response.rendersEntity ?? '';
      const resolvedSource = resolveSemanticWidgetSource({ entityId, specTemplateIri, templateSourceEntityIri: declaredTargetIri });
      const targetEntityIri = resolvedSource.targetEntityIri;
      const templateEntity = await readLoadData<EntityData>(generation, 'inspector__get_entity', { entityId: templateEntityIri }, 'template-entity');
      if (!specLoadController.isCurrent(generation)) {
        console.info('[SemanticJsonRenderWidget] Stale template entity result ignored:', { generation, currentGeneration: specLoadController.generation });
        return;
      }
      const targetEntity = targetEntityIri && targetEntityIri !== templateEntityIri
        ? await readLoadData<EntityData>(generation, 'inspector__get_entity', { entityId: targetEntityIri }, 'target-entity')
        : templateEntity;
      if (!specLoadController.isCurrent(generation)) {
        console.info('[SemanticJsonRenderWidget] Stale target entity result ignored:', { generation, currentGeneration: specLoadController.generation });
        return;
      }
      label = (specTemplateIri ? targetEntity.label : templateEntity.label) || 'Interface semântica';
      sourceSnapshotTx = targetEntity.snapshotTx ?? 0;
      const source = response.specSource;
      if (!source) {
        spec = null;
        errorMessage = 'Esta interface ainda não tem uma especificação semântica salva.';
        return;
      }

      let parsed: unknown;
      try { parsed = JSON.parse(source); }
      catch {
        spec = null;
        errorMessage = 'A especificação salva não contém JSON válido.';
        return;
      }
      const validation = validateSemanticSpec(parsed);
      if (validation.success === false) {
        console.error('[SemanticJsonRenderWidget] Invalid semantic specification:', {
          specIri: response.specIri,
          reason: validation.message,
        });
        spec = null;
        errorMessage = validation.message;
        return;
      }

      const parsedSpec = validation.spec as RuntimeSpec;
      const reportedComponents = new Set(componentCompatibility.map((contract) => contract.componentId));
      const missingDiagnostics = [...new Set(Object.values(parsedSpec.elements).map((element) => String(element.type ?? '')))]
        .filter((componentId) => componentId && !reportedComponents.has(componentId))
        .map((componentId): ComponentCompatibility => ({
          componentId,
          contractIri: null,
          pinnedVersion: null,
          targetVersion: null,
          status: 'missing_contract',
          reason: 'O serviço não retornou diagnóstico para este componente.',
        }));
      if (missingDiagnostics.length) componentCompatibility = [...componentCompatibility, ...missingDiagnostics];
      Object.assign(componentContractVersions, semanticComponentContractVersionMap(componentCompatibility));
      const blockedContracts = partitionSemanticComponentCompatibility(componentCompatibility).blocked;
      if (blockedContracts.length) {
        spec = null;
        errorMessage = 'Este widget precisa ser atualizado antes de renderizar com segurança.';
        return;
      }
      const requests = extractBindings(parsedSpec);
      if (requests.length && !targetEntityIri) {
        spec = null;
        errorMessage = 'Esta interface declara vínculos de dados, mas não tem uma entidade-alvo para lê-los.';
        return;
      }
      sourceEntityIri = targetEntityIri;
      propertyLabelByIri = Object.fromEntries(jsonProperties(targetEntity).map((property) => [property.property, property.propertyLabel ?? property.property]));
      bindingStates = {};
      for (const propertyIri of new Set(requests.map((request) => request.propertyIri))) {
        bindingStates[propertyIri] = {
          propertyIri,
          propertyLabel: propertyLabelByIri[propertyIri] ?? propertyIri,
          items: [], hasMore: false, cursor: null, loading: true, loadingMore: false, error: '', snapshotTx: sourceSnapshotTx,
        };
      }
      const runtime = buildRuntimeSemanticSpec(parsedSpec, requests);
      bindingTargets = runtime.bindingTargets;
      spec = runtime.spec;
      setSubscriptionIris();
      void Promise.all([
        ...[...new Set(requests.map((request) => request.propertyIri))].map((propertyIri) => loadProperty(propertyIri)),
      ]).then(async () => {
        if (!specLoadController.isCurrent(generation)) return;
        await entitySubscription.synchronize();
        if (!specLoadController.isCurrent(generation)) return;
        entitySubscription.replayMissed();
        await refreshSourceSnapshot();
      }).catch(() => {
        if (specLoadController.isCurrent(generation)) errorMessage = 'Não foi possível concluir o carregamento dos dados desta interface.';
      });
    } catch (error) {
      if (specLoadController.isCurrent(generation)) {
        spec = null;
        errorMessage = formatSemanticSpecLoadError(error);
        console.error('[SemanticJsonRenderWidget] Failed to load specification:', {
          generation,
          specIri: templateEntityIri,
          reason: errorMessage,
        });
      } else {
        console.info('[SemanticJsonRenderWidget] Stale read failure ignored:', { generation, currentGeneration: specLoadController.generation });
      }
    }
  }

  function handleAnnounce(params?: Record<string, unknown>) {
    notice = typeof params?.message === 'string' ? params.message : '';
  }

  async function closeWidget() {
    try { await invoke('widget_blackboard__remove_widget', { widgetId }); }
    catch (error) { console.error('Failed to remove semantic JSON Render widget:', error); }
  }

  onMount(() => {
    mounted = true;
    activeLoadIdentity = semanticWidgetLoadIdentity(entityId, specTemplateIri);
    void loadSpec(true);
  });

  $effect(() => {
    const identity = semanticWidgetLoadIdentity(entityId, specTemplateIri);
    if (!mounted || !entityId || identity === activeLoadIdentity) return;
    activeLoadIdentity = identity;
    loading = true;
    errorMessage = '';
    spec = null;
    sourceEntityIri = '';
    templateEntityIri = '';
    sourceSnapshotTx = 0;
    templateSnapshotTx = 0;
    propertyLabelByIri = {};
    bindingTargets = {};
    bindingStates = {};
    referencedEntityIris = new Set();
    setSubscriptionIris();
    void loadSpec(true);
  });

  onDestroy(() => {
    specLoadController.dispose();
    entitySubscription.destroy();
  });
</script>

<WidgetContainer icon="schema" title={label} {windowState} {onWindowStateChange} onClose={closeWidget} {entityId} {conversationIri}>
  <section class="semantic-render-body" aria-busy={loading}>
    {#if loading}
      <div class="state-message" role="status" aria-live="polite"><span class="material-symbols-outlined" aria-hidden="true">progress_activity</span><span>Carregando interface…</span></div>
      {:else if compatibility.blocked.length > 0}
        <aside class="contract-warning" role="alert" aria-live="assertive">
          <span class="material-symbols-outlined" aria-hidden="true">warning</span>
          <div><strong>Widget incompatível</strong><p>{errorMessage || 'Este widget precisa ser atualizado antes de renderizar com segurança.'}</p><p>Peça ao agente para atualizar a especificação deste widget para os contratos atuais.</p>
            <ul>{#each compatibility.blocked as contract (`${contract.componentId}:${contract.contractIri ?? 'missing'}`)}
              <li><code>{contract.componentId}</code> · {contract.status === 'missing_contract' ? 'contrato ausente' : `v${contract.pinnedVersion ?? '?'} → v${contract.targetVersion ?? '?'}`}<span>{contract.reason}</span></li>
            {/each}</ul>
            <Button variant="outline" size="sm" onclick={() => loadSpec()}>Recarregar diagnóstico</Button>
          </div>
        </aside>
      {:else if errorMessage}
      <div class="state-message error-state" role="alert"><span class="material-symbols-outlined" aria-hidden="true">warning</span><p>{errorMessage}</p><Button variant="outline" size="sm" onclick={() => loadSpec()}>Tentar novamente</Button></div>
    {:else if spec}
      {#if compatibility.updatesAvailable.length > 0}
        <aside class="contract-update-note" role="status" aria-live="polite">
          <span class="material-symbols-outlined" aria-hidden="true">update</span>
          <div><strong>Atualização disponível</strong><p>Este widget continua compatível; uma atualização pode aproveitar contratos mais recentes.</p>
            <ul>{#each compatibility.updatesAvailable as contract (`${contract.componentId}:${contract.contractIri ?? ''}`)}
              <li><code>{contract.componentId}</code> · v{contract.pinnedVersion ?? '?'} → v{contract.targetVersion ?? '?'}{#if contract.reason}<span>{contract.reason}</span>{/if}</li>
            {/each}</ul>
          </div>
        </aside>
      {/if}
      {#if notice}<p class="local-notice" role="status" aria-live="polite">{notice}</p>{/if}
      <JsonUIProvider {registry} initialState={{}} handlers={{ announce: async (params) => handleAnnounce(params) }}>
        <Renderer {spec} {registry} />
      </JsonUIProvider>
    {:else}
      <div class="state-message">Esta interface ainda não tem conteúdo.</div>
    {/if}
  </section>
</WidgetContainer>

<style>
  .semantic-render-body { box-sizing: border-box; width: 100%; height: auto; min-width: 0; overflow: visible; padding: var(--spacing-widget-inset); color: var(--text-body); }
  .state-message { display: flex; min-height: 8rem; flex-direction: column; align-items: center; justify-content: center; gap: var(--spacing-field-group); color: var(--text-secondary); text-align: center; overflow-wrap: anywhere; }
  .error-state { color: var(--color-error); }
  .local-notice { margin: 0 0 var(--spacing-field-group); padding: var(--spacing-control-internal); border-inline-start: var(--size-hud-frame-line) solid var(--color-success); color: var(--text-body); overflow-wrap: anywhere; }
  .contract-warning { display: grid; grid-template-columns: auto minmax(0, 1fr); gap: var(--spacing-control-internal); margin-block-end: var(--spacing-section); padding: var(--spacing-control-internal); border-inline-start: var(--size-hud-frame-line) solid var(--color-warning); background: color-mix(in srgb, var(--color-warning) 9%, var(--color-surface-2)); color: var(--text-primary); }
  .contract-warning > .material-symbols-outlined { color: var(--color-warning); }
  .contract-update-note { display: grid; grid-template-columns: auto minmax(0, 1fr); gap: var(--spacing-control-internal); margin-block-end: var(--spacing-section); padding: var(--spacing-control-internal); border-inline-start: var(--size-hud-frame-line) solid var(--color-interactive); background: color-mix(in srgb, var(--color-interactive) 9%, var(--color-surface-2)); color: var(--text-primary); }
  .contract-update-note > .material-symbols-outlined { color: var(--color-interactive); }
  .contract-warning p { margin: var(--spacing-label-to-value) 0; color: var(--text-body); }
  .contract-warning ul { margin: 0; padding-inline-start: var(--spacing-section); color: var(--text-body); overflow-wrap: anywhere; }
  .contract-update-note p { margin: var(--spacing-label-to-value) 0; color: var(--text-body); }
  .contract-update-note ul { margin: 0; padding-inline-start: var(--spacing-section); color: var(--text-body); overflow-wrap: anywhere; }
  .contract-warning li span, .contract-update-note li span { display: block; margin-block-start: var(--spacing-label-to-value); color: var(--text-body); }
  .contract-warning code, .contract-update-note code { overflow-wrap: anywhere; }
</style>
