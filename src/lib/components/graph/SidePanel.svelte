<script>
	export let currentNodeLabel = '';
	export let nodeTriples = [];
	export let nodeBacklinks = [];
	export let nodeStatistics = null;
	export let applicableProperties = [];
	export let loadingTriples = false;
	export let onNavigateToNode;
	export let getNodeDisplayName;

	let expandedSections = {
		statistics: true,
		metadata: true,
		properties: true,
		hierarchical: true,
		semantic: true,
		backlinks: true
	};

	function toggleSection(section) {
		expandedSections[section] = !expandedSections[section];
	}

	// Predicate categories
	const hierarchicalPredicates = [
		'http://www.w3.org/2000/01/rdf-schema#subClassOf',
		'http://www.w3.org/2000/01/rdf-schema#subPropertyOf',
		'http://www.w3.org/2004/02/skos/core#broader'
	];

	const semanticPredicates = [
		'http://www.w3.org/2004/02/skos/core#related',
		'http://FOUNDATION.local/ontology/antonym',
		'http://www.w3.org/2000/01/rdf-schema#seeAlso',
		'http://FOUNDATION.local/ontology/causes',
		'http://FOUNDATION.local/ontology/entails',
		'http://FOUNDATION.local/ontology/partOf',
		'http://FOUNDATION.local/ontology/hasPart',
		'http://FOUNDATION.local/ontology/memberOf',
		'http://FOUNDATION.local/ontology/hasMember',
		'http://FOUNDATION.local/ontology/madeOf',
		'http://FOUNDATION.local/ontology/containsSubstance',
		'http://FOUNDATION.local/ontology/domainTopic',
		'http://FOUNDATION.local/ontology/pertainsTo'
	];

	// Organize triples by category
	$: organizedTriples = {
		metadata: nodeTriples.filter(t => ['http://www.w3.org/2000/01/rdf-schema#label', 'http://www.w3.org/2000/01/rdf-schema#comment', 'http://www.w3.org/2004/02/skos/core#altLabel', 'http://www.w3.org/2004/02/skos/core#example'].some(p => t.a === p)),
		hierarchical: nodeTriples.filter(t => hierarchicalPredicates.some(p => t.a === p)),
		semantic: nodeTriples.filter(t => semanticPredicates.some(p => t.a === p))
	};

	$: organizedBacklinks = {
		children: nodeBacklinks.filter(t => hierarchicalPredicates.some(p => t.a === p)),
		references: nodeBacklinks.filter(t => !hierarchicalPredicates.some(p => t.a === p) && !semanticPredicates.some(p => t.a === p))
	};

	function getPredicateLabel(predicate) {
		return predicate.split(/[/#]/).pop();
	}
</script>

<aside class="floating-side-panel">
	<div class="panel-header">
		<div class="node-title-section">
			<h3>{currentNodeLabel}</h3>
			<span class="node-type-badge">Class</span>
		</div>
	</div>

	{#if loadingTriples}
		<div class="loading-state">Loading data...</div>
	{:else}
		<div class="panel-content">
			<!-- Quick Stats Grid -->
			{#if nodeStatistics}
				<div class="quick-stats-grid">
					<div class="stat-box">
						<span class="stat-value">{nodeStatistics.children_count || 0}</span>
						<span class="stat-label">Types</span>
					</div>
					<div class="stat-box">
						<span class="stat-value">{organizedTriples.hierarchical.length || 0}</span>
						<span class="stat-label">Is a</span>
					</div>
					<div class="stat-box">
						<span class="stat-value">{applicableProperties.length || 0}</span>
						<span class="stat-label">Can have</span>
					</div>
					<div class="stat-box">
						<span class="stat-value">{nodeStatistics.backlinks_count || 0}</span>
						<span class="stat-label">Used by</span>
					</div>
				</div>
			{/if}

			<!-- Description Section -->
			{#if organizedTriples.metadata.length > 0}
				<section class="panel-section">
					<button class="section-header" on:click={() => toggleSection('metadata')}>
						<span class="section-icon">{expandedSections.metadata ? '▼' : '▶'}</span>
						<span class="section-title">Description</span>
					</button>
					{#if expandedSections.metadata}
						<div class="section-content">
							{#each organizedTriples.metadata as triple}
								<div class="form-row">
									<label class="form-label" title={triple.a}>
										{getPredicateLabel(triple.a)}
									</label>
									<div class="form-value">
										{#if triple.v_type === 'ref'}
											<button class="entity-link" on:click={() => onNavigateToNode(triple.v)}>
												{getNodeDisplayName(triple.v)}
											</button>
										{:else}
											<span class="literal-value">{triple.v}</span>
										{/if}
									</div>
								</div>
							{/each}
						</div>
					{/if}
				</section>
			{/if}

			<!-- Can Have Section -->
			{#if applicableProperties.length > 0}
				<section class="panel-section">
					<button class="section-header" on:click={() => toggleSection('properties')}>
						<span class="section-icon">{expandedSections.properties ? '▼' : '▶'}</span>
						<span class="section-title">Can have</span>
					</button>
					{#if expandedSections.properties}
						<div class="section-content">
							{#each applicableProperties as prop}
								<div class="property-item">
									<div class="property-header">
										<button class="entity-link" on:click={() => onNavigateToNode(prop.property_id)}>
											{prop.property_label}
										</button>
										<span class="property-type-badge">{prop.property_type.split(/[/#]/).pop()}</span>
									</div>
									{#if prop.range_label || prop.range}
										<div class="property-range">
											→
											{#if prop.range}
												<button class="entity-link-small" on:click={() => onNavigateToNode(prop.range)}>
													{prop.range_label || prop.range.split(/[/#]/).pop()}
												</button>
											{:else}
												<span class="range-label">{prop.range_label}</span>
											{/if}
										</div>
									{/if}
								</div>
							{/each}
						</div>
					{/if}
				</section>
			{/if}

			<!-- Is A Section -->
			{#if organizedTriples.hierarchical.length > 0}
				<section class="panel-section">
					<button class="section-header" on:click={() => toggleSection('hierarchical')}>
						<span class="section-icon">{expandedSections.hierarchical ? '▼' : '▶'}</span>
						<span class="section-title">Is a type of</span>
					</button>
					{#if expandedSections.hierarchical}
						<div class="section-content">
							{#each organizedTriples.hierarchical as triple}
								<div class="form-row is-a-row">
									<button class="entity-link" on:click={() => onNavigateToNode(triple.v)}>
										{triple.v_label || getNodeDisplayName(triple.v)}
									</button>
								</div>
							{/each}
						</div>
					{/if}
				</section>
			{/if}

			<!-- Related Concepts Section -->
			{#if organizedTriples.semantic.length > 0}
				<section class="panel-section">
					<button class="section-header" on:click={() => toggleSection('semantic')}>
						<span class="section-icon">{expandedSections.semantic ? '▼' : '▶'}</span>
						<span class="section-title">Related to</span>
					</button>
					{#if expandedSections.semantic}
						<div class="section-content">
							{#each organizedTriples.semantic as triple}
								<div class="form-row">
									<label class="form-label" title={triple.a}>
										{getPredicateLabel(triple.a)}
									</label>
									<div class="form-value">
										<button class="entity-link" on:click={() => onNavigateToNode(triple.v)}>
											{triple.v_label || getNodeDisplayName(triple.v)}
										</button>
									</div>
								</div>
							{/each}
						</div>
					{/if}
				</section>
			{/if}

			<!-- More Specific Types Section -->
			{#if nodeBacklinks.length > 0}
				<section class="panel-section">
					<button class="section-header" on:click={() => toggleSection('backlinks')}>
						<span class="section-icon">{expandedSections.backlinks ? '▼' : '▶'}</span>
						<span class="section-title">More specific types</span>
					</button>
					{#if expandedSections.backlinks}
						<div class="section-content">
							<!-- More Specific Types (subClassOf pointing TO this) -->
							{#if organizedBacklinks.children.length > 0}
								<div class="subsection">
									<div class="subsection-title">Types of {currentNodeLabel} ({organizedBacklinks.children.length})</div>
									{#each organizedBacklinks.children as backlink}
										<div class="form-row">
											<button class="entity-link" on:click={() => onNavigateToNode(backlink.v)}>
												{backlink.v_label || getNodeDisplayName(backlink.v)}
											</button>
										</div>
									{/each}
								</div>
							{/if}

							<!-- Other References -->
							{#if organizedBacklinks.references.length > 0}
								<div class="subsection">
									<div class="subsection-title">Used by ({organizedBacklinks.references.length})</div>
									{#each organizedBacklinks.references as backlink}
										<div class="form-row">
											<button class="entity-link" on:click={() => onNavigateToNode(backlink.v)}>
												{backlink.v_label || getNodeDisplayName(backlink.v)}
											</button>
											<span class="backlink-predicate" title={backlink.a}>
												{getPredicateLabel(backlink.a)}
											</span>
										</div>
									{/each}
								</div>
							{/if}
						</div>
					{/if}
				</section>
			{/if}
		</div>
	{/if}
</aside>

<style>
	.floating-side-panel {
		position: fixed;
		top: 80px;
		right: 20px;
		width: 380px;
		max-height: calc(100vh - 100px);
		background: rgba(10, 10, 10, 0.7);
		backdrop-filter: blur(20px);
		border: 1px solid rgba(255, 255, 255, 0.1);
		border-radius: 12px;
		box-shadow: 0 4px 20px rgba(0, 0, 0, 0.5);
		z-index: 999;
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.panel-header {
		padding: 20px 24px;
		border-bottom: 1px solid rgba(255, 255, 255, 0.1);
		flex-shrink: 0;
	}

	.node-title-section {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 12px;
	}

	.node-type-badge {
		display: inline-block;
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		font-size: 9px;
		text-transform: uppercase;
		letter-spacing: 1px;
		color: rgba(100, 200, 255, 0.8);
		background: rgba(100, 200, 255, 0.12);
		padding: 4px 8px;
		border-radius: 3px;
		flex-shrink: 0;
	}

	.panel-header h3 {
		margin: 0;
		font-family: 'Science Gothic Medium', 'Science Gothic', sans-serif;
		font-size: 18px;
		font-weight: 500;
		color: #ffffff;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		flex: 1;
	}

	.quick-stats-grid {
		display: grid;
		grid-template-columns: repeat(4, 1fr);
		gap: 8px;
		padding: 16px 24px;
		background: rgba(255, 255, 255, 0.02);
		border-bottom: 1px solid rgba(255, 255, 255, 0.05);
	}

	.stat-box {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 4px;
		padding: 12px 8px;
		background: rgba(255, 255, 255, 0.03);
		border-radius: 6px;
		border: 1px solid rgba(255, 255, 255, 0.05);
	}

	.stat-box .stat-value {
		font-family: 'Science Gothic Medium', 'Science Gothic', sans-serif;
		font-size: 20px;
		color: rgba(255, 140, 66, 0.9);
		font-weight: 600;
		line-height: 1;
	}

	.stat-box .stat-label {
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		font-size: 9px;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		color: rgba(255, 255, 255, 0.4);
		text-align: center;
	}

	.section-help-text {
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		font-size: 10px;
		color: rgba(255, 255, 255, 0.4);
		font-style: italic;
		margin-left: auto;
	}

	.panel-content {
		overflow-y: auto;
		flex: 1;
		padding: 8px 0;
	}

	.panel-section {
		border-bottom: 1px solid rgba(255, 255, 255, 0.05);
	}

	.section-header {
		width: 100%;
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 12px 24px;
		background: none;
		border: none;
		color: rgba(255, 255, 255, 0.9);
		font-family: 'Science Gothic Medium', 'Science Gothic', sans-serif;
		font-size: 13px;
		cursor: pointer;
		transition: background 0.2s;
	}

	.section-header:hover {
		background: rgba(255, 255, 255, 0.05);
	}

	.section-icon {
		font-size: 10px;
		color: rgba(255, 140, 66, 0.8);
		width: 12px;
	}

	.section-title {
		flex: 1;
		text-align: left;
	}

	.section-count {
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		font-size: 11px;
		color: rgba(255, 140, 66, 0.6);
		background: rgba(255, 140, 66, 0.1);
		padding: 2px 8px;
		border-radius: 4px;
	}

	.section-content {
		padding: 8px 24px 16px 24px;
	}

	.form-row {
		margin-bottom: 12px;
	}

	.is-a-row {
		margin-bottom: 8px;
	}

	.form-label {
		display: block;
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		color: rgba(255, 255, 255, 0.5);
		margin-bottom: 4px;
	}

	.form-value {
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		font-size: 13px;
		color: rgba(255, 255, 255, 0.9);
		word-break: break-word;
	}

	.entity-link {
		background: none;
		border: none;
		color: #ff8c42;
		cursor: pointer;
		text-decoration: none;
		padding: 0;
		font-family: inherit;
		font-size: inherit;
		text-align: left;
		transition: color 0.2s;
	}

	.entity-link:hover {
		color: #ffb380;
		text-decoration: underline;
	}

	.literal-value {
		color: rgba(255, 255, 255, 0.8);
	}

	.subsection {
		margin-bottom: 16px;
	}

	.subsection-title {
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		font-size: 11px;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		color: rgba(255, 140, 66, 0.7);
		margin-bottom: 8px;
		font-weight: 500;
	}

	.backlink-predicate {
		display: block;
		font-size: 10px;
		color: rgba(255, 255, 255, 0.4);
		margin-top: 2px;
	}

	.property-item {
		margin-bottom: 12px;
		padding: 8px 12px;
		background: rgba(255, 255, 255, 0.02);
		border-radius: 6px;
		border-left: 2px solid rgba(100, 200, 255, 0.3);
	}

	.property-header {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-bottom: 4px;
	}

	.property-type-badge {
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		font-size: 9px;
		padding: 2px 6px;
		background: rgba(100, 200, 255, 0.15);
		color: rgba(100, 200, 255, 0.8);
		border-radius: 3px;
		text-transform: uppercase;
	}

	.property-range {
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		font-size: 11px;
		color: rgba(255, 255, 255, 0.5);
		margin-left: 8px;
		display: flex;
		align-items: center;
		gap: 4px;
	}

	.entity-link-small {
		background: none;
		border: none;
		color: #ff8c42;
		cursor: pointer;
		text-decoration: none;
		padding: 0;
		font-family: inherit;
		font-size: inherit;
		text-align: left;
		transition: color 0.2s;
	}

	.entity-link-small:hover {
		color: #ffb380;
		text-decoration: underline;
	}

	.range-label {
		color: rgba(255, 255, 255, 0.6);
	}

	.loading-state {
		padding: 40px 24px;
		text-align: center;
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		font-size: 14px;
		color: rgba(255, 255, 255, 0.5);
	}

	/* Custom scrollbar */
	.panel-content::-webkit-scrollbar {
		width: 8px;
	}

	.panel-content::-webkit-scrollbar-track {
		background: rgba(0, 0, 0, 0.2);
		border-radius: 4px;
	}

	.panel-content::-webkit-scrollbar-thumb {
		background: rgba(255, 140, 66, 0.3);
		border-radius: 4px;
	}

	.panel-content::-webkit-scrollbar-thumb:hover {
		background: rgba(255, 140, 66, 0.5);
	}

	/* Remove unused stat-badge styles that were replaced by stat-box */
</style>
