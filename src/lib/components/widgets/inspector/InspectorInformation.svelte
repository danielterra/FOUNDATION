<script lang="ts">
  import type { Snippet } from 'svelte';
  import { validateSemanticSpec } from '$lib/json-render/semantic/spec';

  let { title, role = 'supporting', priority = 'supporting', active = true, children }: {
    title: string;
    role?: string;
    priority?: 'primary' | 'supporting' | 'supplemental';
    active?: boolean;
    children: Snippet;
  } = $props();

  const specResult = $derived(validateSemanticSpec({
    root: 'inspector-information',
    elements: {
      'inspector-information': {
        type: 'Information',
        children: [],
        props: { title, role, priority, layout: { columns: 1 } },
      },
    },
  }));
</script>

{#if !active}
  {@render children()}
{:else if specResult.success}
  <section class="inspector-information" role="group" aria-label={title} data-priority={priority}>
    {@render children()}
  </section>
{:else}
  <div role="alert">Não foi possível preparar as informações desta propriedade.</div>
{/if}

<style>
  .inspector-information { min-width: 0; }
</style>
