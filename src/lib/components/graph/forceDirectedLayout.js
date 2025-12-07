import * as d3 from 'd3';

/**
 * Creates a force-directed graph layout for ontology visualization
 * Central node stays in the middle, connected nodes arrange around it
 */
export function createForceDirectedLayout(data, width, height) {
	const { nodes, links, centralNodeId } = data;

	// Clone nodes to avoid mutating original data
	const graphNodes = nodes.map((node) => ({
		...node,
		// Mark the central node
		isCentral: node.id === centralNodeId
	}));

	// Clone links with source/target as IDs (d3 will replace with node objects)
	const graphLinks = links.map((link) => ({
		source: link.source,
		target: link.target,
		label: link.label
	}));

	// Analyze direction: mark incoming (left) and outgoing (right) nodes
	const centralId = graphNodes.find(n => n.isCentral)?.id;
	const nodeDirection = new Map();
	graphNodes.forEach(node => {
		nodeDirection.set(node.id, { isIncoming: false, isOutgoing: false });
	});

	// Detect direction from links relative to central node
	graphLinks.forEach(link => {
		const sourceId = typeof link.source === 'string' ? link.source : link.source.id;
		const targetId = typeof link.target === 'string' ? link.target : link.target.id;

		if (centralId) {
			// If link points TO central node, source is incoming (left)
			if (targetId === centralId && sourceId !== centralId) {
				if (nodeDirection.has(sourceId)) {
					nodeDirection.get(sourceId).isIncoming = true;
				}
			}
			// If link points FROM central node, target is outgoing (right)
			if (sourceId === centralId && targetId !== centralId) {
				if (nodeDirection.has(targetId)) {
					nodeDirection.get(targetId).isOutgoing = true;
				}
			}
		}
	});

	// Apply direction info to nodes
	graphNodes.forEach(node => {
		const direction = nodeDirection.get(node.id);
		if (direction) {
			node.isIncoming = direction.isIncoming;
			node.isOutgoing = direction.isOutgoing;
		}
	});

	// Custom force to position incoming left and outgoing right
	const directionForce = () => {
		graphNodes.forEach(node => {
			if (node.isCentral) return; // Don't move central node

			const targetX = node.isIncoming ? -250 : // Incoming on left
			                node.isOutgoing ? 250 :  // Outgoing on right
			                0; // Neutral stays at center

			// Apply gentle force towards target X position
			node.vx += (targetX - node.x) * 0.03;
		});
	};

	// Create force simulation
	const simulation = d3
		.forceSimulation(graphNodes)
		// Attract nodes to center
		.force('center', d3.forceCenter(0, 0))
		// Repel nodes from each other
		.force(
			'charge',
			d3.forceManyBody().strength((d) => (d.isCentral ? -1000 : -300))
		)
		// Spring force on links
		.force(
			'link',
			d3
				.forceLink(graphLinks)
				.id((d) => d.id)
				.distance(150)
		)
		// Prevent overlap
		.force(
			'collision',
			d3.forceCollide().radius((d) => (d.isCentral ? 40 : 30))
		)
		// Direction force to position incoming/outgoing horizontally
		.force('direction', directionForce);

	// Run simulation synchronously for initial layout
	simulation.stop();
	for (let i = 0; i < 300; i++) {
		simulation.tick();
	}

	// Fix central node at center
	const centralNode = graphNodes.find((n) => n.isCentral);
	if (centralNode) {
		centralNode.fx = 0;
		centralNode.fy = 0;
	}

	return {
		nodes: graphNodes,
		links: graphLinks,
		simulation
	};
}
