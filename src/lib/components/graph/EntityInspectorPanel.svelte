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
		onClose = () => {}
	} = $props();

	let isDragging = $state(false);
	let dragOffset = $state({ x: 0, y: 0 });
	let panel;
	let entityData = $state(null);
	let loading = $state(true);

	// Helper to detect icon type
	function getIconType(icon) {
		if (!icon) return null;
		if (icon.startsWith('http://') || icon.startsWith('https://') ||
		    icon.startsWith('file://') || icon.startsWith('data:')) {
			return 'image';
		}
		return 'material-symbol';
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

			// Filter out rdfs:label and foundation:icon (already shown in header)
			const filteredProperties = (data.properties || []).filter(
				prop => prop.property !== 'rdfs:label' && prop.property !== 'foundation:icon'
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
					value: prop.valueLabel || prop.value,
					isObjectProperty: prop.isObjectProperty
				});
			}

			// Convert to array and sort (own properties first, then inherited)
			const groupsArray = Array.from(propertyGroups.values());
			groupsArray.sort((a, b) => {
				if (!a.sourceClass) return -1; // own properties first
				if (!b.sourceClass) return 1;
				return 0;
			});

			entityData = {
				id: entityId,
				label: entityLabel,
				type: data.entityType,
				propertyGroups: groupsArray
			};
			loading = false;
		} catch (err) {
			console.error('Failed to load entity data:', err);
			loading = false;
		}
	}

	function handleMouseDown(event) {
		if (event.target.closest('.panel-header-drag')) {
			isDragging = true;
			dragOffset = {
				x: event.clientX - position.x,
				y: event.clientY - position.y
			};
			event.preventDefault();
		}
	}

	function handleMouseMove(event) {
		if (isDragging) {
			position = {
				x: event.clientX - dragOffset.x,
				y: event.clientY - dragOffset.y
			};
		}
	}

	function handleMouseUp() {
		isDragging = false;
	}
</script>

<svelte:window
	onmousemove={handleMouseMove}
	onmouseup={handleMouseUp}
/>

<div
	bind:this={panel}
	class="entity-inspector-panel"
	class:dragging={isDragging}
	style="left: {position.x}px; top: {position.y}px;"
>
	<Card>
		{#snippet children()}
			<div class="panel-header" onmousedown={handleMouseDown}>
				<div class="panel-header-drag">
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
					<span class="panel-title">{entityLabel || entityId}</span>
				</div>
				<button class="close-button" onclick={onClose} type="button">✕</button>
			</div>

			<div class="panel-content">
				{#if loading}
					<div class="loading">Loading...</div>
				{:else if entityData}
					<div class="entity-info">
						<PropertyRow
							label="Type"
							value={entityData.type}
						/>

						{#each entityData.propertyGroups as group}
							<PropertyGroup
								groupLabel={group.sourceClassLabel}
								properties={group.properties}
							/>
						{/each}
					</div>
				{:else}
					<div class="error">Failed to load entity data</div>
				{/if}
			</div>
		{/snippet}
	</Card>
</div>

<style>
	.entity-inspector-panel {
		position: fixed;
		width: 400px;
		min-height: 300px;
		max-height: 80vh;
		z-index: 1000;
		display: flex;
		flex-direction: column;
		isolation: isolate;
	}

	.entity-inspector-panel.dragging {
		cursor: grabbing;
		user-select: none;
	}

	.panel-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0 0 12px 0;
		margin-bottom: 12px;
		border-bottom: 1px solid var(--color-border);
	}

	.panel-header-drag {
		display: flex;
		align-items: center;
		gap: 8px;
		cursor: grab;
		flex: 1;
		min-width: 0;
	}

	.panel-header-drag:active {
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
		font-size: 18px;
		color: var(--color-neutral);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.close-button {
		background: none;
		border: none;
		color: var(--color-neutral);
		font-size: 18px;
		cursor: pointer;
		padding: 4px 8px;
		line-height: 1;
		border-radius: 4px;
		transition: background 0.2s, color 0.2s;
		flex-shrink: 0;
	}

	.close-button:hover {
		background: var(--color-danger-dim);
		color: var(--color-danger);
	}

	.panel-content {
		flex: 1;
		overflow-y: auto;
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
