<script>
	import { onMount } from 'svelte';
	import { invoke } from '@tauri-apps/api/core';
	import InfiniteCanvas from '$lib/components/canvas/InfiniteCanvas.svelte';
	import DraggablePanel from '$lib/components/canvas/DraggablePanel.svelte';
	import EntityInspectorPanel from '$lib/components/graph/EntityInspectorPanel.svelte';
	import Search from '$lib/components/graph/Search.svelte';
	import SetupWizard from '$lib/components/SetupWizard.svelte';
	import { calculateGraphLayout, extractRelationships, calculateBoundingBox } from '$lib/utils/graphLayout';
	import { Button } from '$lib/components/ui/button';

	let canvasComponent = $state();
	let showSetupWizard = $state(false);
	let checkingSetup = $state(true);
	let inspectorPanels = $state([]);
	let nextPanelId = 0;
	let zoomLevel = $state(1);

	function handleKeydown(event) {
		if ((event.metaKey || event.ctrlKey) && event.key === 'f') {
			event.preventDefault();
		}

		if ((event.metaKey || event.ctrlKey) && event.key === '0') {
			event.preventDefault();
			if (canvasComponent) {
				canvasComponent.resetZoom();
			}
		}

		if ((event.metaKey || event.ctrlKey) && (event.key === '+' || event.key === '=')) {
			event.preventDefault();
			if (canvasComponent) {
				canvasComponent.zoomIn();
			}
		}

		if ((event.metaKey || event.ctrlKey) && event.key === '-') {
			event.preventDefault();
			if (canvasComponent) {
				canvasComponent.zoomOut();
			}
		}
	}

	async function openInspectorPanel(entityId, entityLabel = '', entityIcon = null) {
		const transform = canvasComponent ? canvasComponent.getTransform() : { x: 0, y: 0, scale: 1 };

		const panelWidth = 400;
		const panelHeight = 300;

		const screenCenterX = window.innerWidth / 2;
		const screenCenterY = window.innerHeight / 2;

		const canvasX = (screenCenterX - transform.x) / transform.scale - panelWidth / 2;
		const canvasY = (screenCenterY - transform.y) / transform.scale - panelHeight / 2;

		let relationships = [];
		try {
			const dataJson = await invoke('inspector__get_entity', { entityId });
			const data = JSON.parse(dataJson);
			relationships = extractRelationships(data);
		} catch (e) {
			console.error('Failed to extract relationships:', e);
		}

		inspectorPanels = [
			...inspectorPanels,
			{
				id: nextPanelId++,
				entityId,
				entityLabel,
				entityIcon,
				position: { x: canvasX, y: canvasY },
				relationships,
				isFolded: true
			}
		];

		if (inspectorPanels.length > 1) {
			setTimeout(() => applyAutoLayout(), 50);
		}
	}

	function closeInspectorPanel(panelId) {
		inspectorPanels = inspectorPanels.filter(p => p.id !== panelId);
	}

	function updatePanelPosition(panelId, newPosition) {
		inspectorPanels = inspectorPanels.map(p =>
			p.id === panelId ? { ...p, position: newPosition } : p
		);
	}

	function updatePanelFoldState(panelId, isFolded) {
		inspectorPanels = inspectorPanels.map(p =>
			p.id === panelId ? { ...p, isFolded } : p
		);

		if (inspectorPanels.length > 1) {
			setTimeout(() => applyAutoLayout(), 100);
		}
	}

	function handleSearch(entityId, entityLabel, entityIcon) {
		openInspectorPanel(entityId, entityLabel, entityIcon);
	}

	async function handleSetupComplete() {
		showSetupWizard = false;
		window.location.reload();
	}

	function applyAutoLayout() {
		if (inspectorPanels.length === 0) return;

		const transform = canvasComponent ? canvasComponent.getTransform() : { x: 0, y: 0, scale: 1 };
		const scale = transform.scale;

		const panelsWithDimensions = inspectorPanels.map(panel => {
			const panelElement = document.querySelector(`[data-panel-id="${panel.id}"]`);

			let width = 400;
			let height = 80;

			if (panelElement) {
				const rect = panelElement.getBoundingClientRect();
				width = (rect.width / scale) || 400;
				height = (rect.height / scale) || (panel.isFolded ? 80 : 300);
			} else {
				height = panel.isFolded ? 80 : 300;
			}

			return {
				...panel,
				width,
				height
			};
		});

		const positions = calculateGraphLayout(panelsWithDimensions, {
			rankdir: 'LR',
			nodesep: 30,
			ranksep: 50
		});

		inspectorPanels = inspectorPanels.map(panel => {
			const newPos = positions.get(panel.id);
			return newPos ? { ...panel, position: newPos } : panel;
		});
	}

	onMount(async () => {
		try {
			const setupComplete = await invoke('setup__check');
			if (!setupComplete) {
				showSetupWizard = true;
				checkingSetup = false;
				return;
			}
		} catch (e) {
			console.error('Setup check failed:', e);
		}

		checkingSetup = false;

		try {
			const userJson = await invoke('inspector__get_entity', {
				entityId: 'foundation:ThisUser'
			});
			const userData = JSON.parse(userJson);
			openInspectorPanel('foundation:ThisUser', userData.label, userData.icon);
		} catch (e) {
			console.error('Failed to load current user:', e);
		}
	});
