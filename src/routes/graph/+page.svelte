<script>
	import { onMount } from 'svelte';
	import { invoke } from '@tauri-apps/api/core';
	import TopBar from '$lib/components/graph/TopBar.svelte';
	import SidePanel from '$lib/components/graph/SidePanel.svelte';
	import GraphVisualization from '$lib/components/graph/GraphVisualization.svelte';

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
			// Get graph data from Rust backend
			const graphJson = await invoke('get_ontology_graph');
			fullGraphData = JSON.parse(graphJson);

			loading = false;

			// Wait for next tick to ensure container dimensions are available
			setTimeout(() => {
				// Find the most fundamental node: one that is not a subclass of anything
				// AND has subclasses (to ensure it's connected)
				const hasSubClassOfOutgoing = new Set();
				const hasSubClassOfIncoming = {};

				fullGraphData.links.forEach((link) => {
					if (link.label === 'subClassOf') {
						const sourceId = typeof link.source === 'object' ? link.source.id : link.source;
						const targetId = typeof link.target === 'object' ? link.target.id : link.target;

						hasSubClassOfOutgoing.add(sourceId);
						hasSubClassOfIncoming[targetId] = (hasSubClassOfIncoming[targetId] || 0) + 1;
					}
				});

				// Find nodes that are NOT subclasses of anything (no outgoing subClassOf)
				// BUT have incoming subClassOf (are superclasses of something)
				const rootCandidates = fullGraphData.nodes.filter(
					(node) => !hasSubClassOfOutgoing.has(node.id) && hasSubClassOfIncoming[node.id] > 0
				);

				// Pick the one with most subclasses, or just pick the first node as fallback
				let rootNode = fullGraphData.nodes[0];
				if (rootCandidates.length > 0) {
					rootNode = rootCandidates.reduce((best, node) =>
						(hasSubClassOfIncoming[node.id] || 0) > (hasSubClassOfIncoming[best.id] || 0) ? node : best
					);
				}

				// Start with the root node
				navigateToNode(rootNode.id);
			}, 0);

			// Add window resize listener
			const handleResize = () => {
				if (currentNodeId) {
					navigateToNode(currentNodeId);
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

	function navigateToNode(nodeId) {
		currentNodeId = nodeId;

		// Find the central node
		const centralNode = fullGraphData.nodes.find((n) => n.id === nodeId);
		if (!centralNode) return;

		currentNodeLabel = centralNode.label;

		// Load facts for this node
		loadNodeFacts(nodeId);

		// Find nodes up to 2 levels away (depth: 2)
		const connectedNodeIds = new Set([nodeId]);
		const relevantLinks = [];

		// Level 1: Direct connections
		const level1Nodes = new Set();
		fullGraphData.links.forEach((link) => {
			if (link.source === nodeId || link.source.id === nodeId) {
				const targetId = typeof link.target === 'object' ? link.target.id : link.target;
				connectedNodeIds.add(targetId);
				level1Nodes.add(targetId);
				relevantLinks.push({
					source: nodeId,
					target: targetId,
					label: link.label
				});
			}
			if (link.target === nodeId || link.target.id === nodeId) {
				const sourceId = typeof link.source === 'object' ? link.source.id : link.source;
				connectedNodeIds.add(sourceId);
				level1Nodes.add(sourceId);
				relevantLinks.push({
					source: sourceId,
					target: nodeId,
					label: link.label
				});
			}
		});

		// Level 2: Connections from level 1 nodes
		fullGraphData.links.forEach((link) => {
			const sourceId = typeof link.source === 'object' ? link.source.id : link.source;
			const targetId = typeof link.target === 'object' ? link.target.id : link.target;

			// If source is in level 1, add its target
			if (level1Nodes.has(sourceId) && !connectedNodeIds.has(targetId)) {
				connectedNodeIds.add(targetId);
				relevantLinks.push({
					source: sourceId,
					target: targetId,
					label: link.label
				});
			}
			// If target is in level 1, add its source
			if (level1Nodes.has(targetId) && !connectedNodeIds.has(sourceId)) {
				connectedNodeIds.add(sourceId);
				relevantLinks.push({
					source: sourceId,
					target: targetId,
					label: link.label
				});
			}
		});

		// Get only the connected nodes
		const visibleNodes = fullGraphData.nodes.filter((n) => connectedNodeIds.has(n.id));

		// Update visible graph data with central node ID
		visibleGraphData = { nodes: visibleNodes, links: relevantLinks, centralNodeId: nodeId };
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
