<script lang="ts">
  import { JsonUIProvider, Renderer } from '@json-render/svelte';
  import type { Spec } from '@json-render/core';
  import { registry } from '$lib/json-render/semantic/registry';
  import { validateSemanticSpec } from '$lib/json-render/semantic/spec';

  let {
    label,
    value = '',
    cardinality = 'single',
    valueType = 'text',
    language,
    required = false,
    onvaluechange,
  } = $props<{
    label: string;
    value?: string | string[];
    cardinality?: 'single' | 'multiple';
    valueType?: 'text' | 'email' | 'number' | 'boolean' | 'date' | 'date_time' | 'currency' | 'percentage' | 'url' | 'code' | 'markdown' | 'html' | 'query_config' | 'recurrence_rule';
    language?: string | null;
    required?: boolean;
    onvaluechange: (value: string | string[]) => void;
  }>();

  let inputState = $state<{ value: string | string[] }>({ value: '' });
  $effect(() => {
    inputState.value = cardinality === 'multiple'
      ? (Array.isArray(value) ? [...value] : [String(value)])
      : (Array.isArray(value) ? value[0] ?? '' : String(value));
  });

  const specResult = $derived(validateSemanticSpec({
    root: 'value-entry',
    elements: {
      'value-entry': {
        type: 'ValueEntry',
        children: [],
        props: {
          label,
          labelVisible: false,
          value: { $bindState: '/value' },
          cardinality,
          valueType,
          language: language ?? undefined,
          required,
          editable: true,
        },
      },
    },
    state: inputState,
  }));

  function handleStateChange(changes: Array<{ path: string; value: unknown }>) {
    const changedValue = [...changes].reverse().find((change) => change.path === '/value')?.value;
    if (changedValue !== undefined) onvaluechange(Array.isArray(changedValue) ? changedValue.map(String) : String(changedValue));
  }
</script>

{#if specResult.success}
  <JsonUIProvider {registry} initialState={inputState} handlers={{}} onStateChange={handleStateChange}>
    <Renderer spec={specResult.spec as Spec} {registry} />
  </JsonUIProvider>
{:else}
  <div role="alert">Não foi possível apresentar este campo para edição.</div>
{/if}
