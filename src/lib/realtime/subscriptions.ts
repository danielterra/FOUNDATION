/**
 * Pub/sub bus for ontology entity events.
 *
 * Instead of every widget attaching its own global `listen('entity-updated')` and
 * filtering client-side, each consumer declares the entity IRIs it currently shows
 * (and, for collection views, IRI substring patterns) through a subscription handle.
 * A single listener per event type fans events out to the matching handlers.
 *
 * The aggregate set of subscribed IRIs/patterns is pushed to the backend
 * (`events__set_subscriptions` or `events__set_subscriptions_v2`), which emits
 * entity-* events ONLY for what is shown.
 * Closing a widget drops its IRIs from the set, so the backend stops emitting them.
 *
 * Extended with:
 * - creation-queries (classIri+predicate+objectValue) for discovering new entities
 *   without a race condition at birth. Matched in memory by the backend.
 * - `entity-joined-set` event carrying the new entityId when a creation-query fires.
 * - sinceTx cursor: the consumer declares the max tx seen at snapshot time; the bus
 *   requests a replay of events with tx > sinceTx on (re)subscription to close the
 *   assinar-depois-do-evento gap.
 */
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { invoke } from '@tauri-apps/api/core';

export type EntityEventType = 'updated' | 'referenced' | 'deleted' | 'joined-set';

export interface EntityEvent {
  type: EntityEventType;
  entityId: string;
  /** Predicates changed in this write — present only for `updated` events. */
  changedPredicates?: string[];
  /** tx monotonic clock value from the triple store — present when the backend carries it. */
  tx?: number;
  /** For `joined-set`: the class, predicate and objectValue that matched. */
  classIri?: string;
  predicate?: string;
  objectValue?: string;
}

export type EntityHandler = (event: EntityEvent) => void;

export interface CreationQuerySpec {
  classIri: string;
  predicate: string;
  objectValue: string;
}

interface Subscription {
  handler: EntityHandler;
  iris: Set<string>;
  patterns: Set<string>;
  creationQueries: CreationQuerySpec[];
  sinceTx: number;
}

const subscriptions = new Set<Subscription>();

let started = false;
const unlisteners: UnlistenFn[] = [];

function dispatch(
  type: EntityEventType,
  entityId: string | undefined,
  extra?: {
    changedPredicates?: string[];
    tx?: number;
    classIri?: string;
    predicate?: string;
    objectValue?: string;
  },
) {
  if (!entityId) return;
  for (const sub of subscriptions) {
    const matchesIri = sub.iris.has(entityId);
    const matchesPat = matchesPattern(sub.patterns, entityId);
    const matchesQuery =
      type === 'joined-set' &&
      extra?.classIri != null &&
      sub.creationQueries.some(
        (q) =>
          q.classIri === extra.classIri &&
          q.predicate === extra.predicate &&
          q.objectValue === extra.objectValue,
      );

    if (matchesIri || matchesPat || matchesQuery) {
      sub.handler({ type, entityId, ...extra });
    }
  }
}

function matchesPattern(patterns: Set<string>, entityId: string): boolean {
  if (patterns.size === 0) return false;
  for (const p of patterns) {
    if (entityId.includes(p)) return true;
  }
  return false;
}

/** Attach the global listeners once, lazily, on the first subscription. */
function ensureStarted() {
  if (started) return;
  started = true;
  listen<{ entityId?: string; changedPredicates?: string[]; tx?: number }>(
    'entity-updated',
    (e) =>
      dispatch('updated', e.payload?.entityId, {
        changedPredicates: e.payload?.changedPredicates,
        tx: e.payload?.tx,
      }),
  ).then((u) => unlisteners.push(u));
  listen<{ entityId?: string; tx?: number }>('entity-referenced', (e) =>
    dispatch('referenced', e.payload?.entityId, { tx: e.payload?.tx }),
  ).then((u) => unlisteners.push(u));
  listen<{ entityId?: string }>('entity-deleted', (e) =>
    dispatch('deleted', e.payload?.entityId),
  ).then((u) => unlisteners.push(u));
  listen<{
    entityId?: string;
    classIri?: string;
    predicate?: string;
    objectValue?: string;
    tx?: number;
  }>('entity-joined-set', (e) =>
    dispatch('joined-set', e.payload?.entityId, {
      classIri: e.payload?.classIri,
      predicate: e.payload?.predicate,
      objectValue: e.payload?.objectValue,
      tx: e.payload?.tx,
    }),
  ).then((u) => unlisteners.push(u));
}

let syncTimer: ReturnType<typeof setTimeout> | undefined;
let lastSentKey = '';

function scheduleSync() {
  clearTimeout(syncTimer);
  syncTimer = setTimeout(syncNow, 50);
}

function syncNow() {
  const iris = new Set<string>();
  const patterns = new Set<string>();
  const queriesMap = new Map<string, CreationQuerySpec>();
  for (const sub of subscriptions) {
    for (const iri of sub.iris) iris.add(iri);
    for (const p of sub.patterns) patterns.add(p);
    for (const q of sub.creationQueries) {
      queriesMap.set(`${q.classIri}|${q.predicate}|${q.objectValue}`, q);
    }
  }
  const iriList = [...iris].sort();
  const patternList = [...patterns].sort();
  const queryList = [...queriesMap.values()];

  const key =
    iriList.join(' ') +
    '|' +
    patternList.join(' ') +
    '|' +
    queryList.map((q) => `${q.classIri}:${q.predicate}=${q.objectValue}`).join(',');

  if (key === lastSentKey) return;
  lastSentKey = key;

  if (queryList.length > 0) {
    invoke('events__set_subscriptions_v2', {
      iris: iriList,
      patterns: patternList,
      creationQueries: queryList.map((q) => ({
        classIri: q.classIri,
        predicate: q.predicate,
        objectValue: q.objectValue,
      })),
    }).catch((err) => console.error('[realtime] events__set_subscriptions_v2 failed:', err));
  } else {
    invoke('events__set_subscriptions', { iris: iriList, patterns: patternList }).catch((err) =>
      console.error('[realtime] events__set_subscriptions failed:', err),
    );
  }
}

