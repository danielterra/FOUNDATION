import { writable } from 'svelte/store';

export type ModalState = {
  title: string;
  html?: string;
  sections?: Array<{
    label: string;
    content: string;
    isError?: boolean;
  }>;
};

export const modal = writable<ModalState | null>(null);
