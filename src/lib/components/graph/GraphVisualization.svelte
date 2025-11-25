<script>
	import { onMount, onDestroy } from 'svelte';
	import * as d3 from 'd3';
	import { createGraphSimulation } from './graphSimulation.js';

	export let graphData;
	export let onNodeClick;

	let svg;
	let width = 0;
	let height = 0;
	let svgSelection = null;
	let zoomBehavior = null;
	let currentSimulation = null;

	onMount(() => {
		const updateSize = () => {
			width = window.innerWidth;
			height = window.innerHeight;
		};

		updateSize();
		window.addEventListener('resize', updateSize);

		return () => {
			window.removeEventListener('resize', updateSize);
		};
	});

	onDestroy(() => {
		if (currentSimulation) {
			currentSimulation.stop();
		}
	});

	$: if (svg && graphData && width && height) {
		renderGraph(graphData);
	}

	export function recenter() {
		if (!svgSelection || !zoomBehavior) return;

		svgSelection
			.transition()
			.duration(750)
			.call(zoomBehavior.transform, d3.zoomIdentity.translate(width / 2, height / 2).scale(1));
	}

	async function renderGraph(data) {
		if (!svg) return;

		// Stop previous simulation
		if (currentSimulation) {
			currentSimulation.stop();
		}

		// Unfix all nodes to allow repositioning (except central node)
		data.nodes.forEach((node) => {
			if (!data.centralNodeId || node.id !== data.centralNodeId) {
				node.fx = null;
				node.fy = null;
			}
		});

		// Clear previous content
		d3.select(svg).selectAll('*').remove();

		// Create SVG container
		svgSelection = d3.select(svg).attr('width', width).attr('height', height);

		// Create zoom behavior
		zoomBehavior = d3
			.zoom()
			.scaleExtent([0.1, 4])
			.on('zoom', (event) => {
				g.attr('transform', event.transform);
			});

		svgSelection.call(zoomBehavior);

		// Create main group for zoom/pan
		const g = svgSelection.append('g');

		// Create arrow markers
		const defs = svgSelection.append('defs');

		defs
			.append('marker')
			.attr('id', 'arrowhead')
			.attr('viewBox', '0 -5 10 10')
			.attr('refX', 28)
			.attr('refY', 0)
			.attr('markerWidth', 6)
			.attr('markerHeight', 6)
			.attr('orient', 'auto')
			.append('path')
			.attr('d', 'M0,-5L10,0L0,5')
			.attr('fill', 'rgba(255, 255, 255, 0.3)');

		// Create link group
		const linkGroup = g.append('g');

		// Create node group
		const nodeGroup = g.append('g');

		// Center graph initially
		svgSelection.call(
			zoomBehavior.transform,
			d3.zoomIdentity.translate(width / 2, height / 2).scale(1)
		);

		// Start with empty data for incremental addition
		const incrementalNodes = [];
		const incrementalLinks = [];

		// Create simulation with empty data initially
		currentSimulation = createGraphSimulation(
			{ nodes: incrementalNodes, links: incrementalLinks },
			{
				onTick: () => {
					linkGroup
						.selectAll('line')
						.attr('x1', (d) => d.source.x)
						.attr('y1', (d) => d.source.y)
						.attr('x2', (d) => d.target.x)
						.attr('y2', (d) => d.target.y);

					nodeGroup.selectAll('g').attr('transform', (d) => `translate(${d.x},${d.y})`);
				}
			}
		);

		// Build parent-child relationship map
		const parentMap = new Map();
		const childrenMap = new Map();

		data.links.forEach((link) => {
			if (link.label === 'subClassOf') {
				const sourceId = typeof link.source === 'object' ? link.source.id : link.source;
				const targetId = typeof link.target === 'object' ? link.target.id : link.target;

				parentMap.set(sourceId, targetId);

				if (!childrenMap.has(targetId)) {
					childrenMap.set(targetId, []);
				}
				childrenMap.get(targetId).push(sourceId);
			}
		});

		// Calculate hierarchical level for each node
		function calculateLevel(nodeId, visited = new Set()) {
			if (visited.has(nodeId)) return 0;
			visited.add(nodeId);

			if (!parentMap.has(nodeId)) return 0;

			const parentId = parentMap.get(nodeId);
			return 1 + calculateLevel(parentId, visited);
		}

		// Assign levels to nodes
		const nodesByLevel = new Map();
		data.nodes.forEach((node) => {
			node.level = calculateLevel(node.id);
			if (!nodesByLevel.has(node.level)) {
				nodesByLevel.set(node.level, []);
			}
			nodesByLevel.get(node.level).push(node);
		});

		// Sort levels to add from root to leaves
		const sortedLevels = Array.from(nodesByLevel.keys()).sort((a, b) => a - b);

		// Add central node first
		const centralNode = data.nodes.find((n) => n.id === data.centralNodeId);
		if (centralNode) {
			centralNode.x = 0;
			centralNode.y = 0;
			centralNode.fx = 0;
			centralNode.fy = 0;
			incrementalNodes.push(centralNode);

			// Add central node to DOM
			const nodeEnter = nodeGroup
				.selectAll('g')
				.data(incrementalNodes, (d) => d.id)
				.enter()
				.append('g')
				.style('cursor', 'pointer')
				.style('opacity', 0)
				.on('click', (event, d) => {
					event.stopPropagation();
					if (onNodeClick) {
						onNodeClick(d.id, d.label);
					}
				});

			nodeEnter
				.append('circle')
				.attr('r', 8)
				.attr('fill', (d) => {
					const colors = { 1: '#4a9eff', 2: '#ff6b9d', 3: '#50fa7b' };
					return colors[d.group] || '#ffffff';
				})
				.attr('stroke', 'rgba(0, 0, 0, 0.8)')
				.attr('stroke-width', 2);

			nodeEnter
				.append('text')
				.text((d) => d.label)
				.attr('font-size', '11px')
				.attr('font-family', "'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif")
				.attr('fill', 'rgba(255, 255, 255, 0.9)')
				.attr('dx', 12)
				.attr('dy', 4)
				.style('pointer-events', 'none')
				.style('user-select', 'none');

			nodeEnter.transition().duration(300).style('opacity', 1);

			currentSimulation.nodes(incrementalNodes);
			currentSimulation.alpha(0.3).restart();
		}

		// Add nodes level by level with 200ms delay between each node
		let nodeIndexInLevel = 0;
		for (const level of sortedLevels) {
			const nodesAtLevel = nodesByLevel.get(level);
			const levelRadius = 150 + level * 100; // Increase radius per level
			nodeIndexInLevel = 0;

			for (const nodeData of nodesAtLevel) {
				if (nodeData.id === data.centralNodeId) continue; // Skip central node, already added

				await new Promise((resolve) => setTimeout(resolve, 200));

				// Initialize node position based on parent or in circle for this level
				const parentId = parentMap.get(nodeData.id);
				const parentNode = incrementalNodes.find((n) => n.id === parentId);

				if (parentNode) {
					// Position near parent
					const angle = (nodeIndexInLevel / nodesAtLevel.length) * 2 * Math.PI;
					const offsetRadius = 100;
					nodeData.x = parentNode.x + Math.cos(angle) * offsetRadius;
					nodeData.y = parentNode.y + Math.sin(angle) * offsetRadius;
				} else {
					// Position in circle at level radius
					const angle = (nodeIndexInLevel / nodesAtLevel.length) * 2 * Math.PI;
					nodeData.x = Math.cos(angle) * levelRadius;
					nodeData.y = Math.sin(angle) * levelRadius;
				}

				nodeData.vx = 0;
				nodeData.vy = 0;
				nodeIndexInLevel++;

			// Add node to incremental array
			incrementalNodes.push(nodeData);

			// Add relevant links for this node
			const relevantLinks = data.links.filter((link) => {
				const sourceId = typeof link.source === 'object' ? link.source.id : link.source;
				const targetId = typeof link.target === 'object' ? link.target.id : link.target;
				const sourceExists = incrementalNodes.some((n) => n.id === sourceId);
				const targetExists = incrementalNodes.some((n) => n.id === targetId);
				return sourceExists && targetExists;
			});

			// Update links array
			incrementalLinks.length = 0;
			incrementalLinks.push(...relevantLinks);

			// Update link visualization
			const links = linkGroup
				.selectAll('line')
				.data(incrementalLinks, (d) => `${d.source.id || d.source}-${d.target.id || d.target}`);

			links.exit().remove();

			links
				.enter()
				.append('line')
				.attr('stroke', 'rgba(255, 255, 255, 0.3)')
				.attr('stroke-width', 1.5)
				.attr('marker-end', 'url(#arrowhead)');

			// Update node visualization
			const nodes = nodeGroup.selectAll('g').data(incrementalNodes, (d) => d.id);

			nodes.exit().remove();

			const nodeEnter = nodes
				.enter()
				.append('g')
				.style('cursor', 'pointer')
				.style('opacity', 0)
				.on('click', (event, d) => {
					event.stopPropagation();
					if (onNodeClick) {
						onNodeClick(d.id, d.label);
					}
				});

			nodeEnter
				.append('circle')
				.attr('r', 8)
				.attr('fill', (d) => {
					const colors = { 1: '#4a9eff', 2: '#ff6b9d', 3: '#50fa7b' };
					return colors[d.group] || '#ffffff';
				})
				.attr('stroke', 'rgba(0, 0, 0, 0.8)')
				.attr('stroke-width', 2);

			nodeEnter
				.append('text')
				.text((d) => d.label)
				.attr('font-size', '11px')
				.attr('font-family', "'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif")
				.attr('fill', 'rgba(255, 255, 255, 0.9)')
				.attr('dx', 12)
				.attr('dy', 4)
				.style('pointer-events', 'none')
				.style('user-select', 'none');

			nodeEnter.transition().duration(300).style('opacity', 1);

			// Update simulation with new data
			currentSimulation.nodes(incrementalNodes);
			currentSimulation.force('link').links(incrementalLinks);
			currentSimulation.alpha(0.3).restart();
			}
		}
	}
</script>

<svg bind:this={svg} />

<style>
	svg {
		width: 100%;
		height: 100vh;
		background: #000000;
		display: block;
	}
</style>
