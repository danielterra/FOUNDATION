<script lang="ts">
  import { inspectorIconSource, inspectorIconSymbol } from './iconSource';

  let {
    kind,
    label,
    icon = null,
    compact = false,
    wrap = false,
    reference = false,
    color = null,
  }: {
    kind: 'class' | 'individual';
    label: string;
    icon?: string | null;
    compact?: boolean;
    wrap?: boolean;
    reference?: boolean;
    color?: string | null;
  } = $props();

  const iconSource = $derived(inspectorIconSource(icon));
  const iconSymbol = $derived(inspectorIconSymbol(icon));
</script>

<span class="ontology-entity-identity" class:compact class:wrap class:reference class:class-identity={kind === 'class'} class:individual-identity={kind === 'individual'} style:color={color || undefined}>
  {#if iconSource}
    <img class="identity-icon-image" src={iconSource} alt="" />
  {:else}
    <span class="material-symbols-outlined identity-icon" aria-hidden="true">{iconSymbol}</span>
  {/if}
  <span class="identity-label">{label}</span>
</span>

<style>
  .ontology-entity-identity {
    display: inline-flex;
    align-items: center;
    gap: var(--spacing-inline-tight);
    min-width: 0;
    max-width: 100%;
    color: var(--color-ontology-individual);
  }

  .ontology-entity-identity.class-identity {
    color: var(--color-ontology-class);
  }

  .identity-icon,
  .identity-icon-image {
    width: var(--type-icon-small);
    height: var(--type-icon-small);
    flex: 0 0 auto;
    font-size: var(--type-icon-small);
    object-fit: contain;
  }

  .identity-label {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: inherit;
  }

  .wrap .identity-label {
    overflow: visible;
    text-overflow: clip;
    white-space: normal;
    overflow-wrap: anywhere;
  }

  .compact .identity-icon,
  .compact .identity-icon-image {
    width: var(--type-icon-small);
    height: var(--type-icon-small);
    font-size: var(--type-icon-small);
  }

  .reference .identity-icon,
  .reference .identity-icon-image {
    width: var(--type-icon-medium);
    height: var(--type-icon-medium);
    font-size: var(--type-icon-medium);
  }
</style>
