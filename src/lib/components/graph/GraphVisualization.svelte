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

	// OWL node types (matching backend group numbers)
	const NODE_TYPE = {
		CLASS: 1,           // owl:Class - represents a class/concept
		INDIVIDUAL: 6       // Individual - instance of a class
	};

	// Visual configuration for OWL entities
	// DESIGN PRINCIPLE: var(--color-interactive) = Elementos interagíveis (clicáveis)
	const OWL_VISUAL = {
		CLASS: {
			nodeRadius: 18,
			borderColor: 'var(--color-interactive)',
			backgroundColor: 'var(--color-interactive)',  // Usará opacity no CSS inline
			textTransform: 'uppercase',
			fontWeight: 'bold'
		},
		INDIVIDUAL: {
			nodeRadius: 18,
			borderColor: 'var(--color-interactive)',
			backgroundColor: 'var(--color-interactive)',  // Usará opacity no CSS inline
			textTransform: 'none',
			fontWeight: '500'
		},
		PROPERTY: {
			labelBackground: 'var(--color-black)',
			labelColor: 'var(--color-neutral)',
			linkColor: 'var(--color-neutral)',
			linkWidth: 1.5
		}
	};

	function getNodeVisualConfig(node) {
		return node.group === NODE_TYPE.INDIVIDUAL ? OWL_VISUAL.INDIVIDUAL : OWL_VISUAL.CLASS;
	}

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

		// Create arrow markers for properties (ObjectProperty relationships)
		const defs = svgSelection.append('defs');

		defs
			.append('marker')
			.attr('id', 'arrowhead')
			.attr('viewBox', '0 -5 10 10')
			.attr('refX', 10)
			.attr('refY', 0)
			.attr('markerWidth', 6)
			.attr('markerHeight', 6)
			.attr('orient', 'auto')
			.append('path')
			.attr('d', 'M0,-5L10,0L0,5')
			.attr('fill', OWL_VISUAL.PROPERTY.linkColor);

		// Create link group (for ObjectProperty relationships)
		const linkGroup = g.append('g').attr('class', 'properties');

		// Create node group (for Classes and Individuals)
		const nodeGroup = g.append('g').attr('class', 'entities');

		// Center graph initially
		svgSelection.call(
			zoomBehavior.transform,
			d3.zoomIdentity.translate(width / 2, height / 2).scale(1)
		);

		// Create force-directed layout
		const { nodes: layoutNodes, links: layoutLinks, simulation } = createForceDirectedLayout(data, width, height);
		currentSimulation = simulation;

		// Debug: check if labels are present
		console.log('[GraphViz] Links with labels:', layoutLinks.map(l => ({
			source: l.source.id || l.source,
			target: l.target.id || l.target,
			label: l.label
		})));

		// Draw ObjectProperty links
		const links = linkGroup
			.selectAll('line')
			.data(layoutLinks)
			.enter()
			.append('line')
			.attr('stroke', OWL_VISUAL.PROPERTY.linkColor)
			.attr('stroke-width', OWL_VISUAL.PROPERTY.linkWidth)
			.attr('marker-end', 'url(#arrowhead)')
			.attr('x1', (d) => {
				const config = getNodeVisualConfig(d.source);
				const dx = d.target.x - d.source.x;
				const dy = d.target.y - d.source.y;
				const dist = Math.sqrt(dx * dx + dy * dy);
				return d.source.x + (dx / dist) * config.nodeRadius;
			})
			.attr('y1', (d) => {
				const config = getNodeVisualConfig(d.source);
				const dx = d.target.x - d.source.x;
				const dy = d.target.y - d.source.y;
				const dist = Math.sqrt(dx * dx + dy * dy);
				return d.source.y + (dy / dist) * config.nodeRadius;
			})
			.attr('x2', (d) => {
				const config = getNodeVisualConfig(d.target);
				const dx = d.target.x - d.source.x;
				const dy = d.target.y - d.source.y;
				const dist = Math.sqrt(dx * dx + dy * dy);
				return d.target.x - (dx / dist) * config.nodeRadius;
			})
			.attr('y2', (d) => {
				const config = getNodeVisualConfig(d.target);
				const dx = d.target.x - d.source.x;
				const dy = d.target.y - d.source.y;
				const dist = Math.sqrt(dx * dx + dy * dy);
				return d.target.y - (dy / dist) * config.nodeRadius;
			});

		// Draw property labels (ObjectProperty names)
		const linkLabelGroups = linkGroup
			.selectAll('g.property-label')
			.data(layoutLinks)
			.enter()
			.append('g')
			.attr('class', 'property-label')
			.attr('transform', (d) => `translate(${(d.source.x + d.target.x) / 2}, ${(d.source.y + d.target.y) / 2})`)
			.style('pointer-events', 'none');

		// Add background rectangle for each property label
		linkLabelGroups
			.append('rect')
			.attr('x', function(d) {
				const textLength = (d.label || '').length * 6;
				return -textLength / 2 - 4;
			})
			.attr('y', -10)
			.attr('width', function(d) {
				const textLength = (d.label || '').length * 6;
				return textLength + 8;
			})
			.attr('height', 16)
			.attr('rx', 3)
			.attr('fill', OWL_VISUAL.PROPERTY.labelBackground);

		// Add text on top of background
		const linkLabels = linkLabelGroups
			.append('text')
			.text((d) => d.label || '')
			.attr('font-size', '11px')
			.attr('font-family', "'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif")
			.attr('fill', OWL_VISUAL.PROPERTY.labelColor)
			.attr('text-anchor', 'middle')
			.attr('dy', '0.3em')
			.style('user-select', 'none');

		// Drag behavior functions
		function dragstarted(event, d) {
			d3.select(this).style('cursor', 'grabbing');
		}

		function dragged(event, d) {
			d.x = event.x;
			d.y = event.y;

			// Manually update this node's position
			d3.select(this).attr('transform', `translate(${d.x},${d.y})`);

			// Update connected links
			links.each(function(l) {
				if (l.source === d || l.target === d) {
					const link = d3.select(this);
					const sourceConfig = getNodeVisualConfig(l.source);
					const targetConfig = getNodeVisualConfig(l.target);
					const dx = l.target.x - l.source.x;
					const dy = l.target.y - l.source.y;
					const dist = Math.sqrt(dx * dx + dy * dy);

					link.attr('x1', l.source.x + (dx / dist) * sourceConfig.nodeRadius)
						.attr('y1', l.source.y + (dy / dist) * sourceConfig.nodeRadius)
						.attr('x2', l.target.x - (dx / dist) * targetConfig.nodeRadius)
						.attr('y2', l.target.y - (dy / dist) * targetConfig.nodeRadius);
				}
			});

			// Update connected link labels
			linkLabelGroups.each(function(l) {
				if (l.source === d || l.target === d) {
					d3.select(this).attr('transform',
						`translate(${(l.source.x + l.target.x) / 2}, ${(l.source.y + l.target.y) / 2})`
					);
				}
			});
		}

		function dragended(event, d) {
			d3.select(this).style('cursor', 'grab');
		}

		// Create drag behavior
		const drag = d3.drag()
			.on('start', dragstarted)
			.on('drag', dragged)
			.on('end', dragended);

		// Draw nodes (Classes and Individuals)
		const nodes = nodeGroup
			.selectAll('g')
			.data(layoutNodes)
			.enter()
			.append('g')
			.attr('transform', (d) => `translate(${d.x},${d.y})`)
			.style('cursor', 'grab')
			.call(drag)
			.on('click', (event, d) => {
				event.stopPropagation();
				if (onNodeClick) {
					onNodeClick(d.id, d.label);
				}
			});

		// Helper to detect icon type
		const getIconType = (icon) => {
			if (!icon) return null;
			if (icon.startsWith('http://') || icon.startsWith('https://') ||
			    icon.startsWith('file://') || icon.startsWith('data:')) {
				return 'image';
			}
			return 'material-symbol';
		};

		// Add icon or circle for each node (Class or Individual)
		nodes.each(function(d) {
			const nodeGroup = d3.select(this);
			const config = getNodeVisualConfig(d);

			if (d.icon) {
				const iconType = getIconType(d.icon);

				// Add dark opaque background circle to cover lines
				nodeGroup
					.append('circle')
					.attr('r', config.nodeRadius)
					.attr('fill', 'color-mix(in srgb, var(--color-black) 95%, transparent)')
					.attr('stroke', 'none')
					.style('pointer-events', 'none');

				if (iconType === 'image') {
					// Render image icon
					nodeGroup
						.append('foreignObject')
						.attr('x', -16)
						.attr('y', -16)
						.attr('width', 32)
						.attr('height', 32)
						.style('pointer-events', 'none')
						.append('xhtml:div')
						.style('width', '100%')
						.style('height', '100%')
						.style('display', 'flex')
						.style('align-items', 'center')
						.style('justify-content', 'center')
						.style('background', 'var(--color-black)')
						.style('border', `2px solid ${config.borderColor}`)
						.style('border-radius', '50%')
						.style('overflow', 'hidden')
						.style('pointer-events', 'none')
						.html(`<img src="${d.icon}" style="width: 24px; height: 24px; object-fit: cover; pointer-events: none;" />`);
				} else {
					// Render Material Symbols icon
					nodeGroup
						.append('foreignObject')
						.attr('x', -16)
						.attr('y', -16)
						.attr('width', 32)
						.attr('height', 32)
						.style('pointer-events', 'none')
						.append('xhtml:div')
						.style('width', '100%')
						.style('height', '100%')
						.style('display', 'flex')
						.style('align-items', 'center')
						.style('justify-content', 'center')
						.style('background', 'var(--color-black)')
						.style('border', `2px solid ${config.borderColor}`)
						.style('border-radius', '50%')
						.style('pointer-events', 'none')
						.html(`<span class="material-symbols-outlined" style="font-size: 20px; color: ${config.borderColor}; pointer-events: none;">${d.icon}</span>`);
				}

				// Add invisible circle for click events
				nodeGroup
					.append('circle')
					.attr('r', 16)
					.attr('fill', 'transparent')
					.attr('stroke', 'none')
					.style('cursor', 'pointer');
			} else {
				// Fallback to circle for nodes without icons
				nodeGroup
					.append('circle')
					.attr('r', 8)
					.attr('fill', config.borderColor)
					.attr('stroke', 'color-mix(in srgb, var(--color-black) 80%, transparent)')
					.attr('stroke-width', 2);
			}
		});

		// Add background rectangles for labels
		nodes
			.append('rect')
			.attr('x', 18)
			.attr('y', -10)
			.attr('width', (d) => {
				const config = getNodeVisualConfig(d);
				const label = config.textTransform === 'uppercase' ? d.label.toUpperCase() : d.label;
				return label.length * 8.5 + 8; // Approximate width based on character count
			})
			.attr('height', 20)
			.attr('rx', 4)
			.attr('fill', 'var(--color-black)')
			.style('pointer-events', 'none');

		// Add labels (Class names in UPPERCASE, Individual names in original case)
		nodes
			.append('text')
			.text((d) => {
				const config = getNodeVisualConfig(d);
				return config.textTransform === 'uppercase' ? d.label.toUpperCase() : d.label;
			})
			.attr('font-size', '14px')
			.attr('font-weight', (d) => getNodeVisualConfig(d).fontWeight)
			.attr('fill', 'var(--color-neutral)')
			.attr('dx', 24)
			.attr('dy', 5)
			.style('pointer-events', 'none')
			.style('user-select', 'none');

		// Fade in animation
		nodes.style('opacity', 0).transition().duration(500).style('opacity', 1);

		links.style('opacity', 0).transition().duration(500).style('opacity', 1);

		linkLabelGroups.style('opacity', 0).transition().duration(500).style('opacity', 1);
	}
</script>

<svg bind:this={svg} />

<style>
	svg {
		width: 100%;
		height: 100vh;
		background: transparent;
		display: block;
		position: relative;
		z-index: 1;
	}
</style>
