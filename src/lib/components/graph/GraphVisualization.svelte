<script>
	import { onMount, onDestroy } from 'svelte';
	import * as d3 from 'd3';
	import { createForceDirectedLayout } from './forceDirectedLayout.js';

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
			const newWidth = window.innerWidth;
			const newHeight = window.innerHeight;

			// Only update SVG size, don't re-render entire graph
			if (svgSelection) {
				svgSelection.attr('width', newWidth).attr('height', newHeight);
			}

			width = newWidth;
			height = newHeight;
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

	// Only re-render when graphData changes, NOT when size changes
	$: if (svg && graphData) {
		// Initialize size if not set
		if (!width || !height) {
			width = window.innerWidth;
			height = window.innerHeight;
		}
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

		// Create force-directed layout
		const { nodes: layoutNodes, links: layoutLinks, simulation } = createForceDirectedLayout(data, width, height);
		currentSimulation = simulation;

		// Draw links
		const links = linkGroup
			.selectAll('line')
			.data(layoutLinks)
			.enter()
			.append('line')
			.attr('stroke', 'rgba(255, 255, 255, 0.5)')
			.attr('stroke-width', 2)
			.attr('marker-end', 'url(#arrowhead)')
			.attr('x1', (d) => d.source.x)
			.attr('y1', (d) => d.source.y)
			.attr('x2', (d) => d.target.x)
			.attr('y2', (d) => d.target.y);

		// Draw nodes
		const nodes = nodeGroup
			.selectAll('g')
			.data(layoutNodes)
			.enter()
			.append('g')
			.attr('transform', (d) => `translate(${d.x},${d.y})`)
			.style('cursor', 'pointer')
			.on('click', (event, d) => {
				event.stopPropagation();
				if (onNodeClick) {
					onNodeClick(d.id, d.label);
				}
			});

		nodes
			.append('circle')
			.attr('r', 8)
			.attr('fill', (d) => {
				const colors = {
					1: '#4a9eff', // RDF/RDFS/OWL - blue
					2: '#ff6b9d', // BFO - pink
					3: '#50fa7b', // Schema.org - green
					4: '#ffb86c', // FOAF - orange
					5: '#bd93f9' // Bridge - purple
				};
				return colors[d.group] || '#ffffff';
			})
			.attr('stroke', 'rgba(0, 0, 0, 0.8)')
			.attr('stroke-width', 2);

		nodes
			.append('text')
			.text((d) => d.label)
			.attr('font-size', '14px')
			.attr('font-weight', '500')
			.attr('font-family', "'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif")
			.attr('fill', 'rgba(255, 255, 255, 0.95)')
			.attr('dx', 12)
			.attr('dy', 5)
			.style('pointer-events', 'none')
			.style('user-select', 'none');

		// Fade in animation
		nodes.style('opacity', 0).transition().duration(500).style('opacity', 1);

		links.style('opacity', 0).transition().duration(500).style('opacity', 1);
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