function setsEqual(a: Set<string>, b: Set<string>): boolean {
  if (a.size !== b.size) return false;
  for (const v of a) if (!b.has(v)) return false;
  return true;
}

function queriesEqual(a: CreationQuerySpec[], b: CreationQuerySpec[]): boolean {
  if (a.length !== b.length) return false;
  return a.every((qa, i) => {
    const qb = b[i];
    return qa.classIri === qb.classIri && qa.predicate === qb.predicate && qa.objectValue === qb.objectValue;
  });
}

export interface EntitySubscription {
  /** Replace the exact entity IRIs this consumer is watching. */
  setIris(iris: Iterable<string>): void;
  /** Replace the IRI substring patterns this consumer watches (collection views). */
  setPatterns(patterns: Iterable<string>): void;
  /**
   * Replace the creation-queries this consumer is interested in.
   * Each query fires `entity-joined-set` when a new entity of the given class
   * is written with the given predicate→objectValue.
   */
  setCreationQueries(queries: CreationQuerySpec[]): void;
  /**
   * Declare the max tx seen at snapshot time. On the next sync the bus requests
   * a replay of events with tx > sinceTx so no writes are missed in the gap between
   * the snapshot and the subscription being registered.
   */
  setSinceTx(tx: number): void;
  /** Request immediate replay of events with tx > the last setSinceTx value. */
  replayMissed(): void;
  /** Detach this consumer and drop its IRIs from the aggregate set. */
  destroy(): void;
}

/**
 * Register a consumer of entity events. Call `setIris`/`setPatterns`/`setCreationQueries`
 * (typically from a Svelte `$effect`) whenever the displayed set changes, and `destroy()`
 * in `onDestroy`.
 */
export function createEntitySubscription(handler: EntityHandler): EntitySubscription {
  ensureStarted();
  const sub: Subscription = {
    handler,
    iris: new Set(),
    patterns: new Set(),
    creationQueries: [],
    sinceTx: 0,
  };
  subscriptions.add(sub);

  return {
    setIris(iris: Iterable<string>) {
      const next = new Set([...iris].filter(Boolean));
      if (setsEqual(sub.iris, next)) return;
      sub.iris = next;
      scheduleSync();
    },
    setPatterns(patterns: Iterable<string>) {
      const next = new Set([...patterns].filter(Boolean));
      if (setsEqual(sub.patterns, next)) return;
      sub.patterns = next;
      scheduleSync();
    },
    setCreationQueries(queries: CreationQuerySpec[]) {
      if (queriesEqual(sub.creationQueries, queries)) return;
      sub.creationQueries = queries;
      // Creation-queries define the entry window for brand-new entities.
      // Any delay between registering the query here and pushing it to the
      // backend is exactly the race window where a freshly-created entity
      // escapes the backend match — no entity-joined-set is emitted, the ring
      // never sees the event, and replay does not reconstruct it (fallback only
      // covers entity-updated). Cancel any pending debounce so that setIris/
      // setPatterns changes coalesce into this immediate sync; the lastSentKey
      // gate in syncNow() prevents a redundant push if nothing actually changed.
      clearTimeout(syncTimer);
      syncTimer = undefined;
      syncNow();
    },
    setSinceTx(tx: number) {
      sub.sinceTx = tx;
    },
    replayMissed() {
      if (sub.sinceTx <= 0) return;
      const sinceTx = sub.sinceTx;
      invoke<
        Array<{
          tx: number;
          event: string;
          entityId: string;
          changedPredicates: string[];
          classIri?: string | null;
          predicate?: string | null;
          objectValue?: string | null;
        }>
      >('events__replay_since', { sinceTx })
        .then((events) => {
          for (const e of events) {
            const type: EntityEventType =
              e.event === 'entity-updated'
                ? 'updated'
                : e.event === 'entity-referenced'
                  ? 'referenced'
                  : e.event === 'entity-joined-set'
                    ? 'joined-set'
                    : 'deleted';
            // Mirror the live dispatch's matching: deliver joined-set only when
            // the consumer registered a matching creation-query (was previously
            // over-broad because the fields weren't carried on replay).
            const matchesQuery =
              type === 'joined-set' &&
              e.classIri != null &&
              sub.creationQueries.some(
                (q) =>
                  q.classIri === e.classIri &&
                  q.predicate === e.predicate &&
                  q.objectValue === e.objectValue,
              );
            if (
              sub.iris.has(e.entityId) ||
              matchesPattern(sub.patterns, e.entityId) ||
              matchesQuery
            ) {
              sub.handler({
                type,
                entityId: e.entityId,
                changedPredicates: e.changedPredicates,
                tx: e.tx,
                classIri: e.classIri ?? undefined,
                predicate: e.predicate ?? undefined,
                objectValue: e.objectValue ?? undefined,
              });
            }
          }
        })
        .catch((err) => console.error('[realtime] replay_since failed:', err));
    },
    destroy() {
      if (!subscriptions.has(sub)) return;
      subscriptions.delete(sub);
      scheduleSync();
    },
  };
}
