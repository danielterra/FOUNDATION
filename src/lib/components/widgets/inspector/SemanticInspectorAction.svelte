<script lang="ts">
  import { JsonUIProvider, Renderer } from '@json-render/svelte';
  import type { Spec } from '@json-render/core';
  import { registry } from '$lib/json-render/semantic/registry';
  import { validateSemanticSpec } from '$lib/json-render/semantic/spec';

  let {
    label,
    icon,
    priority = 'supporting',
    available = true,
    onpress,
  } = $props<{
    label: string;
    icon: string;
    priority?: 'primary' | 'supporting' | 'destructive';
    available?: boolean;
    onpress: () => void;
  }>();

  const specResult = $derived(validateSemanticSpec({
    root: 'inspector-action',
    elements: {
      'inspector-action': {
        type: 'Action',
        children: [],
        props: { label, icon, priority, available },
        on: { press: { action: 'perform' } },
      },
    },
  }));
</script>

{#if specResult.success}
  <div class="semantic-action-shell">
    <JsonUIProvider {registry} handlers={{ perform: async () => onpress() }}>
      <Renderer spec={specResult.spec as Spec} {registry} />
    </JsonUIProvider>
  </div>
{/if}

<style>
  .semantic-action-shell :global(.action-copy) { display: none; }
  .semantic-action-shell :global(.action-block) { display: block; }
</style>
