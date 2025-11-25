<script>
  import { invoke } from "@tauri-apps/api/core";
  import { onMount } from "svelte";

  let facts = $state([]);
  let stats = $state(null);
  let loading = $state(true);
  let error = $state(null);

  async function loadData() {
    try {
      loading = true;
      error = null;

      // Load database stats
      const statsJson = await invoke("get_db_stats");
      stats = JSON.parse(statsJson);

      // Load all facts
      const factsJson = await invoke("get_all_facts", { limit: 1000 });
      facts = JSON.parse(factsJson);

      loading = false;
    } catch (e) {
      error = e.toString();
      loading = false;
    }
  }

  onMount(() => {
    loadData();
  });
</script>

<main class="container">
  <h1>SuperNOVA - Database Facts</h1>

  {#if loading}
    <p class="loading">Loading database...</p>
  {:else if error}
    <p class="error">Error: {error}</p>
  {:else}
    <!-- Stats Section -->
    {#if stats}
      <div class="stats-card">
        <h2>Database Statistics</h2>
        <div class="stats-grid">
          <div class="stat">
            <span class="stat-label">Total Facts:</span>
            <span class="stat-value">{stats.total_facts}</span>
          </div>
          <div class="stat">
            <span class="stat-label">Active Facts:</span>
            <span class="stat-value">{stats.active_facts}</span>
          </div>
          <div class="stat">
            <span class="stat-label">Transactions:</span>
            <span class="stat-value">{stats.total_transactions}</span>
          </div>
          <div class="stat">
            <span class="stat-label">Entities:</span>
            <span class="stat-value">{stats.entities_count}</span>
          </div>
          <div class="stat">
            <span class="stat-label">Ontology Imported:</span>
            <span class="stat-value">{stats.ontology_imported ? "✅ Yes" : "❌ No"}</span>
          </div>
        </div>
      </div>
    {/if}

    <!-- Facts Table -->
    <div class="facts-section">
      <h2>All Facts ({facts.length})</h2>

      {#if facts.length === 0}
        <p class="empty">No facts in database yet. Import base ontologies to populate.</p>
      {:else}
        <div class="table-container">
          <table class="facts-table">
            <thead>
              <tr>
                <th>TX</th>
                <th>Entity (E)</th>
                <th>Attribute (A)</th>
                <th>Value (V)</th>
                <th>Type</th>
                <th>Origin</th>
                <th>Status</th>
              </tr>
            </thead>
            <tbody>
              {#each facts as fact}
                <tr class:retracted={fact.retracted}>
                  <td class="tx">{fact.tx}</td>
                  <td class="entity" title={fact.e}>{fact.e}</td>
                  <td class="attribute" title={fact.a}>{fact.a}</td>
                  <td class="value" title={fact.v}>{fact.v}</td>
                  <td class="type">
                    <span class="badge badge-{fact.v_type}">{fact.v_type}</span>
                  </td>
                  <td class="origin">{fact.origin}</td>
                  <td class="status">
                    {#if fact.retracted}
                      <span class="badge badge-retracted">Retracted</span>
                    {:else}
                      <span class="badge badge-active">Active</span>
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </div>

    <button class="refresh-btn" onclick={loadData}>Refresh Data</button>
  {/if}
</main>

<style>
  :global(body) {
    margin: 0;
    padding: 0;
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
  }

  .container {
    max-width: 1400px;
    margin: 0 auto;
    padding: 20px;
  }

  h1 {
    text-align: center;
    color: #2c3e50;
    margin-bottom: 30px;
  }

  h2 {
    color: #34495e;
    margin-top: 0;
  }

  .loading, .error, .empty {
    text-align: center;
    padding: 40px;
    font-size: 1.2em;
  }

  .error {
    color: #e74c3c;
    background-color: #fadbd8;
    border-radius: 8px;
  }

  .empty {
    color: #7f8c8d;
    background-color: #ecf0f1;
    border-radius: 8px;
  }

  /* Stats Card */
  .stats-card {
    background: white;
    border-radius: 12px;
    padding: 24px;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
    margin-bottom: 30px;
  }

  .stats-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 20px;
    margin-top: 20px;
  }

  .stat {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .stat-label {
    font-size: 0.9em;
    color: #7f8c8d;
    font-weight: 500;
  }

  .stat-value {
    font-size: 1.8em;
    font-weight: 700;
    color: #2c3e50;
  }

  /* Facts Section */
  .facts-section {
    background: white;
    border-radius: 12px;
    padding: 24px;
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.1);
    margin-bottom: 30px;
  }

  .table-container {
    overflow-x: auto;
    margin-top: 20px;
  }

  .facts-table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.9em;
  }

  .facts-table thead {
    background-color: #34495e;
    color: white;
  }

  .facts-table th {
    padding: 12px 8px;
    text-align: left;
    font-weight: 600;
    position: sticky;
    top: 0;
  }

  .facts-table td {
    padding: 10px 8px;
    border-bottom: 1px solid #ecf0f1;
    max-width: 300px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .facts-table tr:hover {
    background-color: #f8f9fa;
  }

  .facts-table tr.retracted {
    opacity: 0.5;
    text-decoration: line-through;
  }

  .tx {
    font-weight: 600;
    color: #3498db;
    min-width: 60px;
  }

  .entity {
    font-family: monospace;
    color: #8e44ad;
  }

  .attribute {
    font-family: monospace;
    color: #16a085;
  }

  .value {
    font-family: monospace;
    color: #2c3e50;
  }

  /* Badges */
  .badge {
    display: inline-block;
    padding: 4px 8px;
    border-radius: 4px;
    font-size: 0.85em;
    font-weight: 600;
    text-transform: uppercase;
  }

  .badge-string { background-color: #e8f5e9; color: #2e7d32; }
  .badge-number { background-color: #e3f2fd; color: #1565c0; }
  .badge-integer { background-color: #e1f5fe; color: #0277bd; }
  .badge-boolean { background-color: #fff3e0; color: #e65100; }
  .badge-ref { background-color: #f3e5f5; color: #6a1b9a; }
  .badge-datetime { background-color: #fce4ec; color: #c2185b; }

  .badge-active { background-color: #c8e6c9; color: #2e7d32; }
  .badge-retracted { background-color: #ffcdd2; color: #c62828; }

  /* Refresh Button */
  .refresh-btn {
    display: block;
    margin: 0 auto;
    padding: 12px 24px;
    background-color: #3498db;
    color: white;
    border: none;
    border-radius: 8px;
    font-size: 1em;
    font-weight: 600;
    cursor: pointer;
    transition: background-color 0.3s;
  }

  .refresh-btn:hover {
    background-color: #2980b9;
  }

  /* Dark mode support */
  @media (prefers-color-scheme: dark) {
    :global(body) {
      background-color: #1a1a1a;
      color: #e0e0e0;
    }

    h1, h2 {
      color: #e0e0e0;
    }

    .stats-card, .facts-section {
      background-color: #2d2d2d;
    }

    .stat-label {
      color: #b0b0b0;
    }

    .stat-value {
      color: #e0e0e0;
    }

    .facts-table thead {
      background-color: #1a1a1a;
    }

    .facts-table td {
      border-bottom-color: #3a3a3a;
    }

    .facts-table tr:hover {
      background-color: #3a3a3a;
    }

    .empty {
      background-color: #2d2d2d;
      color: #b0b0b0;
    }
  }
</style>
