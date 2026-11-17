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

export type SemanticRecordsColumn = {
  id: string;
  label: string;
  valueType: 'text' | 'number' | 'currency' | 'percentage' | 'date' | 'date_time' | 'reference';
  unit?: string;
  sortable?: boolean;
  filterOperators?: Array<'equals' | 'contains' | 'gt' | 'gte' | 'lt' | 'lte'>;
};

export type SemanticRecordsCriteria = {
  filters: Array<{ columnId: string; operator: 'equals' | 'contains' | 'gt' | 'gte' | 'lt' | 'lte'; value: string }>;
  sort: { columnId: string; direction: 'asc' | 'desc' };
};

export type SemanticRecordsSource = { alias: string; label?: string; classIri?: string };
export type SemanticRecordsRow = { entityIri: string; sourceAlias: string; values: Record<string, BoundPropertyItem[]> };
export type SemanticRecordsPageState = {
  rows: SemanticRecordsRow[];
  sources: SemanticRecordsSource[];
  pageInfo: { returnedCount: number; totalCount: number; hasMore: boolean; nextCursor?: string };
  loading: boolean;
  error: string;
  criteria: SemanticRecordsCriteria;
};

export type EntityBindingContext = {
  get(propertyIri: string): BoundPropertyState | undefined;
  loadMore(propertyIri: string): void;
  retry(propertyIri: string): void;
  openEntity(entityIri: string): void;
  getRecords(elementId: string): SemanticRecordsPageState | undefined;
  loadRecords(elementId: string, criteria: SemanticRecordsCriteria, cursor?: string): void;
  retryRecords(elementId: string): void;
};

export const ENTITY_BINDING_CONTEXT = Symbol.for('foundation.semantic-entity-bindings');
