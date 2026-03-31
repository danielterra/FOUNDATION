import * as d3 from 'd3';

/**
 * Creates a force-directed graph layout for ontology visualization.
 * All nodes are equal — no central node concept.
 */
export function createForceDirectedLayout(data, width, height, onNodeRevealed) {
	const { nodes, links } = data;

	const graphNodes = nodes.map(node => ({ ...node }));

	const allLinks = links.map(link => ({
		source: link.source,
		target: link.target,
		label: link.label,
	}));

	const nodeIds = new Set(graphNodes.map(n => n.id));
	const graphLinks = allLinks.filter(link => {
		const sourceId = typeof link.source === 'string' ? link.source : link.source.id;
		const targetId = typeof link.target === 'string' ? link.target : link.target.id;
		return nodeIds.has(sourceId) && nodeIds.has(targetId);
	});

	const simulation = d3.forceSimulation(graphNodes)
		.force('link', d3.forceLink(graphLinks).id(d => d.id))
		.force('charge', d3.forceManyBody())
		.force('center', d3.forceCenter(0, 0))
		.force('collision', d3.forceCollide().radius(125));

	simulation.alpha(1).restart();

	if (onNodeRevealed) {
		graphNodes.forEach(n => onNodeRevealed(n));
	}

	return { nodes: graphNodes, links: graphLinks, simulation };
}
