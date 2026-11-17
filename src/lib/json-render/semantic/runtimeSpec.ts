import type { Spec } from '@json-render/core';
import { matchesChangedPredicates } from '../../realtime/entityPredicates';

export type SemanticBindingRequest = {
  elementId: string;
  target: string;
  propertyIri: string;
  projections?: Record<string, string>;
};

export type SemanticRecordsSourceBinding = {
  source: 'entity';
  propertyIri: string;
  alias: string;
  label?: string;
  fields: Record<string, string>;
  membershipWatch?: {
    classIri: string;
    predicate: string;
    objectBinding: { source: 'entity'; propertyIri: string };
  };
};

export type SemanticRecordsBinding = { sources: SemanticRecordsSourceBinding[] };

type RuntimeElement = { props?: Record<string, unknown>; [key: string]: unknown };
export type RuntimeSemanticSpec = Spec & { elements: Record<string, RuntimeElement> };

export type SemanticWidgetSource = {
  entityId: string;
  specTemplateIri?: string | null;
  templateSourceEntityIri?: string | null;
};

export function semanticWidgetLoadIdentity(entityId: string, specTemplateIri?: string | null) {
  return JSON.stringify([specTemplateIri ?? '', entityId]);
}

export function formatSemanticSpecLoadError(error: unknown) {
  const message = (error instanceof Error ? error.message : String(error))
    .replace(/[\u0000-\u001f\u007f]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();

  try {
    const structuredError = JSON.parse(message) as { code?: unknown };
    if (structuredError.code === 'invalid_spec_structure') {
      return 'Não foi possível mostrar este widget porque a definição está incompleta. Peça ao agente para revisá-la.';
    }
  } catch {
    // Tauri may reject with a plain-text error instead of a structured payload.
  }

  if (/^JSON render spec \S+ has invalid (?:specSource|component references)/.test(message)) {
    return 'Não foi possível mostrar este widget porque a definição está incompleta. Peça ao agente para revisá-la.';
  }

  return message;
}

export type SemanticRefreshSnapshot =
  | { kind: 'ignore' }
  | { kind: 'event'; snapshotTx: number }
  | { kind: 'reload' };

export type SemanticComponentCompatibility = {
  componentId: string;
  contractIri?: string | null;
  pinnedVersion?: number | null;
  targetVersion?: number | null;
  status: 'current' | 'update_available' | 'update_required' | 'missing_contract';
  reason?: string;
};

export const SEMANTIC_COMPONENT_CONTRACT_VERSIONS = Symbol('semantic-component-contract-versions');

export function semanticComponentContractVersionMap(entries: SemanticComponentCompatibility[]) {
  return Object.fromEntries(entries.map(({ componentId, pinnedVersion }) => [componentId, pinnedVersion ?? null]));
}

export function informationPrimaryChildSpans(contractVersion: number | null | undefined) {
  return contractVersion == null || ![0, 6, 7].includes(contractVersion);
}

export function informationSummaryVisible(contractVersion: number | null | undefined, summary: unknown, title: unknown) {
  if (typeof summary !== 'string' || !summary.trim()) return false;
  if (contractVersion !== 0 && contractVersion !== 7) return true;
  const normalize = (value: unknown) => String(value ?? '').trim().replace(/\s+/g, ' ').toLocaleLowerCase();
  return normalize(summary) !== normalize(title);
}

export function informationFactLabelVisible(contractVersion: number | null | undefined, itemCount: number) {
  return (contractVersion !== 0 && contractVersion !== 7) || itemCount !== 1;
}

export function partitionSemanticComponentCompatibility(entries: SemanticComponentCompatibility[]) {
  return {
    blocked: entries.filter((entry) => entry.status === 'update_required' || entry.status === 'missing_contract'),
    updatesAvailable: entries.filter((entry) => entry.status === 'update_available'),
  };
}

export function resolveSemanticRefreshSnapshot(currentSnapshotTx: number, eventTx?: number): SemanticRefreshSnapshot {
  if (eventTx == null || !Number.isFinite(eventTx) || eventTx <= 0) return { kind: 'reload' };
  if (eventTx <= currentSnapshotTx) return { kind: 'ignore' };
  return { kind: 'event', snapshotTx: eventTx };
}

export function isSemanticRequestCurrent(
  requestLoadGeneration: number,
  currentLoadGeneration: number,
  requestRefreshGeneration: number,
  currentRefreshGeneration: number,
) {
  return requestLoadGeneration === currentLoadGeneration && requestRefreshGeneration === currentRefreshGeneration;
}

export type SemanticLoadControllerOptions = {
  onLoadingChange: (loading: boolean) => void;
  onUnexpectedError: (error: unknown, generation: number) => void;
};

export function createSemanticLoadController({ onLoadingChange, onUnexpectedError }: SemanticLoadControllerOptions) {
  let generation = 0;
  let activeToken: object | null = null;
  let activePromise: Promise<void> | null = null;
  let reloadQueued = false;
  let queuedRun: ((requestGeneration: number) => Promise<void>) | null = null;
  let disposed = false;

  function request(run: (requestGeneration: number) => Promise<void>, supersede = false): Promise<void> {
    if (disposed) return Promise.resolve();
    if (activePromise && !supersede) {
      reloadQueued = true;
      queuedRun = run;
      return activePromise;
    }

    const token = {};
    activeToken = token;
    reloadQueued = false;
    queuedRun = null;
    let requestGeneration = ++generation;
    let currentRun = run;
    onLoadingChange(true);

    const promise = (async () => {
      while (!disposed && activeToken === token) {
        try {
          await currentRun(requestGeneration);
        } catch (error) {
          if (!disposed && activeToken === token && requestGeneration === generation) {
            onUnexpectedError(error, requestGeneration);
          }
        }

        if (disposed || activeToken !== token) return;
        if (reloadQueued) {
          reloadQueued = false;
          currentRun = queuedRun ?? currentRun;
          queuedRun = null;
          requestGeneration = ++generation;
          continue;
        }

        activeToken = null;
        activePromise = null;
        onLoadingChange(false);
        return;
      }
    })();
    activePromise = promise;
    return promise;
  }

  return {
    request,
    isCurrent: (requestGeneration: number) => !disposed && requestGeneration === generation,
    invalidate() {
      if (disposed) return generation;
      reloadQueued = false;
      queuedRun = null;
      activeToken = null;
      activePromise = null;
      generation += 1;
      return generation;
    },
    dispose() {
      disposed = true;
      reloadQueued = false;
      queuedRun = null;
      activeToken = null;
      activePromise = null;
      generation += 1;
    },
    get generation() { return generation; },
  };
}

export function shouldReloadSemanticTemplate(eventTx: number | undefined, templateSnapshotTx: number) {
  return eventTx == null || eventTx > templateSnapshotTx;
}

export function shouldContinueSemanticPagination({
  loadedCount,
  targetCount,
  hasMore,
  error,
}: {
  loadedCount: number;
  targetCount: number;
  hasMore: boolean;
  error: string;
}) {
  return loadedCount < targetCount
    && hasMore
    && !error;
}

export function semanticPaginationAdvanced(previous: { items: unknown[]; cursor: unknown }, current: { items: unknown[]; cursor: unknown } | undefined) {
  return Boolean(current)
    && current.items.length > previous.items.length
    && JSON.stringify(current.cursor) !== JSON.stringify(previous.cursor);
}

export function resolveSemanticWidgetSource({ entityId, specTemplateIri, templateSourceEntityIri }: SemanticWidgetSource) {
  if (specTemplateIri) {
    return { templateEntityIri: specTemplateIri, targetEntityIri: entityId };
  }

  return { templateEntityIri: entityId, targetEntityIri: templateSourceEntityIri ?? '' };
}

export function semanticWidgetEventAction({
  eventType,
  eventEntityIri,
  templateEntityIri,
  targetEntityIri,
  referencedEntityIris,
  changedPredicates,
  watchedPredicates,
}: {
  eventType: 'updated' | 'deleted' | 'referenced' | 'joined-set';
  eventEntityIri: string;
  templateEntityIri: string;
  targetEntityIri: string;
  referencedEntityIris: Iterable<string>;
  changedPredicates?: string[];
  watchedPredicates: string[];
}) {
  if (eventType === 'updated' && eventEntityIri === templateEntityIri) return 'reload-template';
  if (eventType === 'deleted' && eventEntityIri === targetEntityIri) return 'target-deleted';
  if (eventType === 'deleted' && [...referencedEntityIris].includes(eventEntityIri)) return 'refresh-target';
  if (eventType === 'updated' && eventEntityIri === targetEntityIri && matchesChangedPredicates(changedPredicates, watchedPredicates)) return 'refresh-target';
  if (eventType === 'updated' && [...referencedEntityIris].includes(eventEntityIri)) return 'refresh-target';
  return 'ignore';
}

export function buildRuntimeSemanticSpec(source: RuntimeSemanticSpec, requests: SemanticBindingRequest[]) {
  const spec = JSON.parse(JSON.stringify(source)) as RuntimeSemanticSpec;
  const bindingTargets: Record<string, string[]> = {};

  for (const request of requests) {
    const element = spec.elements[request.elementId];
    if (!element) continue;
    element.props ??= {};
    const bindings = (element.props.__ontologyBindings as Record<string, string> | undefined) ?? {};
    bindings[request.target] = request.propertyIri;
    element.props.__ontologyBindings = bindings;
    if (request.projections) {
      const projectionBindings = (element.props.__ontologyProjectionBindings as Record<string, Record<string, string>> | undefined) ?? {};
      projectionBindings[request.target] = request.projections;
      element.props.__ontologyProjectionBindings = projectionBindings;
    }
    bindingTargets[request.target] = [...(bindingTargets[request.target] ?? []), request.propertyIri];
  }

  for (const [elementId, element] of Object.entries(spec.elements)) {
    if (element.type === 'Records') {
      element.props ??= {};
      element.props.__semanticElementId = elementId;
    }
    const recordsBinding = (element.bindings as Record<string, unknown> | undefined)?.records as SemanticRecordsBinding | undefined;
    if (!recordsBinding) continue;
    element.props ??= {};
    element.props.__ontologyRecordsBinding = recordsBinding;
    const watchedIris = recordsBinding.sources.flatMap((recordSource) => [
      recordSource.propertyIri,
      ...Object.values(recordSource.fields ?? {}),
      ...(recordSource.membershipWatch ? [recordSource.membershipWatch.objectBinding.propertyIri] : []),
    ]);
    bindingTargets.records = [...new Set([...(bindingTargets.records ?? []), ...watchedIris])];
  }

  return { spec, bindingTargets };
}
