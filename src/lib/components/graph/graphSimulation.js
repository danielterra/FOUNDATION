import * as d3 from 'd3';

/**
 * Creates a D3 force simulation with custom hierarchical forces
 * @param {Object} data - Graph data with nodes and links
 * @param {Function} options.onTick - Callback for each simulation tick
 * @param {Function} options.onEnd - Callback when simulation ends
 * @returns {Object} D3 force simulation
 */
export function createGraphSimulation(data, { onTick, onEnd } = {}) {
	// Build parent-child relationship maps
	const parentMap = new Map();
	const childrenMap = new Map();

	data.links.forEach((link) => {
		const sourceId = typeof link.source === 'object' ? link.source.id : link.source;
		const targetId = typeof link.target === 'object' ? link.target.id : link.target;

		if (link.label === 'subClassOf') {
			parentMap.set(sourceId, targetId);

			if (!childrenMap.has(targetId)) {
				childrenMap.set(targetId, []);
			}
			childrenMap.get(targetId).push(sourceId);
		}
	});

	// Calculate hierarchical levels
	function calculateLevel(nodeId, visited = new Set()) {
		if (visited.has(nodeId)) return 0;
		visited.add(nodeId);

		if (!parentMap.has(nodeId)) return 0;

		const parentId = parentMap.get(nodeId);
		return 1 + calculateLevel(parentId, visited);
	}

	// Assign levels to nodes
	data.nodes.forEach((node) => {
		node.level = calculateLevel(node.id);
	});

	const maxLevel = Math.max(...data.nodes.map((n) => n.level), 0);

	// Custom hierarchical force for parent-child and sibling attraction
	function hierarchicalForce(alpha) {
		const strength = 0.25 * alpha;

		data.nodes.forEach((nodeA) => {
			data.nodes.forEach((nodeB) => {
				if (nodeA === nodeB) return;

				const parentA = parentMap.get(nodeA.id);
				const parentB = parentMap.get(nodeB.id);

				// Parent-child attraction (strong)
				if (parentA === nodeB.id || parentB === nodeA.id) {
					const dx = nodeB.x - nodeA.x;
					const dy = nodeB.y - nodeA.y;
					const distance = Math.sqrt(dx * dx + dy * dy) || 1;

					const force = strength * 1.5; // Strong parent-child attraction
					nodeA.vx += (dx / distance) * force;
					nodeA.vy += (dy / distance) * force;
					nodeB.vx -= (dx / distance) * force;
					nodeB.vy -= (dy / distance) * force;
				}
				// Siblings (same parent) attract
				else if (parentA && parentB && parentA === parentB) {
					const dx = nodeB.x - nodeA.x;
					const dy = nodeB.y - nodeA.y;
					const distance = Math.sqrt(dx * dx + dy * dy) || 1;

					const force = strength * 0.8;
					nodeA.vx += (dx / distance) * force;
					nodeA.vy += (dy / distance) * force;
					nodeB.vx -= (dx / distance) * force;
					nodeB.vy -= (dy / distance) * force;
				}
				// Different clusters repel
				else if (nodeA.level === nodeB.level && parentA !== parentB) {
					const dx = nodeB.x - nodeA.x;
					const dy = nodeB.y - nodeA.y;
					const distance = Math.sqrt(dx * dx + dy * dy) || 1;

					const force = strength * 0.4;
					nodeA.vx -= (dx / distance) * force;
					nodeA.vy -= (dy / distance) * force;
					nodeB.vx += (dx / distance) * force;
					nodeB.vy += (dy / distance) * force;
				}
			});
		});
	}

	// Custom force to maintain standard distances between levels
	function levelDistanceForce(alpha) {
		const strength = 0.1 * alpha;

		data.links.forEach((link) => {
			if (link.label !== 'subClassOf') return;

			const source = typeof link.source === 'object' ? link.source : data.nodes.find((n) => n.id === link.source);
			const target = typeof link.target === 'object' ? link.target : data.nodes.find((n) => n.id === link.target);

			if (!source || !target) return;

			// Define standard distances based on level transitions
			let targetDistance = 150;
			if (target.level === 0 && source.level === 1) {
				targetDistance = 100; // Level 0 → 1
			} else if (target.level === 1 && source.level === 2) {
				targetDistance = 150; // Level 1 → 2
			} else if (target.level === 2 && source.level === 3) {
				targetDistance = 200; // Level 2 → 3
			}

			const dx = target.x - source.x;
			const dy = target.y - source.y;
			const distance = Math.sqrt(dx * dx + dy * dy) || 1;
			const diff = distance - targetDistance;

			const force = (diff * strength) / distance;

			source.vx += dx * force;
			source.vy += dy * force;
			target.vx -= dx * force;
			target.vy -= dy * force;
		});
	}

	// Create force simulation
	const simulation = d3
		.forceSimulation(data.nodes)
		.force(
			'link',
			d3
				.forceLink(data.links)
				.id((d) => d.id)
				.distance((link) => {
					if (link.label !== 'subClassOf') return 120;

					const source = typeof link.source === 'object' ? link.source : data.nodes.find((n) => n.id === link.source);
					const target = typeof link.target === 'object' ? link.target : data.nodes.find((n) => n.id === link.target);

					if (!source || !target) return 150;

					if (target.level === 0 && source.level === 1) return 120;
					if (target.level === 1 && source.level === 2) return 150;
					if (target.level === 2 && source.level === 3) return 180;

					return 150;
				})
				.strength(1)
		)
		.force('charge', d3.forceManyBody().strength(-500).distanceMax(600))
		.force('collision', d3.forceCollide().radius(70).strength(1).iterations(5))
		.force('center', d3.forceCenter(0, 0).strength(0.03))
		.force('hierarchical', hierarchicalForce)
		.alphaDecay(0.015)
		.velocityDecay(0.3);

	// Add event handlers
	if (onTick) {
		simulation.on('tick', onTick);
	}

	if (onEnd) {
		simulation.on('end', onEnd);
	}

	return simulation;
}
