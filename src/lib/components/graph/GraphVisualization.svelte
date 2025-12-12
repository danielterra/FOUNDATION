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
	let focusedNodeId = null;

	// D3 selections - need to be accessible for focus mode
	let nodes, links, linkLabelGroups, layoutLinks, layoutNodes;

	// OWL node types (matching backend group numbers)
	const NODE_TYPE = {
		CLASS: 1,           // owl:Class - represents a class/concept
		INDIVIDUAL: 6,      // Individual - instance of a class
		LITERAL: 7          // Literal value or datatype
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
		LITERAL: {
			nodeRadius: 14,
			borderColor: 'var(--color-neutral)',
			backgroundColor: 'var(--color-neutral)',
			textTransform: 'none',
			fontWeight: '400'
		},
		PROPERTY: {
			labelBackground: 'var(--color-black)',
			labelColor: 'var(--color-neutral)',
			linkColor: 'var(--color-neutral)',
			linkWidth: 1.5
		}
	};

	function getNodeVisualConfig(node) {
		if (node.group === NODE_TYPE.LITERAL) return OWL_VISUAL.LITERAL;
		if (node.group === NODE_TYPE.INDIVIDUAL) return OWL_VISUAL.INDIVIDUAL;
		return OWL_VISUAL.CLASS;
	}

	// Focus mode functions
	function applyFocusMode(nodeId) {
		focusedNodeId = nodeId;

		// Get connected node IDs
		const connectedNodeIds = new Set([nodeId]);
		if (layoutLinks) {
			layoutLinks.forEach(link => {
				const sourceId = link.source.id || link.source;
				const targetId = link.target.id || link.target;
				if (sourceId === nodeId) connectedNodeIds.add(targetId);
				if (targetId === nodeId) connectedNodeIds.add(sourceId);
			});
		}

		// Dim non-connected nodes
		if (nodes) {
			nodes.style('opacity', d => connectedNodeIds.has(d.id) ? 1 : 0.3);
		}

		// Dim non-connected links
		if (links) {
			links.style('opacity', d => {
				const sourceId = d.source.id || d.source;
				const targetId = d.target.id || d.target;
				return (sourceId === nodeId || targetId === nodeId) ? 1 : 0.3;
			});
		}

		// Dim non-connected link labels
		if (linkLabelGroups) {
			linkLabelGroups.style('opacity', d => {
				const sourceId = d.source.id || d.source;
				const targetId = d.target.id || d.target;
				return (sourceId === nodeId || targetId === nodeId) ? 1 : 0.3;
			});
		}
	}

	function clearFocusMode() {
		focusedNodeId = null;

		// Restore full opacity to all elements
		if (nodes) nodes.style('opacity', 1);
		if (links) links.style('opacity', 1);
		if (linkLabelGroups) linkLabelGroups.style('opacity', 1);
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

		// Handle ESC key to clear focus mode
		const handleKeyDown = (event) => {
			if (event.key === 'Escape' && focusedNodeId !== null) {
				clearFocusMode();
			}
		};

		updateSize();
		window.addEventListener('resize', updateSize);
		window.addEventListener('keydown', handleKeyDown);

		return () => {
			window.removeEventListener('resize', updateSize);
			window.removeEventListener('keydown', handleKeyDown);
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

		// Click on background to clear focus
		svgSelection.on('click', (event) => {
			// Only clear if clicking on SVG background (not on nodes/links)
			if (event.target === svg) {
				clearFocusMode();
			}
		});

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

		// Track visible nodes for dynamic rendering
		// Start with empty array, nodes will be added via callback
		let visibleNodes = [];

		// Variables for layout - will be set after createForceDirectedLayout
		let simulation;

		// Initialize link selections (assign to outer scope variables)
		links = linkGroup.selectAll('line');
		linkLabelGroups = linkGroup.selectAll('g.property-label');

		function updateLinks() {
			// Skip if layoutLinks is not initialized yet
			if (!layoutLinks) return;

			// Group links by directional pair (A->B is separate from B->A)
			const linksByDirection = new Map();
			layoutLinks.forEach((link, index) => {
				const sourceId = link.source.id || link.source;
				const targetId = link.target.id || link.target;
				const dirKey = `${sourceId}→${targetId}`;

				if (!linksByDirection.has(dirKey)) {
					linksByDirection.set(dirKey, []);
				}
				linksByDirection.get(dirKey).push({ link, index });
			});

			// Assign curve offset to each link
			const linksWithCurve = layoutLinks.map((link, index) => {
				const sourceId = link.source.id || link.source;
				const targetId = link.target.id || link.target;
				const dirKey = `${sourceId}→${targetId}`;
				const reverseKey = `${targetId}→${sourceId}`;

				const group = linksByDirection.get(dirKey);
				const hasReverse = linksByDirection.has(reverseKey);

				let curveOffset = 0;

				// If there are multiple links in same direction
				if (group.length > 1) {
					const position = group.findIndex(item => item.index === index);
					const totalLinks = group.length;
					const spacing = 30; // Increased spacing for better label separation
					curveOffset = (position - (totalLinks - 1) / 2) * spacing;
				}
				// If there's a reverse link (bidirectional), add curve to distinguish
				else if (hasReverse && group.length === 1) {
					curveOffset = 25; // Increased curve to separate from reverse direction
				}

				return { ...link, curveOffset };
			});

			// Update link paths (changed from line to path for curves)
			links = linkGroup
				.selectAll('path.link')
				.data(linksWithCurve, (d, i) => `${d.source.id || d.source}-${d.target.id || d.target}-${i}`);

			// Remove old links
			links.exit().remove();

			// Add new links
			const newLinks = links
				.enter()
				.append('path')
				.attr('class', 'link')
				.attr('stroke', OWL_VISUAL.PROPERTY.linkColor)
				.attr('stroke-width', OWL_VISUAL.PROPERTY.linkWidth)
				.attr('fill', 'none')
				.attr('marker-end', 'url(#arrowhead)');

			// Merge enter and update selections
			links = newLinks.merge(links);

			// Update link labels
			linkLabelGroups = linkGroup
				.selectAll('g.property-label')
				.data(linksWithCurve, (d, i) => `${d.source.id || d.source}-${d.target.id || d.target}-${i}`);

			// Remove old label groups
			linkLabelGroups.exit().remove();

			// Add new label groups
			const newLabelGroups = linkLabelGroups
				.enter()
				.append('g')
				.attr('class', 'property-label')
				.style('pointer-events', 'none');

			// Add background rectangle for each label
			newLabelGroups
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
			newLabelGroups
				.append('text')
				.text((d) => d.label || '')
				.attr('font-size', '11px')
				.attr('font-family', "'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif")
				.attr('fill', OWL_VISUAL.PROPERTY.labelColor)
				.attr('text-anchor', 'middle')
				.attr('dy', '0.3em')
				.style('user-select', 'none');

			// Merge enter and update selections
			linkLabelGroups = newLabelGroups.merge(linkLabelGroups);
		}

		// Links will be initialized when nodes are revealed (no initial call needed)

		// Drag behavior functions
		let dragStartPos = null;

		function dragstarted(event, d) {
			d3.select(this).style('cursor', 'grabbing');
			// Store start position to detect if it was a click or drag
			dragStartPos = { x: event.x, y: event.y };
			// Reheat simulation during drag
			if (!event.active) simulation.alphaTarget(0.3).restart();
		}

		function dragged(event, d) {
			// Update node position
			d.fx = event.x;
			d.fy = event.y;
		}

		function dragended(event, d) {
			d3.select(this).style('cursor', 'grab');
			// Cool down simulation after drag
			if (!event.active) simulation.alphaTarget(0);
			// Release node so simulation can reposition it
			d.fx = null;
			d.fy = null;

			// Check if this was a click (minimal movement)
			if (dragStartPos) {
				const dx = Math.abs(event.x - dragStartPos.x);
				const dy = Math.abs(event.y - dragStartPos.y);
				const wasClick = dx < 5 && dy < 5; // Less than 5px movement = click

				if (wasClick) {
					if (event.sourceEvent?.metaKey || event.sourceEvent?.ctrlKey) {
						// CMD+Click to toggle focus mode
						if (focusedNodeId === d.id) {
							clearFocusMode();
						} else {
							applyFocusMode(d.id);
						}
					} else if (onNodeClick) {
						// Regular click
						onNodeClick(d.id, d.label, d.icon);
					}
				}

				dragStartPos = null;
			}
		}

		// Tick handler function - updates positions on each simulation tick
		function onTick() {
			// Update links with curved paths
			links.attr('d', (d) => {
				const sourceConfig = getNodeVisualConfig(d.source);
				const targetConfig = getNodeVisualConfig(d.target);

				const dx = d.target.x - d.source.x;
				const dy = d.target.y - d.source.y;
				const dist = Math.sqrt(dx * dx + dy * dy);

				if (dist === 0) return ''; // Avoid division by zero

				// Perpendicular vector for curve control point
				const perpX = -dy / dist;
				const perpY = dx / dist;

				if (d.curveOffset === 0) {
					// Straight line: simple offset by radius
					const startX = d.source.x + (dx / dist) * sourceConfig.nodeRadius;
					const startY = d.source.y + (dy / dist) * sourceConfig.nodeRadius;
					const endX = d.target.x - (dx / dist) * targetConfig.nodeRadius;
					const endY = d.target.y - (dy / dist) * targetConfig.nodeRadius;

					return `M ${startX},${startY} L ${endX},${endY}`;
				} else {
					// Curved line: calculate control point first
					const midX = (d.source.x + d.target.x) / 2;
					const midY = (d.source.y + d.target.y) / 2;
					const controlX = midX + perpX * d.curveOffset;
					const controlY = midY + perpY * d.curveOffset;

					// Calculate tangent vectors at start and end of curve
					// For quadratic Bezier at t=0: tangent = 2(P1 - P0)
					// For quadratic Bezier at t=1: tangent = 2(P2 - P1)
					const startTangentX = 2 * (controlX - d.source.x);
					const startTangentY = 2 * (controlY - d.source.y);
					const startTangentDist = Math.sqrt(startTangentX * startTangentX + startTangentY * startTangentY);

					const endTangentX = 2 * (d.target.x - controlX);
					const endTangentY = 2 * (d.target.y - controlY);
					const endTangentDist = Math.sqrt(endTangentX * endTangentX + endTangentY * endTangentY);

					// Offset start/end points along tangent direction
					const startX = d.source.x + (startTangentX / startTangentDist) * sourceConfig.nodeRadius;
					const startY = d.source.y + (startTangentY / startTangentDist) * sourceConfig.nodeRadius;
					const endX = d.target.x - (endTangentX / endTangentDist) * targetConfig.nodeRadius;
					const endY = d.target.y - (endTangentY / endTangentDist) * targetConfig.nodeRadius;

					return `M ${startX},${startY} Q ${controlX},${controlY} ${endX},${endY}`;
				}
			});

			// Update link labels (position at curve midpoint)
			linkLabelGroups.attr('transform', (d) => {
				const dx = d.target.x - d.source.x;
				const dy = d.target.y - d.source.y;
				const dist = Math.sqrt(dx * dx + dy * dy);
				const midX = (d.source.x + d.target.x) / 2;
				const midY = (d.source.y + d.target.y) / 2;

				if (d.curveOffset === 0) {
					// Straight line: midpoint
					return `translate(${midX}, ${midY})`;
				} else {
					// Curved line: position label on the curve
					const perpX = -dy / dist;
					const perpY = dx / dist;

					// For quadratic bezier curve at t=0.5, the point is:
					// B(t) = (1-t)²P₀ + 2(1-t)tP₁ + t²P₂
					// At t=0.5: B(0.5) = 0.25*P₀ + 0.5*P₁ + 0.25*P₂
					// Control point P₁
					const controlX = midX + perpX * d.curveOffset;
					const controlY = midY + perpY * d.curveOffset;

					// Label position at t=0.5 on the curve
					let labelX = 0.25 * d.source.x + 0.5 * controlX + 0.25 * d.target.x;
					let labelY = 0.25 * d.source.y + 0.5 * controlY + 0.25 * d.target.y;

					// CRITICAL FIX: For vertical links, perpendicular is horizontal,
					// so controlY = midY (no change). Labels overlap vertically.
					// Solution: Add explicit vertical offset based on curveOffset sign

					// Check if link is more vertical than horizontal
					const isVertical = Math.abs(dy) > Math.abs(dx);

					if (isVertical && Math.abs(d.curveOffset) > 0) {
						// For vertical links with curve, shift labels vertically
						// Direction matters: if going DOWN (dy > 0), invert the offset
						const direction = dy > 0 ? -1 : 1; // Flip for downward links
						labelY += d.curveOffset * direction; // Apply directional offset
					}

					return `translate(${labelX}, ${labelY})`;
				}
			});

			// Update nodes
			nodes.attr('transform', (d) => `translate(${d.x},${d.y})`);
		}

		// Create drag behavior
		const drag = d3.drag()
			.on('start', dragstarted)
			.on('drag', dragged)
			.on('end', dragended);

		// Function to update nodes dynamically
		// Initialize nodes selection (assign to outer scope variable)
		nodes = nodeGroup.selectAll('g');

		function updateNodes() {
			// Update selection with current visible nodes
			nodes = nodeGroup
				.selectAll('g')
				.data(visibleNodes, d => d.id); // Use key function for proper data join

			// Remove old nodes (exit selection)
			nodes.exit().remove();

			// Add new nodes (enter selection)
			const newNodes = nodes
				.enter()
				.append('g')
				.attr('transform', (d) => `translate(${d.x},${d.y})`)
				.style('cursor', 'grab')
				.call(drag);

			// Add node visuals to new nodes
			createNodeVisuals(newNodes);

			// Merge enter and update selections
			nodes = newNodes.merge(nodes);
		}

		// Nodes will be initialized when revealed by the simulation (no initial call needed)

		// Helper to detect icon type
		const getIconType = (icon) => {
			if (!icon) return null;
			if (icon.startsWith('http://') || icon.startsWith('https://') ||
			    icon.startsWith('file://') || icon.startsWith('data:')) {
				return 'image';
			}
			return 'material-symbol';
		};

		// Function to add visual elements to nodes
		function createNodeVisuals(nodeSelection) {
			// Add icon or circle for each node (Class or Individual)
			nodeSelection.each(function(d) {
				const nodeGroup = d3.select(this);
				const config = getNodeVisualConfig(d);

				// Use warning color if this is a broken reference
				const iconColor = d.isBrokenRef ? 'var(--color-warning)' : config.borderColor;

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
							.style('border', `2px solid ${iconColor}`)
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
							.style('border', `2px solid ${iconColor}`)
							.style('border-radius', '50%')
							.style('pointer-events', 'none')
							.html(`<span class="material-symbols-outlined" style="font-size: 20px; color: ${iconColor}; pointer-events: none;">${d.icon}</span>`);
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
			nodeSelection
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
			nodeSelection
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
		}

		// Now create the force-directed layout
		const layoutResult = createForceDirectedLayout(
			data,
			width,
			height,
			(newNode) => {
				// Callback when node is revealed - add to visible nodes
				visibleNodes.push(newNode);
			}
		);

		// Assign to outer scope variables FIRST
		layoutNodes = layoutResult.nodes;
		layoutLinks = layoutResult.links;
		simulation = layoutResult.simulation;
		currentSimulation = simulation;

		// Now update nodes and links with all data available
		updateNodes();
		updateLinks();

		// Register tick handler
		simulation.on('tick', onTick);

		// Cleanup function
		const cleanup = () => {
			if (simulation) simulation.stop();
		};
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
