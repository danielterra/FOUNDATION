<script>
	import PropertyRow from './PropertyRow.svelte';

	let {
		groupLabel = null,
		properties = [],
		onNavigateToEntity = null
	} = $props();
</script>

{#if groupLabel}
	<div class="property-group-header">
		<span class="group-label">from {groupLabel}</span>
	</div>
{/if}

<div class="property-group-content">
	{#each properties as property}
		<PropertyRow
			label={property.label}
			comment={property.comment}
			value={property.value}
			valueLabel={property.valueLabel}
			valueIcon={property.valueIcon}
			unit={property.unit}
			unitLabel={property.unitLabel}
			onValueClick={property.isObjectProperty && onNavigateToEntity ? () => onNavigateToEntity(property.value, property.valueLabel, property.valueIcon) : null}
		/>
	{/each}
</div>

<style>
	.property-group-header {
		margin-top: 16px;
		margin-bottom: 8px;
		padding-bottom: 6px;
		border-bottom: 1px solid var(--color-border);
	}

	.group-label {
		font-size: 11px;
		font-weight: 600;
		color: color-mix(in srgb, var(--color-neutral) 60%, transparent);
		text-transform: uppercase;
		letter-spacing: 0.8px;
	}

	.property-group-content {
		display: flex;
		flex-direction: column;
	}
</style>
