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
		target: link.target
	}));

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
		);

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
