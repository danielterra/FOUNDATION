<script>
	import { invoke } from '@tauri-apps/api/core';

	export let organizedTriples = {};
	export let onNavigateToNode;
	export let getNodeDisplayName;
	export let getNodeIcon;

	function getPredicateLabel(predicate) {
		return predicate.split(/[/#]/).pop();
	}

	// Get icon with fallback to backend fetch
	async function getIconWithFallback(nodeId) {
		// First try to get from graph data
		const graphIcon = getNodeIcon(nodeId);
		if (graphIcon) {
			return graphIcon;
		}

		// Fetch from backend
		try {
			const icon = await invoke('node__get_icon', { nodeId });
			return icon;
		} catch (err) {
			console.warn(`Failed to fetch icon for ${nodeId}:`, err);
			return null;
		}
	}
</script>

<!-- Properties Section -->
{#if Object.keys(organizedTriples).some(p => p !== 'http://www.w3.org/1999/02/22-rdf-syntax-ns#type')}
	<section class="panel-section">
		<div class="section-header">
			<span class="section-icon material-symbols-outlined">description</span>
			<span class="section-title">Properties</span>
		</div>
		<div class="section-content">
			{#each Object.entries(organizedTriples) as [predicate, triples]}
				{#if predicate !== 'http://www.w3.org/1999/02/22-rdf-syntax-ns#type'}
					<div class="form-row">
						<div class="form-label">{getPredicateLabel(predicate)}</div>
						<div class="form-value">
							{#each triples as triple, i}
								{#if triple.v_type === 'ref'}
									<button
										class="entity-badge"
										on:click={() => onNavigateToNode(triple.v)}
									>
										{#await getIconWithFallback(triple.v)}
											<span class="badge-icon material-symbols-outlined">more_horiz</span>
										{:then icon}
											{#if icon}
												<span class="badge-icon material-symbols-outlined">{icon}</span>
											{:else}
												<span class="badge-icon material-symbols-outlined">link</span>
											{/if}
										{/await}
										<span class="badge-label">{getNodeDisplayName(triple.v)}</span>
									</button>
								{:else}
									<span class="literal-value">{triple.v_label || triple.v}</span>
								{/if}
								{#if i < triples.length - 1}<br />{/if}
							{/each}
						</div>
					</div>
				{/if}
			{/each}
		</div>
	</section>
{/if}

<!-- Type Section -->
{#if organizedTriples['http://www.w3.org/1999/02/22-rdf-syntax-ns#type']}
	<section class="panel-section">
		<div class="section-header">
			<span class="section-icon material-symbols-outlined">category</span>
			<span class="section-title">Type</span>
			<span class="section-count">{organizedTriples['http://www.w3.org/1999/02/22-rdf-syntax-ns#type'].length}</span>
		</div>
		<div class="section-content">
			<div class="hierarchical-list">
				{#each organizedTriples['http://www.w3.org/1999/02/22-rdf-syntax-ns#type'] as triple}
					<button
						class="entity-badge"
						on:click={() => onNavigateToNode(triple.v)}
					>
						{#await getIconWithFallback(triple.v)}
							<span class="badge-icon material-symbols-outlined">more_horiz</span>
						{:then icon}
							{#if icon}
								<span class="badge-icon material-symbols-outlined">{icon}</span>
							{:else}
								<span class="badge-icon material-symbols-outlined">label</span>
							{/if}
						{/await}
						<span class="badge-label">{triple.v_label || getNodeDisplayName(triple.v)}</span>
					</button>
				{/each}
			</div>
		</div>
	</section>
{/if}
