<script>
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import * as d3 from 'd3';

  let svg;
  let width = 0;
  let height = 0;
  let loading = true;
  let error = null;

  onMount(async () => {
    try {
      // Get graph data from Rust backend
      const graphJson = await invoke('get_ontology_graph');
      const graphData = JSON.parse(graphJson);
      
      loading = false;
      
      // Wait for next tick to ensure container dimensions are available
      setTimeout(() => {
        renderGraph(graphData);
      }, 0);
    } catch (err) {
      error = err.toString();
      loading = false;
    }
  });

  function renderGraph(data) {
    // Get container dimensions
    const container = document.getElementById('graph-container');
    width = container.clientWidth;
    height = container.clientHeight;

    // Clear any existing SVG
    d3.select(svg).selectAll("*").remove();

    // Create SVG
    const svgElement = d3.select(svg)
      .attr('width', width)
      .attr('height', height);

    // Color scale for groups
    const color = d3.scaleOrdinal()
      .domain([1, 2, 3])
      .range(['#4299e1', '#48bb78', '#ed8936']); // blue, green, orange

    // Create force simulation
    const simulation = d3.forceSimulation(data.nodes)
      .force('link', d3.forceLink(data.links)
        .id(d => d.id)
        .distance(100))
      .force('charge', d3.forceManyBody().strength(-300))
      .force('center', d3.forceCenter(width / 2, height / 2))
      .force('collision', d3.forceCollide().radius(30));

    // Create container for zoom
    const g = svgElement.append('g');

    // Add zoom behavior
    const zoom = d3.zoom()
      .scaleExtent([0.1, 4])
      .on('zoom', (event) => {
        g.attr('transform', event.transform);
      });

    svgElement.call(zoom);

    // Create links
    const link = g.append('g')
      .selectAll('line')
      .data(data.links)
      .join('line')
      .attr('stroke', '#999')
      .attr('stroke-opacity', 0.6)
      .attr('stroke-width', 2);

    // Create link labels
    const linkLabel = g.append('g')
      .selectAll('text')
      .data(data.links)
      .join('text')
      .attr('font-size', 10)
      .attr('fill', '#666')
      .attr('text-anchor', 'middle')
      .text(d => d.label);

    // Create nodes
    const node = g.append('g')
      .selectAll('circle')
      .data(data.nodes)
      .join('circle')
      .attr('r', 8)
      .attr('fill', d => color(d.group))
      .attr('stroke', '#fff')
      .attr('stroke-width', 2)
      .call(drag(simulation));

    // Add titles (tooltips)
    node.append('title')
      .text(d => `${d.label}\n${d.id}`);

    // Create labels
    const label = g.append('g')
      .selectAll('text')
      .data(data.nodes)
      .join('text')
      .attr('font-size', 12)
      .attr('dx', 12)
      .attr('dy', 4)
      .text(d => d.label);

    // Update positions on each tick
    simulation.on('tick', () => {
      link
        .attr('x1', d => d.source.x)
        .attr('y1', d => d.source.y)
        .attr('x2', d => d.target.x)
        .attr('y2', d => d.target.y);

      linkLabel
        .attr('x', d => (d.source.x + d.target.x) / 2)
        .attr('y', d => (d.source.y + d.target.y) / 2);

      node
        .attr('cx', d => d.x)
        .attr('cy', d => d.y);

      label
        .attr('x', d => d.x)
        .attr('y', d => d.y);
    });

    // Drag behavior
    function drag(simulation) {
      function dragstarted(event) {
        if (!event.active) simulation.alphaTarget(0.3).restart();
        event.subject.fx = event.subject.x;
        event.subject.fy = event.subject.y;
      }

      function dragged(event) {
        event.subject.fx = event.x;
        event.subject.fy = event.y;
      }

      function dragended(event) {
        if (!event.active) simulation.alphaTarget(0);
        event.subject.fx = null;
        event.subject.fy = null;
      }

      return d3.drag()
        .on('start', dragstarted)
        .on('drag', dragged)
        .on('end', dragended);
    }
  }
</script>

<div class="container">
  <header>
    <h1>Ontology Graph Visualization</h1>
    <a href="/" class="back-link">← Back to Facts</a>
  </header>

  {#if loading}
    <div class="loading">Loading ontology graph...</div>
  {:else if error}
    <div class="error">Error loading graph: {error}</div>
  {:else}
    <div class="legend">
      <div class="legend-item">
        <span class="legend-color" style="background-color: #4299e1;"></span>
        <span>RDF/RDFS/OWL</span>
      </div>
      <div class="legend-item">
        <span class="legend-color" style="background-color: #48bb78;"></span>
        <span>BFO</span>
      </div>
      <div class="legend-item">
        <span class="legend-color" style="background-color: #ed8936;"></span>
        <span>CCO</span>
      </div>
    </div>
    <div id="graph-container">
      <svg bind:this={svg}></svg>
    </div>
  {/if}
</div>

<style>
  .container {
    width: 100vw;
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
    color: white;
    font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
  }

  header {
    padding: 1.5rem 2rem;
    display: flex;
    justify-content: space-between;
    align-items: center;
    background: rgba(0, 0, 0, 0.2);
  }

  h1 {
    margin: 0;
    font-size: 1.75rem;
    font-weight: 600;
  }

  .back-link {
    color: white;
    text-decoration: none;
    padding: 0.5rem 1rem;
    background: rgba(255, 255, 255, 0.1);
    border-radius: 0.5rem;
    transition: background 0.2s;
  }

  .back-link:hover {
    background: rgba(255, 255, 255, 0.2);
  }

  .loading, .error {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    font-size: 1.25rem;
  }

  .error {
    color: #fc8181;
  }

  .legend {
    display: flex;
    gap: 2rem;
    padding: 1rem 2rem;
    background: rgba(0, 0, 0, 0.1);
    justify-content: center;
  }

  .legend-item {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .legend-color {
    width: 20px;
    height: 20px;
    border-radius: 50%;
    border: 2px solid white;
  }

  #graph-container {
    flex: 1;
    position: relative;
    overflow: hidden;
  }

  svg {
    background: white;
  }
</style>
