<script lang="ts">
	import { modal } from '$lib/stores/modal';
	import { Button } from '$lib/components/ui/button';
	import type { SubconsciousEntity } from '$lib/stores/modal';

	let {
		entities,
		onEntityClick,
	}: {
		entities: SubconsciousEntity[];
		onEntityClick: (iri: string) => void;
	} = $props();

	const relevant = $derived(entities.filter((e) => !e.is_open_loop));
	const openLoops = $derived(entities.filter((e) => e.is_open_loop));

	const totalProps = $derived(
		entities.reduce((sum, e) => sum + (e.property_hits?.length ?? 0), 0)
	);

	function handleOpenEntity(iri: string) {
		modal.set(null);
		onEntityClick(iri);
	}
</script>

<div class="memory-list">
	{#if relevant.length > 0}
		<div class="memory-section-label">CONTEXTO RELEVANTE</div>
		{#each relevant as entity (entity.iri)}
			<div class="entity-group">
				<div class="entity-anchor">
					<span
						class="entity-anchor-label"
						title={entity.iri}
						data-iri={entity.iri}
					>
						"{entity.label}"
						<span class="entity-type-badge">{entity.type_label}</span>
					</span>
					<Button
						variant="ghost"
						class="open-entity-btn"
						onclick={() => handleOpenEntity(entity.iri)}
						aria-label="Abrir {entity.label}"
					>
						<span class="material-symbols-outlined open-entity-icon">open_in_new</span>
					</Button>
				</div>
				{#if entity.property_hits?.length > 0}
					<div class="property-hits">
						{#each entity.property_hits as hit, i (`${entity.iri}|${hit.prop_iri}|${i}`)}
							<div
								class="property-hit"
								title={`${entity.iri}\n${hit.prop_iri}`}
								data-entity-iri={entity.iri}
								data-prop-iri={hit.prop_iri}
							>
								<span class="prop-label">{hit.prop_label}:</span>
								<span class="prop-value">{hit.value}</span>
							</div>
						{/each}
					</div>
				{/if}
			</div>
		{/each}
	{/if}

	{#if openLoops.length > 0}
		<div class="memory-section-label open-loops-label">OPEN LOOPS</div>
		{#each openLoops as entity (entity.iri)}
			<div class="entity-group">
				<div class="entity-anchor">
					<span
						class="entity-anchor-label"
						title={entity.iri}
						data-iri={entity.iri}
					>
						"{entity.label}"
						<span class="entity-type-badge">{entity.type_label}</span>
						<span class="open-loop-tag">aberto</span>
					</span>
					<Button
						variant="ghost"
						class="open-entity-btn"
						onclick={() => handleOpenEntity(entity.iri)}
						aria-label="Abrir {entity.label}"
					>
						<span class="material-symbols-outlined open-entity-icon">open_in_new</span>
					</Button>
				</div>
				{#if entity.property_hits?.length > 0}
					<div class="property-hits">
						{#each entity.property_hits as hit, i (`${entity.iri}|${hit.prop_iri}|${i}`)}
							<div
								class="property-hit"
								title={`${entity.iri}\n${hit.prop_iri}`}
								data-entity-iri={entity.iri}
								data-prop-iri={hit.prop_iri}
							>
								<span class="prop-label">{hit.prop_label}:</span>
								<span class="prop-value">{hit.value}</span>
							</div>
						{/each}
					</div>
				{/if}
			</div>
		{/each}
	{/if}

	<div class="memory-footer">
		{totalProps} {totalProps === 1 ? 'propriedade' : 'propriedades'} de
		{entities.length} {entities.length === 1 ? 'entidade' : 'entidades'}
	</div>
</div>

<style>
	.memory-list {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.memory-section-label {
		font-size: 9px;
		font-weight: 700;
		letter-spacing: 0.1em;
		text-transform: uppercase;
		color: var(--color-neutral-disabled);
		padding: 8px 0 4px;
	}

	.memory-section-label:first-child {
		padding-top: 0;
	}

	.open-loops-label {
		color: var(--color-warning);
		margin-top: 8px;
	}

	.entity-group {
		display: flex;
		flex-direction: column;
		gap: 1px;
		margin-bottom: 4px;
	}

	.entity-anchor {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 6px;
		padding: 3px 4px 3px 0;
		border-radius: var(--radius);
	}

	.entity-anchor-label {
		display: flex;
		align-items: center;
		gap: 5px;
		font-size: 12px;
		font-weight: 600;
		color: var(--color-neutral-active);
		cursor: default;
		flex: 1;
		min-width: 0;
	}

	.entity-type-badge {
		font-size: 9px;
		font-weight: 500;
		color: var(--color-neutral-secondary);
		background: color-mix(in srgb, var(--color-white) 8%, transparent);
		border-radius: 999px;
		padding: 1px 6px;
		white-space: nowrap;
		flex-shrink: 0;
	}

	.open-loop-tag {
		font-size: 9px;
		font-weight: 600;
		color: var(--color-warning);
		background: color-mix(in srgb, var(--color-warning) 15%, transparent);
		border-radius: 999px;
		padding: 1px 6px;
		white-space: nowrap;
		flex-shrink: 0;
	}

	:global([data-slot="button"].open-entity-btn) {
		width: 20px;
		height: 20px;
		min-width: 20px;
		padding: 0;
		flex-shrink: 0;
		color: var(--color-neutral-disabled);
		display: flex;
		align-items: center;
		justify-content: center;
	}

	:global([data-slot="button"].open-entity-btn:hover) {
		color: var(--color-interactive);
		background: color-mix(in srgb, var(--color-interactive) 12%, transparent);
	}

	.open-entity-icon {
		font-size: 13px;
	}

	.property-hits {
		display: flex;
		flex-direction: column;
		gap: 1px;
		padding-left: 16px;
	}

	.property-hit {
		display: flex;
		align-items: baseline;
		gap: 5px;
		padding: 2px 6px;
		border-radius: var(--radius);
		cursor: default;
	}

	.property-hit:hover {
		background: color-mix(in srgb, var(--color-white) 5%, transparent);
	}

	.prop-label {
		font-size: 11px;
		font-weight: 500;
		color: var(--color-neutral-secondary);
		white-space: nowrap;
		flex-shrink: 0;
	}

	.prop-value {
		font-size: 11px;
		color: var(--color-neutral);
		overflow: hidden;
		text-overflow: ellipsis;
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		white-space: normal;
		max-width: 360px;
	}

	.memory-footer {
		margin-top: 10px;
		padding-top: 8px;
		border-top: 1px solid color-mix(in srgb, var(--color-white) 8%, transparent);
		font-size: 10px;
		color: var(--color-neutral-disabled);
		text-align: right;
	}
</style>
