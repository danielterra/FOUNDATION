import type { Snippet } from 'svelte';

export const pageHeader: { actions: Snippet | null } = $state({
  actions: null,
});
