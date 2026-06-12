import { writable } from 'svelte/store';

export type SubconsciousEntity = {
  iri: string;
  label: string;
  type_iri: string;
  type_label: string;
  icon: string | null;
  score: number;
  property_hits: Array<{ prop_label: string; prop_iri: string; value: string }>;
  is_open_loop: boolean | null;
};

export type ModalState = {
  title: string;
  html?: string;
  sections?: Array<{
    label: string;
    content: string;
    isError?: boolean;
  }>;
  component?: {
    type: 'subconscious';
    props: {
      entities: SubconsciousEntity[];
      onEntityClick: (iri: string) => void;
    };
  };
};

export const modal = writable<ModalState | null>(null);
