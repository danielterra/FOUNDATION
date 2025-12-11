<script>
	import { onMount } from 'svelte';
	import { invoke } from '@tauri-apps/api/core';
	import TopBar from '$lib/components/graph/TopBar.svelte';
	import GraphVisualization from '$lib/components/graph/GraphVisualization.svelte';
	import SetupWizard from '$lib/components/SetupWizard.svelte';
	import KeyboardShortcuts from '$lib/components/KeyboardShortcuts.svelte';
	import EntityInspectorPanel from '$lib/components/graph/EntityInspectorPanel.svelte';

	let loading = $state(true);
	let error = $state(null);
	let showSetupWizard = $state(false);
	let checkingSetup = $state(true);
	let fullGraphData = $state(null);
	let currentNodeId = $state(null);
	let currentNodeLabel = $state('');
	let graphComponent = $state();
	let visibleGraphData = $state(null);
	let shortcuts = $state([]);
	let topBarComponent = $state();
	let inspectorPanels = $state([]);


	// Recenter graph to initial position and reset zoom
	function recenterGraph() {
		if (graphComponent) {
			graphComponent.recenter();
		}
	}

	// Reload graph data from backend
	async function reloadGraph() {
		try {
			loading = true;
			error = null;

			// Reload the current node (or CurrentUser if none selected)
			const targetNodeId = currentNodeId || 'http://foundation.local/ontology/CurrentUser';

			// Use entity__get to reload the node data (same as clicking on a node)
			await navigateToNode(targetNodeId);

			loading = false;
		} catch (err) {
			error = err.toString();
			loading = false;
		}
	}

	// Handle keyboard shortcuts
	function handleKeydown(event) {
		// CMD+F or CTRL+F to focus search
		if ((event.metaKey || event.ctrlKey) && event.key === 'f') {
			event.preventDefault();
			if (topBarComponent) {
				topBarComponent.focusSearch();
			}
		}

		// CMD+0 or CTRL+0 to recenter
		if ((event.metaKey || event.ctrlKey) && event.key === '0') {
			event.preventDefault();
			recenterGraph();
		}

		// CMD+R or CTRL+R to reload
		if ((event.metaKey || event.ctrlKey) && event.key === 'r') {
			event.preventDefault();
			reloadGraph();
		}
	}

	// Handle setup wizard completion
	async function handleSetupComplete() {
		showSetupWizard = false;
		window.location.reload();
	}

	onMount(async () => {
		// Load keyboard shortcuts from backend
		try {
			const shortcutsJson = await invoke('shortcuts__get_all');
			shortcuts = JSON.parse(shortcutsJson);
		} catch (e) {
			console.error('Failed to load shortcuts:', e);
		}

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

		// Add keyboard event listener
		window.addEventListener('keydown', handleKeydown);

		try {
			// Start from foundation:ThisUser
			await navigateToNode('foundation:ThisUser');

			loading = false;

			// Add window resize listener
			const handleResize = () => {
				// Just trigger re-render without reloading data
				if (currentNodeId && visibleGraphData) {
					visibleGraphData = { ...visibleGraphData };
				}
			};
			window.addEventListener('resize', handleResize);

			// Cleanup on unmount
			return () => {
				window.removeEventListener('resize', handleResize);
				window.removeEventListener('keydown', handleKeydown);
			};
		} catch (err) {
			error = err.toString();
			loading = false;
		}
	});


	async function navigateToNode(nodeId) {
		try {
			// Get entity data with full neighborhood
			const entityJson = await invoke('entity__get', {
				entityId: nodeId
			});
			const entityData = JSON.parse(entityJson);

			currentNodeId = entityData.id;
			currentNodeLabel = entityData.label;

			// Set graph visualization data
			visibleGraphData = {
				nodes: entityData.nodes,
				links: entityData.links,
				centralNodeId: entityData.id
			};
		} catch (err) {
			console.error('Failed to navigate to node:', err);
			// Don't set global error - just log it
			// This prevents hiding the TopBar when individual panels fail
		}
	}

	function handleNodeClick(nodeId, nodeLabel, nodeIcon) {
		// Se clicar no node central, abre o painel
		if (nodeId === currentNodeId) {
			openInspectorPanel(nodeId, nodeLabel, nodeIcon);
		} else {
			// Se clicar em outro node, navega para ele
			navigateToNode(nodeId);
		}
	}

	function openInspectorPanel(nodeId, nodeLabel, nodeIcon) {
		// Calculate position offset for new panels
		const baseX = window.innerWidth - 420;
		const baseY = 100;
		const offset = inspectorPanels.length * 30;

		// Create new panel
		const newPanel = {
			id: `${nodeId}-${Date.now()}`,
			entityId: nodeId,
			entityLabel: nodeLabel,
			entityIcon: nodeIcon,
			position: {
				x: baseX - offset,
				y: baseY + offset
			}
		};

		inspectorPanels = [...inspectorPanels, newPanel];
	}

	function closeInspectorPanel(panelId) {
		inspectorPanels = inspectorPanels.filter(p => p.id !== panelId);
	}
</script>

<div id="graph-container">
	{#if checkingSetup}
		<div class="loading">Checking setup...</div>
	{:else if showSetupWizard}
		<SetupWizard onComplete={handleSetupComplete} />
	{:else if loading}
		<div class="loading">Loading ontology graph...</div>
	{:else if error}
		<div class="error">Error: {error}</div>
	{:else}
		<TopBar bind:this={topBarComponent} onRecenter={recenterGraph} onSearch={handleNodeClick} screenName="Ontology Graph" />

		{#if visibleGraphData}
			<GraphVisualization
				bind:this={graphComponent}
				graphData={visibleGraphData}
				onNodeClick={handleNodeClick}
			/>
		{/if}

		<KeyboardShortcuts {shortcuts} />
	{/if}
</div>

{#each inspectorPanels as panel (panel.id)}
	<EntityInspectorPanel
		entityId={panel.entityId}
		entityLabel={panel.entityLabel}
		entityIcon={panel.entityIcon}
		position={panel.position}
		onClose={() => closeInspectorPanel(panel.id)}
		onNavigateToEntity={(entityId, entityLabel, entityIcon) => {
			navigateToNode(entityId);
		}}
	/>
{/each}

<style>
	#graph-container {
		width: 100vw;
		height: 100vh;
		position: relative;
		overflow: hidden;
	}

	.loading,
	.error {
		display: flex;
		justify-content: center;
		align-items: center;
		width: 100%;
		height: 100vh;
		font-size: 18px;
		color: var(--color-neutral);
		background: transparent;
		position: relative;
		z-index: 1;
	}

	.error {
		color: var(--color-danger);
	}
</style>
