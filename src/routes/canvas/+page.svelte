<script>
	import { onMount } from 'svelte';
	import { invoke } from '@tauri-apps/api/core';
	import InfiniteCanvas from '$lib/components/canvas/InfiniteCanvas.svelte';
	import DraggablePanel from '$lib/components/canvas/DraggablePanel.svelte';
	import EntityInspectorPanel from '$lib/components/graph/EntityInspectorPanel.svelte';
	import Search from '$lib/components/graph/Search.svelte';
	import SetupWizard from '$lib/components/SetupWizard.svelte';
	import { calculateGraphLayout, extractRelationships, calculateBoundingBox } from '$lib/utils/graphLayout';

	let canvasComponent;
	let showSetupWizard = $state(false);
	let checkingSetup = $state(true);
	let inspectorPanels = $state([]);
	let nextPanelId = 0;
	let zoomLevel = $state(1);

	// Handle keyboard shortcuts
	function handleKeydown(event) {
		// CMD+F or CTRL+F to focus search
		if ((event.metaKey || event.ctrlKey) && event.key === 'f') {
			event.preventDefault();
			// TODO: Focus search
		}

		// CMD+0 or CTRL+0 to reset zoom
		if ((event.metaKey || event.ctrlKey) && event.key === '0') {
			event.preventDefault();
			if (canvasComponent) {
				canvasComponent.resetZoom();
			}
		}

		// CMD+Plus or CTRL+Plus to zoom in
		if ((event.metaKey || event.ctrlKey) && (event.key === '+' || event.key === '=')) {
			event.preventDefault();
			if (canvasComponent) {
				canvasComponent.zoomIn();
			}
		}

		// CMD+Minus or CTRL+Minus to zoom out
		if ((event.metaKey || event.ctrlKey) && event.key === '-') {
			event.preventDefault();
			if (canvasComponent) {
				canvasComponent.zoomOut();
			}
		}
	}

	async function openInspectorPanel(entityId, entityLabel = '', entityIcon = null) {
		console.log('openInspectorPanel called:', entityId, entityLabel, entityIcon);

		const transform = canvasComponent ? canvasComponent.getTransform() : { x: 0, y: 0, scale: 1 };
		console.log('Canvas transform:', transform);

		// Panel dimensions
		const panelWidth = 400;
		const panelHeight = 300;

		// Calculate center of viewport in screen space
		const screenCenterX = window.innerWidth / 2;
		const screenCenterY = window.innerHeight / 2;

		// Convert to canvas space and center the panel
		const canvasX = (screenCenterX - transform.x) / transform.scale - panelWidth / 2;
		const canvasY = (screenCenterY - transform.y) / transform.scale - panelHeight / 2;

		console.log('New panel position:', { x: canvasX, y: canvasY });

		// Carregar dados da entidade para extrair relacionamentos
		let relationships = [];
		try {
			const dataJson = await invoke('entity__get', { entityId });
			const data = JSON.parse(dataJson);
			relationships = extractRelationships(data);
			console.log(`Extracted ${relationships.length} relationships for ${entityLabel}:`, relationships);
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

		console.log('Inspector panels after add:', inspectorPanels);

		// Auto-layout se já existem painéis (mais de 1 após adicionar)
		if (inspectorPanels.length > 1) {
			// Usar setTimeout para garantir que o DOM atualizou
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

		// Recalcular layout quando mudar estado fold
		// Aumentar delay para garantir que o DOM atualizou completamente
		if (inspectorPanels.length > 1) {
			setTimeout(() => applyAutoLayout(), 100);
		}
	}

	function handleSearch(entityId, entityLabel, entityIcon) {
		console.log('handleSearch called:', entityId, entityLabel, entityIcon);
		openInspectorPanel(entityId, entityLabel, entityIcon);
	}

	async function handleSetupComplete() {
		showSetupWizard = false;
		window.location.reload();
	}

	function applyAutoLayout() {
		if (inspectorPanels.length === 0) return;

		// Obter scale atual do canvas para corrigir medições
		const transform = canvasComponent ? canvasComponent.getTransform() : { x: 0, y: 0, scale: 1 };
		const scale = transform.scale;

		// Medir dimensões reais dos painéis do DOM
		const panelsWithDimensions = inspectorPanels.map(panel => {
			// Buscar o elemento real do painel no DOM
			const panelElement = document.querySelector(`[data-panel-id="${panel.id}"]`);

			let width = 400;
			let height = 80; // altura mínima para folded

			if (panelElement) {
				// Obter dimensões reais do DOM e corrigir pelo scale do canvas
				const rect = panelElement.getBoundingClientRect();
				width = (rect.width / scale) || 400;
				height = (rect.height / scale) || (panel.isFolded ? 80 : 300);
				console.log(`📏 ${panel.entityLabel}: folded=${panel.isFolded}, measured=${Math.round(rect.height)}px, scale=${scale.toFixed(2)}, final=${Math.round(height)}px`);
			} else {
				// Fallback: estimativa baseada no estado
				height = panel.isFolded ? 80 : 300;
				console.log(`⚠️ ${panel.entityLabel}: DOM not found, using fallback=${height}px`);
			}

			return {
				...panel,
				width,
				height
			};
		});

		// Calcular layout com dagre - usa dimensões individuais de cada painel
		const positions = calculateGraphLayout(panelsWithDimensions, {
			rankdir: 'LR', // Left to Right
			nodesep: 30, // Espaçamento vertical entre nós do mesmo nível
			ranksep: 50 // Espaçamento horizontal entre níveis
		});

		// Atualizar posições dos painéis (mantém zoom e pan do usuário)
		inspectorPanels = inspectorPanels.map(panel => {
			const newPos = positions.get(panel.id);
			return newPos ? { ...panel, position: newPos } : panel;
		});
	}

	onMount(async () => {
		// Check if initial setup is needed
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

		// Load current user by default
		try {
			const userJson = await invoke('entity__get', {
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
					<button onclick={() => canvasComponent?.zoomOut()} title="Zoom out (⌘-)">−</button>
					<span class="zoom-level">{Math.round(zoomLevel * 100)}%</span>
					<button onclick={() => canvasComponent?.zoomIn()} title="Zoom in (⌘+)">+</button>
					<button onclick={() => canvasComponent?.resetZoom()} title="Reset zoom (⌘0)">⟲</button>
				</div>
				<div class="layout-controls">
					<button
						onclick={applyAutoLayout}
						title="Auto-layout graph"
						disabled={inspectorPanels.length === 0}
					>
						<span class="material-symbols-outlined">account_tree</span>
						Layout
					</button>
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
		width: 100vw;
		height: 100vh;
		position: relative;
		overflow: hidden;
	}

	.loading {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 100vh;
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
		background: linear-gradient(to bottom, rgba(0, 0, 0, 0.8), transparent);
		backdrop-filter: blur(8px);
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
		background: rgba(0, 0, 0, 0.6);
		border: 1px solid color-mix(in srgb, var(--color-neutral) 20%, black);
		border-radius: 8px;
		padding: 6px 12px;
	}

	.zoom-controls button {
		background: none;
		border: none;
		color: var(--color-neutral);
		font-size: 18px;
		font-weight: 700;
		cursor: pointer;
		padding: 4px 8px;
		border-radius: 4px;
		transition: background 0.2s;
		min-width: 28px;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.zoom-controls button:hover {
		background: color-mix(in srgb, var(--color-neutral) 10%, transparent);
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

	.layout-controls button {
		display: flex;
		align-items: center;
		gap: 6px;
		background: rgba(0, 0, 0, 0.6);
		border: 1px solid color-mix(in srgb, var(--color-neutral) 20%, black);
		color: var(--color-neutral);
		font-size: 13px;
		font-weight: 600;
		cursor: pointer;
		padding: 8px 16px;
		border-radius: 8px;
		transition: background 0.2s, border-color 0.2s;
	}

	.layout-controls button:hover:not(:disabled) {
		background: color-mix(in srgb, var(--color-neutral) 10%, transparent);
		border-color: color-mix(in srgb, var(--color-neutral) 30%, black);
	}

	.layout-controls button:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.layout-controls button .material-symbols-outlined {
		font-size: 18px;
	}

	/* Canvas Panels */
	.canvas-panel {
		position: absolute;
		transform-origin: top left;
	}
</style>
