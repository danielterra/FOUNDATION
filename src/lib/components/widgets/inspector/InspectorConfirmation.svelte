<script lang="ts">
  import type { Snippet } from 'svelte';
  import { validateSemanticSpec } from '$lib/json-render/semantic/spec';

  let { action, consequence, impact, children }: {
    action: string;
    consequence: string;
    impact?: string;
    children: Snippet;
  } = $props();

  const specResult = $derived(validateSemanticSpec({
    root: 'inspector-confirmation',
    elements: {
      'inspector-confirmation': {
        type: 'Confirmation',
        children: [],
        props: {
          action,
          consequence,
          impact,
          reversibility: 'irreversible',
          confirmLabel: 'Excluir',
          cancelLabel: 'Cancelar',
          requiresConfirmation: true,
        },
      },
    },
  }));
</script>

{#if specResult.success}
  <section class="inspector-confirmation" aria-label={action}>
    {@render children()}
  </section>
{:else}
  <div role="alert">Não foi possível preparar a confirmação de exclusão.</div>
{/if}

<style>
  .inspector-confirmation { min-width: 0; }
</style>
