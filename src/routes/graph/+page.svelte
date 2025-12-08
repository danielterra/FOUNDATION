<script>
	import { onMount } from 'svelte';
	import { invoke } from '@tauri-apps/api/core';
	import TopBar from '$lib/components/graph/TopBar.svelte';
	import GraphVisualization from '$lib/components/graph/GraphVisualization.svelte';
	import SetupWizard from '$lib/components/SetupWizard.svelte';

	let loading = true;
	let error = null;
	let showSetupWizard = false;
	let checkingSetup = true;
	let fullGraphData = null;
	let currentNodeId = null;
	let currentNodeLabel = '';
	let graphComponent;
	let visibleGraphData = null;


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
			error = err.toString();
		}
	}

	function handleNodeClick(nodeId, nodeLabel) {
		navigateToNode(nodeId);
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
		<TopBar onRecenter={recenterGraph} screenName="Ontology Graph" />

		{#if visibleGraphData}
			<GraphVisualization
				bind:this={graphComponent}
				graphData={visibleGraphData}
				onNodeClick={handleNodeClick}
			/>
		{/if}
	{/if}
</div>

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
