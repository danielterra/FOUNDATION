<script>
	import { onMount } from 'svelte';
	import { invoke } from '@tauri-apps/api/core';
	import Card from '$lib/components/Card.svelte';
	import PropertyRow from '$lib/components/graph/PropertyRow.svelte';
	import PropertyGroup from '$lib/components/graph/PropertyGroup.svelte';

	let {
		entityId = '',
		entityLabel = '',
		entityIcon = null,
		position = { x: window.innerWidth - 420, y: 100 },
		onClose = () => {},
		onNavigateToEntity = null
	} = $props();

	let panel;
	let entityData = $state(null);
	let loading = $state(true);
	let isFolded = $state(true); // Start folded (minimized)

	// Helper to detect icon type
	function getIconType(icon) {
		if (!icon) return null;
		if (icon.startsWith('http://') || icon.startsWith('https://') ||
		    icon.startsWith('file://') || icon.startsWith('data:')) {
			return 'image';
		}
		return 'material-symbol';
	}

	// Get type display text - show what's defined in RDF
	function getTypeText(data) {
		if (!data) return '';

		const typeLabels = [];

		// Show rdf:type if present
		if (data.types && data.types.length > 0) {
			typeLabels.push(...data.types.map(t => t.label));
		}

		// Show rdfs:subClassOf if present (for classes)
		if (data.superClasses && data.superClasses.length > 0) {
			typeLabels.push(data.superClasses.map(c => c.label).join(', '));
		}

		return typeLabels.join(', ');
	}

	function toggleFold() {
		isFolded = !isFolded;
	}

	onMount(async () => {
		await loadEntityData();
	});

	async function loadEntityData() {
		try {
			loading = true;
			const dataJson = await invoke('entity__get', {
				entityId: entityId
			});
			const data = JSON.parse(dataJson);

			// Filter out rdfs:label, rdfs:comment and foundation:icon (already shown in header or not needed)
			const filteredProperties = (data.properties || []).filter(
				prop => prop.property !== 'rdfs:label'
					&& prop.property !== 'rdfs:comment'
					&& prop.property !== 'foundation:icon'
			);

			// Filter backlinks (already filtered in backend, but just in case)
			const filteredBacklinks = (data.backlinks || []).filter(
				prop => prop.property !== 'rdfs:label'
					&& prop.property !== 'rdfs:comment'
					&& prop.property !== 'foundation:icon'
			);

			// Group properties by source class
			const propertyGroups = new Map();

			for (const prop of filteredProperties) {
				const sourceKey = prop.sourceClass || 'own'; // 'own' for properties of this class

				if (!propertyGroups.has(sourceKey)) {
					propertyGroups.set(sourceKey, {
						sourceClass: prop.sourceClass,
						sourceClassLabel: prop.sourceClassLabel,
						properties: []
					});
				}

				propertyGroups.get(sourceKey).properties.push({
					id: prop.property,
					label: prop.propertyLabel,
					comment: prop.propertyComment,
					value: prop.value,
					valueLabel: prop.valueLabel,
					valueIcon: prop.valueIcon,
					isObjectProperty: prop.isObjectProperty,
					unit: prop.unit,
					unitLabel: prop.unitLabel
				});
			}

			// Convert to array and sort (own properties first, then inherited)
			const groupsArray = Array.from(propertyGroups.values());
			groupsArray.sort((a, b) => {
				if (!a.sourceClass) return -1; // own properties first
				if (!b.sourceClass) return 1;
				return 0;
			});

			// Process backlinks
			const backlinks = filteredBacklinks.map(prop => ({
				id: prop.property,
				label: prop.propertyLabel,
				comment: prop.propertyComment,
				value: prop.value,
				valueLabel: prop.valueLabel,
				valueIcon: prop.valueIcon,
				isObjectProperty: prop.isObjectProperty,
				unit: prop.unit,
				unitLabel: prop.unitLabel
			}));

			entityData = {
				id: entityId,
				label: entityLabel,
				types: data.types || [],
				superClasses: data.superClasses || [],
				propertyGroups: groupsArray,
				backlinks
			};
			loading = false;
		} catch (err) {
			console.error('Failed to load entity data:', err);
			loading = false;
		}
	}

</script>

<div
	bind:this={panel}
	class="entity-inspector-panel"
