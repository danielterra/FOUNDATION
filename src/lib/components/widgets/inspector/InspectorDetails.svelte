<script lang="ts">
  import type { Snippet } from 'svelte';
  import { validateSemanticSpec } from '$lib/json-render/semantic/spec';

  let { title, summary, details, expanded, active = true, children }: {
    title: string;
    summary?: string;
    details: Array<{ label: string; value: string }>;
    expanded: boolean;
    active?: boolean;
    children: Snippet;
  } = $props();

  const specResult = $derived(validateSemanticSpec({
    root: 'inspector-details',
    elements: {
      'inspector-details': {
        type: 'Details',
        children: [],
        props: { title, summary, details, initiallyExpanded: expanded },
      },
    },
  }));
</script>

{#if !active}
  {@render children()}
{:else if specResult.success}
  <section class="inspector-details" aria-label={title} data-summary={summary} data-expanded={expanded}>
    {@render children()}
  </section>
{:else}
  <div role="alert">Não foi possível preparar os detalhes desta propriedade.</div>
{/if}

<style>
  .inspector-details { min-width: 0; }
</style>