</script>

<svelte:window onkeydown={handleKeydown} />

<div id="canvas-page">
	{#if showSetupWizard}
		<SetupWizard onComplete={handleSetupComplete} />
	{:else if checkingSetup}
		<div class="loading">Checking setup...</div>
	{:else}
		<!-- Fixed UI Elements (always on top) -->
		<div class="fixed-ui">
			<div class="top-bar">
				<div class="logo">FOUNDATION</div>
				<div class="search-container">
					<Search onSelectResult={handleSearch} />
				</div>
				<div class="zoom-controls">
					<Button variant="ghost" size="icon" onclick={() => canvasComponent?.zoomOut()} aria-label="Zoom out (⌘-)">
						<span class="material-symbols-outlined">remove</span>
					</Button>
					<span class="zoom-level">{Math.round(zoomLevel * 100)}%</span>
					<Button variant="ghost" size="icon" onclick={() => canvasComponent?.zoomIn()} aria-label="Zoom in (⌘+)">
						<span class="material-symbols-outlined">add</span>
					</Button>
					<Button variant="ghost" size="icon" onclick={() => canvasComponent?.resetZoom()} aria-label="Reset zoom (⌘0)">
						<span class="material-symbols-outlined">refresh</span>
					</Button>
				</div>
				<div class="layout-controls">
					<Button
						variant="ghost"
						size="sm"
						onclick={applyAutoLayout}
						title="Auto-layout graph"
						disabled={inspectorPanels.length === 0}
					>
						<span class="material-symbols-outlined">account_tree</span>
						Layout
					</Button>
				</div>
			</div>
		</div>

		<!-- Infinite Canvas -->
		<InfiniteCanvas bind:this={canvasComponent} onZoomChange={(scale) => zoomLevel = scale}>
			{#snippet children()}
				<!-- Inspector Panels positioned in canvas space -->
				{#each inspectorPanels as panel (panel.id)}
					<div
						class="canvas-panel"
						data-panel-id={panel.id}
						style="left: {panel.position.x}px; top: {panel.position.y}px;"
					>
						<DraggablePanel
							position={panel.position}
							canvasTransform={canvasComponent ? canvasComponent.getTransform() : { x: 0, y: 0, scale: 1 }}
							onPositionChange={(newPos) => updatePanelPosition(panel.id, newPos)}
						>
							{#snippet children()}
								<EntityInspectorPanel
									entityId={panel.entityId}
									entityLabel={panel.entityLabel}
									entityIcon={panel.entityIcon}
									isFolded={panel.isFolded}
									onFoldChange={(isFolded) => updatePanelFoldState(panel.id, isFolded)}
									onClose={() => closeInspectorPanel(panel.id)}
									onNavigateToEntity={(entityId, entityLabel, entityIcon) => {
										openInspectorPanel(entityId, entityLabel, entityIcon);
									}}
								/>
							{/snippet}
						</DraggablePanel>
					</div>
				{/each}
			{/snippet}
		</InfiniteCanvas>
	{/if}
</div>

<style>
	#canvas-page {
		width: 100%;
		height: 100%;
		position: relative;
		overflow: hidden;
	}

	.loading {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 100%;
		color: var(--color-neutral);
		font-size: 18px;
	}

	/* Fixed UI Elements */
	.fixed-ui {
		position: fixed;
		top: 0;
		left: 0;
		right: 0;
		z-index: 9999;
		pointer-events: none;
	}

	.fixed-ui > * {
		pointer-events: auto;
	}

	.top-bar {
		display: flex;
		align-items: center;
		gap: 24px;
		padding: 16px 24px;
		background: var(--color-surface-1);
	}

	.logo {
		font-family: var(--font-title);
		font-size: 20px;
		font-weight: 700;
		color: var(--color-neutral);
		letter-spacing: 2px;
	}

	.search-container {
		flex: 1;
		max-width: 600px;
	}

	.zoom-controls {
		display: flex;
		align-items: center;
		gap: 8px;
		background: var(--color-surface-2);
		border-radius: var(--radius);
		padding: 6px 12px;
	}

	.zoom-level {
		color: var(--color-neutral-disabled);
		font-size: 12px;
		font-weight: 600;
		min-width: 45px;
		text-align: center;
	}

	.layout-controls {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	/* Canvas Panels */
	.canvas-panel {
		position: absolute;
		transform-origin: top left;
	}
</style>
