import { writable } from 'svelte/store';

export type DeleteImpactItem = {
  iri: string;
  label: string;
  type_label: string;
};

export type DeleteConfirmState = {
  entityId: string;
  entityLabel: string;
  cascade_items: DeleteImpactItem[];
  backlink_count: number;
  onConfirm: () => void;
  onCancel: () => void;
};

export const deleteConfirm = writable<DeleteConfirmState | null>(null);
