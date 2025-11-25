import * as d3 from 'd3';

/**
 * Creates a radial tree layout for ontology visualization
 * Based on: https://observablehq.com/@d3/radial-tree/2
 */
export function createRadialTree(data, width, height) {
	// Convert flat graph data to hierarchical structure
	const hierarchy = buildHierarchy(data);

	// Create radial tree layout with dynamic radius based on node count
	const nodeCount = data.nodes?.length || 1;
	// Moderate radius scaling for balanced layout
	const baseRadius = Math.min(width, height) / 2 - 80;
	const scaleFactor = Math.max(0.8, Math.sqrt(nodeCount / 20)); // Reduced scaling
	const radius = baseRadius * scaleFactor;

	const tree = d3
		.tree()
		.size([2 * Math.PI, radius])
		.separation((a, b) => (a.parent == b.parent ? 1.5 : 2.5) / a.depth); // Moderate separation

	const root = tree(d3.hierarchy(hierarchy));

	// Prepare nodes and links for D3
	const nodes = root.descendants().map((d) => ({
		id: d.data.id,
		label: d.data.label,
		group: d.data.group,
		x: d.x, // angle in radians
		y: d.y, // distance from center
		depth: d.depth
	}));

	const links = root.links().map((d) => ({
		source: {
			id: d.source.data.id,
			x: d.source.x,
			y: d.source.y
		},
		target: {
			id: d.target.data.id,
			x: d.target.x,
			y: d.target.y
		}
	}));

	console.log(`[RadialTree] Returning ${nodes.length} nodes and ${links.length} links`);

	return { nodes, links, radius };
}

/**
 * Converts flat graph structure to hierarchical tree
 */
function buildHierarchy(data) {
	const { nodes, links, centralNodeId } = data;

	// Create node data lookup
	const nodeData = new Map();
	nodes.forEach((node) => {
		nodeData.set(node.id, {
			id: node.id,
			label: node.label,
			group: node.group
		});
	});

	// Find central node (root)
	const rootId = centralNodeId || nodes[0]?.id;
	const rootData = nodeData.get(rootId);

	if (!rootData) {
		console.error('[RadialTree] Root node not found:', rootId);
		return { id: 'unknown', label: 'Unknown', group: 1, children: [] };
	}

	console.log(`[RadialTree] Building tree for root: ${rootData.label} (${rootId})`);
	console.log(`[RadialTree] Total nodes: ${nodes.length}, Total links: ${links.length}`);

	// Build parent-child relationship maps
	const childrenMap = new Map(); // parentId -> [childIds]
	const parentsMap = new Map();  // childId -> [parentIds]

	links.forEach((link) => {
		const parentId = link.target;
		const childId = link.source;

		// Children map
		if (!childrenMap.has(parentId)) {
			childrenMap.set(parentId, []);
		}
		if (!childrenMap.get(parentId).includes(childId)) {
			childrenMap.get(parentId).push(childId);
		}

		// Parents map
		if (!parentsMap.has(childId)) {
			parentsMap.set(childId, []);
		}
		if (!parentsMap.get(childId).includes(parentId)) {
			parentsMap.get(childId).push(parentId);
		}
	});

	// Debug: Check if root has children or parents
	const rootChildren = childrenMap.get(rootId) || [];
	const rootParents = parentsMap.get(rootId) || [];
	console.log(`[RadialTree] Root node "${rootData.label}" has ${rootChildren.length} children and ${rootParents.length} parents`);

	// If root has no children but has parents, invert the tree to show parents above
	let actualRootId = rootId;
	let centralIsLeaf = false;
	if (rootChildren.length === 0 && rootParents.length > 0) {
		// Use the first parent as the actual root of the tree
		actualRootId = rootParents[0];
		centralIsLeaf = true;
		console.log(`[RadialTree] Inverting tree: using parent "${nodeData.get(actualRootId)?.label}" as root`);
	}

	// Recursively build tree (avoiding cycles)
	function buildNode(nodeId, visited = new Set()) {
		if (visited.has(nodeId)) {
			// Cycle detected, return leaf node
			const data = nodeData.get(nodeId);
			if (!data) return null;
			return {
				id: data.id,
				label: data.label + ' (ref)',
				group: data.group,
				children: []
			};
		}

		visited.add(nodeId);

		const data = nodeData.get(nodeId);
		if (!data) return null;

		const node = {
			id: data.id,
			label: data.label,
			group: data.group,
			children: []
		};

		// Add children
		const childrenIds = childrenMap.get(nodeId) || [];
		for (const childId of childrenIds) {
			const childNode = buildNode(childId, new Set(visited));
			if (childNode) {
				node.children.push(childNode);
			}
		}

		return node;
	}

	// Build the tree starting from the actual root
	return buildNode(actualRootId);
}

/**
 * Projects radial coordinates to Cartesian coordinates
 */
export function project(x, y) {
	const angle = x - Math.PI / 2; // rotate by -90 degrees
	return [y * Math.cos(angle), y * Math.sin(angle)];
}

/**
 * Creates a radial link path
 */
export function linkRadial(d) {
	return d3
		.linkRadial()
		.angle((d) => d.x)
		.radius((d) => d.y)({
		source: d.source,
		target: d.target
	});
}
