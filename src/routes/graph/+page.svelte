<script>
	import { onMount } from 'svelte';
	import { invoke } from '@tauri-apps/api/core';
	import TopBar from '$lib/components/graph/TopBar.svelte';
	import SidePanel from '$lib/components/graph/SidePanel.svelte';
	import InstanceSidePanel from '$lib/components/graph/InstanceSidePanel.svelte';
	import GraphVisualization from '$lib/components/graph/GraphVisualization.svelte';
	import SearchBar from '$lib/components/graph/SearchBar.svelte';
	import SetupWizard from '$lib/components/SetupWizard.svelte';

	let loading = true;
	let error = null;
	let showSetupWizard = false;
	let checkingSetup = true;
	let fullGraphData = null;
	let currentNodeId = null;
	let currentNodeLabel = '';
	let currentNodeIcon = null;
	let nodeTriples = [];
	let nodeBacklinks = [];
	let nodeStatistics = null;
	let applicableProperties = [];
	let loadingTriples = false;
	let graphComponent;
	let visibleGraphData = null;
	let isInstance = false;

	// Get display name for a node (use label if available, otherwise simplify URI)
	function getNodeDisplayName(nodeId) {
		const node = fullGraphData?.nodes.find((n) => n.id === nodeId);
		if (node) {
			return node.label;
		}
		// Fallback: simplify URI
		return nodeId.split(/[/#]/).pop();
	}

	// Get icon for a node
	function getNodeIcon(nodeId) {
		const node = fullGraphData?.nodes.find((n) => n.id === nodeId);
		return node?.icon || null;
	}

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

			// Reload graph data centered on current node or CurrentUser if none selected
			const targetNodeId = currentNodeId || 'http://foundation.local/ontology/CurrentUser';
			const graphJson = await invoke('get_ontology_graph', {
				centralNodeId: targetNodeId
			});
			fullGraphData = JSON.parse(graphJson);

			loading = false;

			// Navigate to the same node to refresh triples and visualization
			if (currentNodeId) {
				await navigateToNode(currentNodeId);
			} else {
				await navigateToNode('http://foundation.local/ontology/CurrentUser');
			}
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

	async function loadNodeTriples(nodeId) {
		loadingTriples = true;
		try {
			// Check if node is an instance and load appropriate data
			const instanceCheck = await invoke('node__check_is_instance', { nodeId: nodeId });
			isInstance = instanceCheck;

			if (isInstance) {
				// For instances, load only triples and icon
				const [triplesJson, icon] = await Promise.all([
					invoke('get_node_triples', { nodeId: nodeId }),
					invoke('node__get_icon', { nodeId: nodeId })
				]);

				nodeTriples = JSON.parse(triplesJson);
				currentNodeIcon = icon;

				// Clear class-specific data
				nodeBacklinks = [];
				nodeStatistics = null;
				applicableProperties = [];
			} else {
				// For classes, load all data
				const [triplesJson, backlinksJson, statisticsJson, icon, propertiesJson] = await Promise.all([
					invoke('get_node_triples', { nodeId: nodeId }),
					invoke('get_node_backlinks', { nodeId: nodeId }),
					invoke('get_node_statistics', { nodeId: nodeId }),
					invoke('node__get_icon', { nodeId: nodeId }),
					invoke('get_applicable_properties', { nodeId: nodeId })
				]);

				nodeTriples = JSON.parse(triplesJson);
				nodeBacklinks = JSON.parse(backlinksJson);
				nodeStatistics = JSON.parse(statisticsJson);
				currentNodeIcon = icon;
				applicableProperties = JSON.parse(propertiesJson);
			}
		} catch (err) {
			console.error('Failed to load node data:', err);
			nodeTriples = [];
			nodeBacklinks = [];
			nodeStatistics = null;
			currentNodeIcon = null;
			applicableProperties = [];
			isInstance = false;
		} finally {
			loadingTriples = false;
		}
	}

	async function navigateToNode(nodeId) {
		try {
			// Get entity data with full neighborhood
			const entityJson = await invoke('entity__get', {
				entityId: nodeId
			});
			const entityData = JSON.parse(entityJson);

			currentNodeId = entityData.id;
			currentNodeLabel = entityData.label;
			currentNodeIcon = entityData.icon;

			// Determine if it's an individual
			isInstance = entityData.entityType === 'individual';

			// Set graph visualization data
			visibleGraphData = {
				nodes: entityData.nodes,
				links: entityData.links,
				centralNodeId: entityData.id
			};

			// For now, skip the side panel data load
			// We'll implement this properly later
			nodeTriples = [];
			nodeBacklinks = [];
			nodeStatistics = null;
			applicableProperties = [];
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
		<TopBar onRecenter={recenterGraph} />

		{#if currentNodeId}
			{#if isInstance}
				<InstanceSidePanel
					{currentNodeLabel}
					{currentNodeIcon}
					{nodeTriples}
					{loadingTriples}
					onNavigateToNode={navigateToNode}
					{getNodeDisplayName}
					{getNodeIcon}
				/>
			{:else}
				<SidePanel
					{currentNodeLabel}
					{currentNodeIcon}
					{nodeTriples}
					{nodeBacklinks}
					{nodeStatistics}
					{applicableProperties}
					{loadingTriples}
					onNavigateToNode={navigateToNode}
					{getNodeDisplayName}
				/>
			{/if}
		{/if}

		{#if visibleGraphData}
			<GraphVisualization
				bind:this={graphComponent}
				graphData={visibleGraphData}
				onNodeClick={handleNodeClick}
			/>
		{/if}

		<SearchBar onSelectClass={navigateToNode} />
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
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		font-size: 18px;
		color: rgba(255, 255, 255, 0.7);
		background: transparent;
		position: relative;
		z-index: 1;
	}

	.error {
		color: #ff6b9d;
	}
</style>
