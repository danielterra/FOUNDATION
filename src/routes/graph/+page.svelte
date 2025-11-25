<script>
	import { onMount } from 'svelte';
	import { invoke } from '@tauri-apps/api/core';
	import TopBar from '$lib/components/graph/TopBar.svelte';
	import SidePanel from '$lib/components/graph/SidePanel.svelte';
	import GraphVisualization from '$lib/components/graph/GraphVisualization.svelte';
	import SearchBar from '$lib/components/graph/SearchBar.svelte';

	let loading = true;
	let error = null;
	let fullGraphData = null;
	let currentNodeId = null;
	let currentNodeLabel = '';
	let nodeFacts = [];
	let loadingFacts = false;
	let graphComponent;
	let visibleGraphData = null;

	// Get display name for a node (use label if available, otherwise simplify URI)
	function getNodeDisplayName(nodeId) {
		const node = fullGraphData?.nodes.find((n) => n.id === nodeId);
		if (node) {
			return node.label;
		}
		// Fallback: simplify URI
		return nodeId.split(/[/#]/).pop();
	}

	// Recenter graph to initial position and reset zoom
	function recenterGraph() {
		if (graphComponent) {
			graphComponent.recenter();
		}
	}

	// Handle keyboard shortcuts
	function handleKeydown(event) {
		// CMD+0 or CTRL+0 to recenter
		if ((event.metaKey || event.ctrlKey) && event.key === '0') {
			event.preventDefault();
			recenterGraph();
		}
	}

	onMount(async () => {
		// Add keyboard event listener
		window.addEventListener('keydown', handleKeydown);

		try {
			// Get graph data from Rust backend starting from owl:Thing
			const graphJson = await invoke('get_ontology_graph', {
				centralNodeId: 'http://www.w3.org/2002/07/owl#Thing'
			});
			fullGraphData = JSON.parse(graphJson);

			loading = false;

			// Wait for next tick to ensure container dimensions are available
			setTimeout(() => {
				// Start with owl:Thing as the root node
				navigateToNode('http://www.w3.org/2002/07/owl#Thing');
			}, 0);

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

	async function loadNodeFacts(nodeId) {
		loadingFacts = true;
		try {
			const factsJson = await invoke('get_node_facts', { nodeId: nodeId });
			nodeFacts = JSON.parse(factsJson);
		} catch (err) {
			console.error('Failed to load facts:', err);
			nodeFacts = [];
		} finally {
			loadingFacts = false;
		}
	}

	async function navigateToNode(nodeId) {
		currentNodeId = nodeId;

		// Load new graph data centered on this node
		try {
			const graphJson = await invoke('get_ontology_graph', {
				centralNodeId: nodeId
			});
			fullGraphData = JSON.parse(graphJson);

			// Find the central node
			const centralNode = fullGraphData.nodes.find((n) => n.id === nodeId);
			if (!centralNode) return;

			currentNodeLabel = centralNode.label;

			// Load facts for this node
			loadNodeFacts(nodeId);

			// Show all nodes and links from backend
			visibleGraphData = {
				nodes: fullGraphData.nodes,
				links: fullGraphData.links,
				centralNodeId: nodeId
			};
		} catch (err) {
			console.error('Failed to navigate to node:', err);
		}
	}

	function handleNodeClick(nodeId, nodeLabel) {
		navigateToNode(nodeId);
	}
</script>

<div id="graph-container">
	{#if loading}
		<div class="loading">Loading ontology graph...</div>
	{:else if error}
		<div class="error">Error: {error}</div>
	{:else}
		<TopBar onRecenter={recenterGraph} />

		{#if currentNodeId}
			<SidePanel
				{currentNodeLabel}
				{nodeFacts}
				{loadingFacts}
				onNavigateToNode={navigateToNode}
				{getNodeDisplayName}
			/>
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
		background: #000000;
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
		background: #000000;
	}

	.error {
		color: #ff6b9d;
	}
</style>
