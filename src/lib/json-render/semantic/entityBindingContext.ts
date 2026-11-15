export type BoundPropertyItem = {
  isObjectProperty: boolean;
  value: string;
  valueEntityKind?: 'class' | 'individual';
  valueLabel?: string | null;
  valueIcon?: string | null;
  valueStatus?: { iri: string; label: string; color?: string | null; icon?: string | null } | null;
  datatype?: string | null;
  projectedProperties?: BoundPropertyProjectionPage[] | null;
};

export type BoundPropertyProjectionPage = {
  entityIri: string;
  propertyIri: string;
  propertyLabel: string;
  unitIri?: string | null;
  unitLabel?: string | null;
  items: BoundPropertyItem[];
  hasMore: boolean;
  nextCursor?: unknown;
  snapshotTx?: number;
};

export type BoundPropertyState = {
  propertyIri: string;
  propertyLabel: string;
  entityIri?: string;
  unitIri?: string | null;
  unitLabel?: string | null;
  items: BoundPropertyItem[];
  hasMore: boolean;
  cursor: unknown;
  loading: boolean;
  loadingMore: boolean;
  error: string;
  snapshotTx: number;
};

export type EntityBindingContext = {
  get(propertyIri: string): BoundPropertyState | undefined;
  loadMore(propertyIri: string): void;
  retry(propertyIri: string): void;
  openEntity(entityIri: string): void;
};

export const ENTITY_BINDING_CONTEXT = Symbol.for('foundation.semantic-entity-bindings');
