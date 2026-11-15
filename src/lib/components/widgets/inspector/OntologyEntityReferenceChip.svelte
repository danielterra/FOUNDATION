<script lang="ts">
  import type { Snippet } from 'svelte';
  import OntologyEntityIdentity from './OntologyEntityIdentity.svelte';

  let {
    kind,
    label,
    icon = null,
    entityIri = null,
    onopen = null,
    onselect = null,
    children,
    calculated = false,
    mode = 'reference',
    statusColor = null,
    selected = false,
    triggerProps = {},
  }: {
    kind: 'class' | 'individual';
    label: string;
    icon?: string | null;
    entityIri?: string | null;
    onopen?: ((iri: string) => void) | null;
    onselect?: ((iri: string) => void) | null;
    children?: Snippet;
    calculated?: boolean;
    mode?: 'reference' | 'select' | 'trigger';
    statusColor?: string | null;
    selected?: boolean;
    triggerProps?: Record<string, unknown>;
  } = $props();

  function handleClick(event: MouseEvent) {
    const triggerClick = triggerProps.onclick;
    if (typeof triggerClick === 'function') triggerClick(event);

    if (mode === 'reference' && entityIri) onopen?.(entityIri);
    if (mode === 'select' && entityIri) onselect?.(entityIri);
  }
</script>

<button
  {...triggerProps}
  type="button"
  class="entity-reference-chip"
  class:class-reference={kind === 'class'}
  class:individual-reference={kind === 'individual'}
  class:status-reference={statusColor}
  class:selected
  class:calculated
  style:--status-color={statusColor || undefined}
  onclick={handleClick}
  role={mode === 'select' ? 'option' : undefined}
  aria-selected={mode === 'select' ? selected : undefined}
  aria-label={mode === 'trigger'
    ? `Status atual: ${label}. Alterar status`
    : mode === 'select'
      ? `Selecionar status ${label}`
      : `Abrir ${kind === 'class' ? 'classe' : 'entidade'} ${label}`}
  title={label}
>
  {#if children}
    {@render children()}
  {:else}
    <OntologyEntityIdentity {kind} {label} {icon} compact reference color={statusColor} />
  {/if}
  {#if mode === 'reference'}
    <span class="material-symbols-outlined reference-open-icon" aria-hidden="true">open_in_new</span>
  {:else if mode === 'trigger'}
    <span class="material-symbols-outlined reference-open-icon" aria-hidden="true">expand_more</span>
  {:else if selected}
    <span class="material-symbols-outlined reference-open-icon" aria-hidden="true">check</span>
  {/if}
</button>
