<script>
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import * as d3 from 'd3';

  let svg;
  let width = 0;
  let height = 0;
  let loading = true;
  let error = null;
  let fullGraphData = null;
  let currentNodeId = null;
  let currentNodeLabel = '';
  let nodeFacts = [];
  let loadingFacts = false;

  // Get display name for a node (use label if available, otherwise simplify URI)
  function getNodeDisplayName(nodeId) {
    const node = fullGraphData?.nodes.find(n => n.id === nodeId);
    if (node) {
      return node.label;
    }
    // Fallback: simplify URI
    return nodeId.split(/[/#]/).pop();
  }

  onMount(async () => {
    try {
      // Get graph data from Rust backend
      const graphJson = await invoke('get_ontology_graph');
      fullGraphData = JSON.parse(graphJson);

      loading = false;

      // Wait for next tick to ensure container dimensions are available
      setTimeout(() => {
        // Find the most fundamental node: one that is not a subclass of anything
        // AND has subclasses (to ensure it's connected)
        const hasSubClassOfOutgoing = new Set();
        const hasSubClassOfIncoming = {};

        fullGraphData.links.forEach(link => {
          if (link.label === 'subClassOf') {
            const sourceId = typeof link.source === 'object' ? link.source.id : link.source;
            const targetId = typeof link.target === 'object' ? link.target.id : link.target;

            hasSubClassOfOutgoing.add(sourceId);
            hasSubClassOfIncoming[targetId] = (hasSubClassOfIncoming[targetId] || 0) + 1;
          }
        });

        // Find nodes that are NOT subclasses of anything (no outgoing subClassOf)
        // BUT have incoming subClassOf (are superclasses of something)
        const rootCandidates = fullGraphData.nodes.filter(node =>
          !hasSubClassOfOutgoing.has(node.id) && hasSubClassOfIncoming[node.id] > 0
        );

        // Pick the one with most subclasses, or just pick the first node as fallback
        let rootNode = fullGraphData.nodes[0];
        if (rootCandidates.length > 0) {
          rootNode = rootCandidates.reduce((best, node) =>
            (hasSubClassOfIncoming[node.id] || 0) > (hasSubClassOfIncoming[best.id] || 0) ? node : best
          );
        }

        // Start with the root node
        navigateToNode(rootNode.id);
      }, 0);

      // Add window resize listener
      const handleResize = () => {
        if (currentNodeId) {
          navigateToNode(currentNodeId);
        }
      };
      window.addEventListener('resize', handleResize);

      // Cleanup on unmount
      return () => {
        window.removeEventListener('resize', handleResize);
      };
    } catch (err) {
      error = err.toString();
      loading = false;
    }
  });

  async function loadNodeFacts(nodeId) {
    loadingFacts = true;
    try {
      const factsJson = await invoke('get_node_facts', { nodeId: nodeId });
      nodeFacts = JSON.parse(factsJson);
    } catch (err) {
      console.error('Failed to load facts:', err);
      nodeFacts = [];
    } finally {
      loadingFacts = false;
    }
  }

  function navigateToNode(nodeId) {
    currentNodeId = nodeId;

    // Find the central node
    const centralNode = fullGraphData.nodes.find(n => n.id === nodeId);
    if (!centralNode) return;

    currentNodeLabel = centralNode.label;

    // Load facts for this node
    loadNodeFacts(nodeId);

    // Find all directly connected nodes
    const connectedNodeIds = new Set([nodeId]);
    const relevantLinks = [];

    fullGraphData.links.forEach(link => {
      if (link.source === nodeId || link.source.id === nodeId) {
        const targetId = typeof link.target === 'object' ? link.target.id : link.target;
        connectedNodeIds.add(targetId);
        relevantLinks.push({
          source: nodeId,
          target: targetId,
          label: link.label
        });
      }
      if (link.target === nodeId || link.target.id === nodeId) {
        const sourceId = typeof link.source === 'object' ? link.source.id : link.source;
        connectedNodeIds.add(sourceId);
        relevantLinks.push({
          source: sourceId,
          target: nodeId,
          label: link.label
        });
      }
    });

    // Get only the connected nodes
    const visibleNodes = fullGraphData.nodes.filter(n => connectedNodeIds.has(n.id));

    renderGraph({ nodes: visibleNodes, links: relevantLinks });
  }

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

    // Color scale for groups (white non-interactive nodes)
    const color = d3.scaleOrdinal()
      .domain([1, 2, 3])
      .range(['#cccccc', '#cccccc', '#cccccc']); // white for all non-central nodes

    // Create force simulation with stronger repulsion and spacing
    const simulation = d3.forceSimulation(data.nodes)
      .force('link', d3.forceLink(data.links)
        .id(d => d.id)
        .distance(200)  // Increased from 100
        .strength(0.5))
      .force('charge', d3.forceManyBody().strength(-800))  // Increased from -300
      .force('center', d3.forceCenter(width / 2, height / 2))
      .force('collision', d3.forceCollide().radius(80));  // Increased from 30

    // Create container for zoom
    const g = svgElement.append('g');

    // Add zoom behavior
    const zoom = d3.zoom()
      .scaleExtent([0.1, 4])
      .on('zoom', (event) => {
        g.attr('transform', event.transform);
      });

    svgElement.call(zoom);

    // Define arrow marker
    svgElement.append('defs').append('marker')
      .attr('id', 'arrowhead')
      .attr('viewBox', '-0 -5 10 10')
      .attr('refX', 25)
      .attr('refY', 0)
      .attr('orient', 'auto')
      .attr('markerWidth', 8)
      .attr('markerHeight', 8)
      .append('svg:path')
      .attr('d', 'M 0,-5 L 10 ,0 L 0,5')
      .attr('fill', '#666')
      .attr('stroke', 'none');

    // Create links
    const link = g.append('g')
      .selectAll('line')
      .data(data.links)
      .join('line')
      .attr('stroke', '#666')
      .attr('stroke-opacity', 0.4)
      .attr('stroke-width', 2)
      .attr('marker-end', 'url(#arrowhead)');

    // Create link labels
    const linkLabel = g.append('g')
      .selectAll('text')
      .data(data.links)
      .join('text')
      .attr('font-size', 10)
      .attr('fill', '#999')
      .attr('text-anchor', 'middle')
      .text(d => d.label);

    // Create nodes
    const node = g.append('g')
      .selectAll('circle')
      .data(data.nodes)
      .join('circle')
      .attr('r', d => d.id === currentNodeId ? 16 : 10)
      .attr('fill', d => d.id === currentNodeId ? '#ff8c42' : color(d.group))
      .attr('stroke', d => d.id === currentNodeId ? '#ffaa66' : '#444')
      .attr('stroke-width', d => d.id === currentNodeId ? 4 : 2)
      .style('cursor', 'pointer')
      .on('click', (event, d) => {
        navigateToNode(d.id);
      })
      .call(drag(simulation));

    // Add titles (tooltips)
    node.append('title')
      .text(d => `${d.label}\nClick to navigate\n${d.id}`);

    // Create labels
    const label = g.append('g')
      .selectAll('text')
      .data(data.nodes)
      .join('text')
      .attr('font-size', d => d.id === currentNodeId ? 20 : 16)
      .attr('font-weight', d => d.id === currentNodeId ? 'bold' : 'normal')
      .attr('dx', d => d.id === currentNodeId ? 20 : 14)
      .attr('dy', d => d.id === currentNodeId ? 6 : 5)
      .attr('fill', d => d.id === currentNodeId ? '#ff8c42' : '#fff')
      .attr('stroke', '#2a2a2a')
      .attr('stroke-width', 3)
      .attr('paint-order', 'stroke')
      .style('pointer-events', 'none')
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
    <div class="info-bar">
      <div class="current-node">
        <span class="label">Current Node:</span>
        <span class="value">{currentNodeLabel}</span>
      </div>
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
        <div class="legend-item">
          <span class="legend-color" style="background-color: #ffd700; border-color: #ff6b6b;"></span>
          <span>Central Node</span>
        </div>
      </div>
    </div>
    <div class="main-content">
      <div id="graph-container">
        <svg bind:this={svg}></svg>
      </div>

      <aside class="side-panel">
        <div class="panel-header">
          <h3>Node Facts</h3>
          {#if nodeFacts.length > 0}
            <span class="fact-count">{nodeFacts.length} facts</span>
          {/if}
        </div>

        {#if loadingFacts}
          <div class="panel-loading">Loading facts...</div>
        {:else if nodeFacts.length === 0}
          <div class="panel-empty">No facts found for this node</div>
        {:else}
          <div class="facts-form">
            {#each nodeFacts as fact}
              <div class="form-row">
                <label class="form-label">{fact.a.split(/[/#]/).pop()}</label>
                <div class="form-value">
                  {#if fact.v_type === 'ref' && fullGraphData?.nodes.some(n => n.id === fact.v)}
                    <button class="entity-link" on:click={() => navigateToNode(fact.v)}>
                      {getNodeDisplayName(fact.v)}
                    </button>
                  {:else}
                    {fact.v}
                  {/if}
                </div>
              </div>
            {/each}
          </div>
        {/if}
      </aside>
    </div>
  {/if}
</div>

<style>
  .container {
    width: 100vw;
    height: 100vh;
    display: flex;
    flex-direction: column;
    background: #1a1a1a;
    color: white;
    font-family: 'Inter', -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
  }

  header {
    padding: 1.5rem 2rem;
    display: flex;
    justify-content: space-between;
    align-items: center;
    background: #0d0d0d;
    border-bottom: 1px solid #2a2a2a;
  }

  h1 {
    margin: 0;
    font-size: 1.75rem;
    font-weight: 600;
    color: white;
  }

  .back-link {
    color: #ff8c42;
    text-decoration: none;
    padding: 0.5rem 1rem;
    background: rgba(255, 140, 66, 0.1);
    border-radius: 0.5rem;
    border: 1px solid #ff8c42;
    transition: all 0.2s;
  }

  .back-link:hover {
    background: rgba(255, 140, 66, 0.2);
    transform: translateY(-1px);
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

  .info-bar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 1rem 2rem;
    background: #0d0d0d;
    border-bottom: 1px solid #2a2a2a;
  }

  .current-node {
    display: flex;
    align-items: center;
    gap: 1rem;
    font-size: 1.1rem;
  }

  .current-node .label {
    font-weight: 600;
    color: #999;
  }

  .current-node .value {
    font-weight: 700;
    color: #ff8c42;
  }

  .legend {
    display: flex;
    gap: 1.5rem;
  }

  .legend-item {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.9rem;
  }

  .legend-color {
    width: 20px;
    height: 20px;
    border-radius: 50%;
    border: 2px solid white;
  }

  .main-content {
    flex: 1;
    display: flex;
    overflow: hidden;
  }

  #graph-container {
    flex: 1;
    position: relative;
    overflow: hidden;
  }

  svg {
    background: #2a2a2a;
  }

  .side-panel {
    width: 400px;
    background: #0d0d0d;
    border-left: 1px solid #2a2a2a;
    display: flex;
    flex-direction: column;
    color: white;
  }

  .panel-header {
    padding: 1.5rem;
    border-bottom: 1px solid #2a2a2a;
    display: flex;
    justify-content: space-between;
    align-items: center;
  }

  .panel-header h3 {
    margin: 0;
    font-size: 1.25rem;
    color: white;
  }

  .fact-count {
    background: rgba(255, 140, 66, 0.2);
    color: #ff8c42;
    border: 1px solid #ff8c42;
    padding: 0.25rem 0.75rem;
    border-radius: 1rem;
    font-size: 0.875rem;
    font-weight: 600;
  }

  .panel-loading, .panel-empty {
    padding: 2rem;
    text-align: center;
    color: #666;
    font-style: italic;
  }

  .facts-form {
    flex: 1;
    overflow-y: auto;
    padding: 1.5rem;
  }

  .form-row {
    display: grid;
    grid-template-columns: 120px 1fr;
    gap: 1rem;
    padding: 0.5rem 0;
    border-bottom: 1px solid #2a2a2a;
  }

  .form-row:last-child {
    border-bottom: none;
  }

  .form-label {
    font-weight: 600;
    color: #999;
    font-size: 0.8rem;
    text-align: right;
    padding-top: 0.25rem;
  }

  .form-value {
    color: white;
    font-size: 0.875rem;
    word-break: break-word;
  }

  .entity-link {
    background: none;
    border: none;
    color: #ff8c42;
    text-decoration: underline;
    cursor: pointer;
    padding: 0;
    font-size: 0.875rem;
    font-family: inherit;
    text-align: left;
    transition: color 0.2s;
  }

  .entity-link:hover {
    color: #ffaa66;
    text-decoration: none;
  }
</style>
