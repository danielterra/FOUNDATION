<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { Spec } from '@json-render/core';
  import { validateSemanticSpec } from '$lib/json-render/semantic/spec';

  let {
    title,
    rangeLabel,
    multiple,
    selectedValues = [],
    children,
  } = $props<{
    title: string;
    rangeLabel?: string | null;
    multiple: boolean;
    selectedValues?: Array<{ iri: string; label: string }>;
    children: Snippet;
  }>();

  const specResult = $derived(validateSemanticSpec({
    root: 'reference-decision',
    elements: {
      'reference-decision': {
        type: 'Decision',
        children: [],
        props: {
          title,
          domain: rangeLabel ?? undefined,
          selectionMode: multiple ? 'multiple' : 'single',
          complexity: 'complex',
          candidates: selectedValues.length
            ? selectedValues.map(({ iri, label }) => ({ value: iri, title: label }))
            : [{ value: '', title: 'Nenhuma referência selecionada' }],
          selected: multiple ? selectedValues.map(({ iri }) => iri) : selectedValues[0]?.iri,
        },
      },
    },
  }));
</script>

{#if specResult.success}
  {@render children()}
{:else}
  <div role="alert">Não foi possível preparar a seleção de referências.</div>
{/if}
