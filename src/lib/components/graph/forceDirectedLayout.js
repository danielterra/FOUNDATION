import * as d3 from 'd3';

/**
 * Creates a force-directed graph layout for ontology visualization
 * Central node stays in the middle, other nodes are revealed gradually
 */
export function createForceDirectedLayout(data, width, height, onNodeRevealed) {
	const { nodes, links, centralNodeId } = data;

	// Separate central node from others
	const centralNode = nodes.find(n => n.id === centralNodeId);
	const otherNodes = nodes.filter(n => n.id !== centralNodeId);

	// Start with only central node
	const graphNodes = [{
		...centralNode,
		isCentral: true,
		fx: 0, // Fixed X at center
		fy: 0, // Fixed Y at center
	}];

	// Categorize nodes by relationship direction
	// Outgoing: central node is the source (links going OUT from central)
	// Incoming: central node is the target (links coming IN to central)
	// Bidirectional: nodes with both types of connections
	const outgoingNodeIds = new Set();
	const incomingNodeIds = new Set();
	const bidirectionalNodeIds = new Set();

	links.forEach(link => {
		if (link.source === centralNodeId) {
			outgoingNodeIds.add(link.target);
		} else if (link.target === centralNodeId) {
			incomingNodeIds.add(link.source);
		}
	});

	// Find bidirectional nodes (appear in both sets)
	outgoingNodeIds.forEach(nodeId => {
		if (incomingNodeIds.has(nodeId)) {
			bidirectionalNodeIds.add(nodeId);
		}
	});

	// Remove bidirectional from outgoing/incoming to keep them separate
	bidirectionalNodeIds.forEach(nodeId => {
		outgoingNodeIds.delete(nodeId);
		incomingNodeIds.delete(nodeId);
	});


	// Track nodes to be revealed with position based on direction
	const nodesToReveal = otherNodes.map(node => {
		let startY;
		if (bidirectionalNodeIds.has(node.id)) {
			// Bidirectional nodes start from right side (no Y bias)
			startY = 0;
		} else if (outgoingNodeIds.has(node.id)) {
			// Outgoing only nodes start from top
			startY = -height;
		} else if (incomingNodeIds.has(node.id)) {
			// Incoming only nodes start from bottom
			startY = height;
		} else {
			// Other nodes start from top
			startY = -height;
		}

		return {
			...node,
			isCentral: false,
			x: 0,
			y: startY
		};
	});

	// All links (we'll filter which ones to show)
	const allLinks = links.map((link) => ({
		source: link.source,
		target: link.target,
		label: link.label
	}));

	// Start with empty links array
	const graphLinks = [];

	// Create force simulation
	const simulation = d3.forceSimulation(graphNodes)
	.force("link", d3.forceLink(graphLinks).id(d => d.id).distance(graphNodes.length * 300).strength(1))
	.force(
		'collision',
		d3.forceCollide().radius(100).strength(1)
	)
    //   .force("charge", d3.forceManyBody().strength(10))
      .force("x", d3.forceX())
      .force("y", d3.forceY());

	// const simulation = d3
	// 	.forceSimulation(graphNodes)
	// 	.force('center', d3.forceCenter(0, 0))
	// 	.force(
	// 		'link',
	// 		d3
	// 			.forceLink(graphLinks)
	// 			.id((d) => d.id)
	// 			.strength(1)
	// 	)
	// 	.alphaDecay(0.09)
	// 	.velocityDecay(0.01);

	// Immediately reveal the central node
	if (onNodeRevealed) {
		onNodeRevealed(graphNodes[0]);
	}

	// Helper to get current node IDs
	const getCurrentNodeIds = () => new Set(graphNodes.map(n => n.id));

	// Add all nodes at once
	graphNodes.push(...nodesToReveal);
	simulation.nodes(graphNodes);

	// Add all valid links
	const nodeIds = getCurrentNodeIds();
	const validLinks = allLinks.filter(link => {
		// Handle both string IDs and object references (after D3 processes them)
		const sourceId = typeof link.source === 'string' ? link.source : link.source.id;
		const targetId = typeof link.target === 'string' ? link.target : link.target.id;
		return nodeIds.has(sourceId) && nodeIds.has(targetId);
	});

	graphLinks.push(...validLinks);
	simulation.force('link').links(graphLinks);

	// Start simulation
	simulation.alpha(1).restart();

	// Notify callback for all nodes
	if (onNodeRevealed) {
		nodesToReveal.forEach(node => onNodeRevealed(node));
	}

	return {
		nodes: graphNodes,
		links: graphLinks,
		simulation
	};
}
