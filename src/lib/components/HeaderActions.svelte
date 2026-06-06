<script lang="ts">
  import { pageHeader } from '$lib/stores/pageHeader.svelte';
  import { onDestroy } from 'svelte';
  import type { Snippet } from 'svelte';

  let { children }: { children: Snippet } = $props();

  $effect(() => {
    pageHeader.actions = children;
  });

  onDestroy(() => {
    if (pageHeader.actions === children) pageHeader.actions = null;
  });
</script>