>
	<Card>
		{#snippet children()}
			<div class="panel-wrapper" class:folded={isFolded}>
			<div class="panel-header panel-drag-handle">
				<div class="panel-header-drag panel-drag-handle">
					<span class="drag-handle">⋮⋮</span>
					{#if entityIcon}
						{@const iconType = getIconType(entityIcon)}
						<div class="entity-icon">
							{#if iconType === 'image'}
								<img src={entityIcon} alt={entityLabel} />
							{:else}
								<span class="material-symbols-outlined">{entityIcon}</span>
							{/if}
						</div>
					{/if}
					<div class="header-text">
						<span class="panel-title">{entityLabel || entityId}</span>
						{#if !loading && entityData}
							{@const typeText = getTypeText(entityData)}
							{#if typeText}
								<span class="panel-type">{typeText}</span>
							{/if}
						{/if}
					</div>
				</div>
				<div class="header-buttons">
					<button class="fold-button" onclick={toggleFold} type="button">
						<span class="material-symbols-outlined">{isFolded ? 'unfold_more' : 'unfold_less'}</span>
					</button>
					<button class="close-button" onclick={onClose} type="button">✕</button>
				</div>
			</div>

			{#if !isFolded}
			<div class="panel-content" onwheel={(e) => e.stopPropagation()}>
				{#if loading}
					<div class="loading">Loading...</div>
				{:else if entityData}
					<div class="entity-info">
						<PropertyRow
							label="IRI"
							value={entityData.id}
						/>

						{#each entityData.propertyGroups as group}
							<PropertyGroup
								groupLabel={group.sourceClassLabel}
								properties={group.properties}
								onNavigateToEntity={onNavigateToEntity}
							/>
						{/each}

						{#if entityData.backlinks && entityData.backlinks.length > 0}
							<PropertyGroup
								groupLabel="Backlinks"
								properties={entityData.backlinks}
								onNavigateToEntity={onNavigateToEntity}
							/>
						{/if}
					</div>
				{:else}
					<div class="error">Failed to load entity data</div>
				{/if}
			</div>
			{/if}
		</div>
		{/snippet}
	</Card>
</div>

<style>
	.entity-inspector-panel {
		position: relative;
		width: 400px;
		z-index: 1000;
		display: flex;
		flex-direction: column;
		isolation: isolate;
	}

	.panel-wrapper {
		display: flex;
		flex-direction: column;
		height: 100%;
		overflow: hidden;
	}

	.panel-wrapper:not(.folded) {
		min-height: 300px;
		max-height: 600px;
	}

	.panel-header {
		position: sticky;
		top: 0;
		display: flex;
		align-items: center;
		justify-content: space-between;
		background: var(--color-black);
		z-index: 1;
	}

	.panel-wrapper:not(.folded) .panel-header {
		padding: 0 0 12px 0;
		margin-bottom: 12px;
		border-bottom: 1px solid var(--color-border);
	}

	.panel-header-drag {
		display: flex;
		align-items: center;
		gap: 8px;
		flex: 1;
		min-width: 0;
	}

	.header-text {
		display: flex;
		flex-direction: column;
		gap: 2px;
		flex: 1;
		min-width: 0;
	}

	.header-buttons {
		display: flex;
		align-items: center;
		gap: 4px;
	}

	.panel-drag-handle {
		cursor: grab;
	}

	.panel-drag-handle:active {
		cursor: grabbing;
	}

	.drag-handle {
		color: var(--color-neutral);
		font-size: 14px;
		line-height: 1;
		user-select: none;
	}

	.entity-icon {
		width: 32px;
		height: 32px;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}

	.entity-icon img {
		width: 28px;
		height: 28px;
		object-fit: cover;
		border-radius: 50%;
	}

	.entity-icon .material-symbols-outlined {
		font-size: 28px;
		color: var(--color-neutral);
	}

	.panel-title {
		font-size: 16px;
		font-weight: 600;
		color: var(--color-neutral);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.panel-type {
		font-size: 11px;
		color: var(--color-neutral-disabled);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.fold-button,
	.close-button {
		background: none;
		border: none;
		color: var(--color-neutral);
		cursor: pointer;
		padding: 4px;
		line-height: 1;
		border-radius: 4px;
		transition: background 0.2s, color 0.2s;
		flex-shrink: 0;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.fold-button {
		font-size: 20px;
		color: var(--color-interactive);
	}

	.fold-button:hover {
		background: var(--color-neutral-dim);
		color: var(--color-primary);
	}

	.close-button {
		font-size: 18px;
	}

	.close-button:hover {
		background: var(--color-danger-dim);
		color: var(--color-danger);
	}

	.panel-content {
		flex: 1;
		overflow-y: auto;
		overflow-x: hidden;
		min-height: 0;
	}

	.loading,
	.error {
		display: flex;
		justify-content: center;
		align-items: center;
		height: 200px;
		color: var(--color-neutral);
	}

	.error {
		color: var(--color-danger);
	}

	.entity-info {
		display: flex;
		flex-direction: column;
	}
</style>
