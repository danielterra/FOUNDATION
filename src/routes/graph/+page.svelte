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
	let nodeTriples = [];
	let nodeBacklinks = [];
	let nodeStatistics = null;
	let applicableProperties = [];
	let loadingTriples = false;
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

	// Reload graph data from backend
	async function reloadGraph() {
		try {
			loading = true;
			error = null;

			// Reload graph data centered on current node or owl:Thing if none selected
			const targetNodeId = currentNodeId || 'http://www.w3.org/2002/07/owl#Thing';
			const graphJson = await invoke('get_ontology_graph', {
				centralNodeId: targetNodeId
			});
			fullGraphData = JSON.parse(graphJson);

			loading = false;

			// Navigate to the same node to refresh triples and visualization
			if (currentNodeId) {
				await navigateToNode(currentNodeId);
			} else {
				await navigateToNode('http://www.w3.org/2002/07/owl#Thing');
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

	async function loadNodeTriples(nodeId) {
		loadingTriples = true;
		try {
			// Load triples, backlinks, statistics, and applicable properties in parallel
			const [triplesJson, backlinksJson, statisticsJson, propertiesJson] = await Promise.all([
				invoke('get_node_triples', { nodeId: nodeId }),
				invoke('get_node_backlinks', { nodeId: nodeId }),
				invoke('get_node_statistics', { nodeId: nodeId }),
				invoke('get_applicable_properties', { nodeId: nodeId })
			]);

			nodeTriples = JSON.parse(triplesJson);
			nodeBacklinks = JSON.parse(backlinksJson);
			nodeStatistics = JSON.parse(statisticsJson);
			applicableProperties = JSON.parse(propertiesJson);
		} catch (err) {
			console.error('Failed to load node data:', err);
			nodeTriples = [];
			nodeBacklinks = [];
			nodeStatistics = null;
			applicableProperties = [];
		} finally {
			loadingTriples = false;
		}
	}

	async function navigateToNode(nodeId) {
		// Load new graph data centered on this node
		try {
			const graphJson = await invoke('get_ontology_graph', {
				centralNodeId: nodeId
			});
			fullGraphData = JSON.parse(graphJson);

			// Use the canonical ID returned by backend (may differ due to equivalence)
			const canonicalId = fullGraphData.central_node_id;
			currentNodeId = canonicalId;

			// Find the central node using the canonical ID
			const centralNode = fullGraphData.nodes.find((n) => n.id === canonicalId);
			if (!centralNode) {
				console.error('Central node not found:', canonicalId);
				return;
			}

			currentNodeLabel = centralNode.label;

			// Load triples for the canonical ID
			loadNodeTriples(canonicalId);

			// Show all nodes and links from backend
			visibleGraphData = {
				nodes: fullGraphData.nodes,
				links: fullGraphData.links,
				centralNodeId: canonicalId
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
	<!-- Background Video -->
	<video
		autoplay
		loop
		muted
		playsinline
		class="background-video"
	>
		<source src="/background-space.mp4" type="video/mp4" />
	</video>

	{#if loading}
		<div class="loading">Loading ontology graph...</div>
	{:else if error}
		<div class="error">Error: {error}</div>
	{:else}
		<TopBar onRecenter={recenterGraph} />

		{#if currentNodeId}
			<SidePanel
				{currentNodeLabel}
				{nodeTriples}
				{nodeBacklinks}
				{nodeStatistics}
				{applicableProperties}
				{loadingTriples}
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
	}

	.background-video {
		position: fixed;
		top: 0;
		left: 0;
		width: 100vw;
		height: 100vh;
		object-fit: cover;
		z-index: 0;
		opacity: 0.2;
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
