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
		'http://supernova.local/ontology/antonym',
		'http://www.w3.org/2000/01/rdf-schema#seeAlso',
		'http://supernova.local/ontology/causes',
		'http://supernova.local/ontology/entails',
		'http://supernova.local/ontology/partOf',
		'http://supernova.local/ontology/hasPart',
		'http://supernova.local/ontology/memberOf',
		'http://supernova.local/ontology/hasMember',
		'http://supernova.local/ontology/madeOf',
		'http://supernova.local/ontology/containsSubstance',
		'http://supernova.local/ontology/domainTopic',
		'http://supernova.local/ontology/pertainsTo'
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
		<h3>{currentNodeLabel}</h3>
	</div>

	{#if loadingTriples}
		<div class="loading-state">Loading data...</div>
	{:else}
		<div class="panel-content">
			<!-- Statistics Section -->
			{#if nodeStatistics}
				<section class="panel-section">
					<button class="section-header" on:click={() => toggleSection('statistics')}>
						<span class="section-icon">{expandedSections.statistics ? '▼' : '▶'}</span>
						<span class="section-title">Statistics</span>
					</button>
					{#if expandedSections.statistics}
						<div class="section-content stats-badges">
							{#if nodeStatistics.children_count > 0}
								<span class="stat-badge children">{nodeStatistics.children_count} children</span>
							{/if}
							{#if nodeStatistics.synonyms_count > 0}
								<span class="stat-badge synonyms">{nodeStatistics.synonyms_count} synonyms</span>
							{/if}
							{#if nodeStatistics.related_count > 0}
								<span class="stat-badge related">{nodeStatistics.related_count} related</span>
							{/if}
							{#if nodeStatistics.examples_count > 0}
								<span class="stat-badge examples">{nodeStatistics.examples_count} examples</span>
							{/if}
							{#if nodeStatistics.backlinks_count > 0}
								<span class="stat-badge backlinks">{nodeStatistics.backlinks_count} backlinks</span>
							{/if}
						</div>
					{/if}
				</section>
			{/if}

			<!-- Metadata Section -->
			{#if organizedTriples.metadata.length > 0}
				<section class="panel-section">
					<button class="section-header" on:click={() => toggleSection('metadata')}>
						<span class="section-icon">{expandedSections.metadata ? '▼' : '▶'}</span>
						<span class="section-title">Metadata</span>
						<span class="section-count">{organizedTriples.metadata.length}</span>
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

			<!-- Applicable Properties Section -->
			{#if applicableProperties.length > 0}
				<section class="panel-section">
					<button class="section-header" on:click={() => toggleSection('properties')}>
						<span class="section-icon">{expandedSections.properties ? '▼' : '▶'}</span>
						<span class="section-title">Applicable Properties</span>
						<span class="section-count">{applicableProperties.length}</span>
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

			<!-- Hierarchical Relations Section -->
			{#if organizedTriples.hierarchical.length > 0}
				<section class="panel-section">
					<button class="section-header" on:click={() => toggleSection('hierarchical')}>
						<span class="section-icon">{expandedSections.hierarchical ? '▼' : '▶'}</span>
						<span class="section-title">Hierarchical</span>
						<span class="section-count">{organizedTriples.hierarchical.length}</span>
					</button>
					{#if expandedSections.hierarchical}
						<div class="section-content">
							{#each organizedTriples.hierarchical as triple}
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

			<!-- Semantic Relations Section -->
			{#if organizedTriples.semantic.length > 0}
				<section class="panel-section">
					<button class="section-header" on:click={() => toggleSection('semantic')}>
						<span class="section-icon">{expandedSections.semantic ? '▼' : '▶'}</span>
						<span class="section-title">Semantic Relations</span>
						<span class="section-count">{organizedTriples.semantic.length}</span>
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

			<!-- Backlinks Section -->
			{#if nodeBacklinks.length > 0}
				<section class="panel-section">
					<button class="section-header" on:click={() => toggleSection('backlinks')}>
						<span class="section-icon">{expandedSections.backlinks ? '▼' : '▶'}</span>
						<span class="section-title">Incoming Relations</span>
						<span class="section-count">{nodeBacklinks.length}</span>
					</button>
					{#if expandedSections.backlinks}
						<div class="section-content">
							<!-- Children (subClassOf, subPropertyOf, broader pointing TO this) -->
							{#if organizedBacklinks.children.length > 0}
								<div class="subsection">
									<div class="subsection-title">Children ({organizedBacklinks.children.length})</div>
									{#each organizedBacklinks.children as backlink}
										<div class="form-row">
											<button class="entity-link" on:click={() => onNavigateToNode(backlink.v)}>
												{backlink.v_label || getNodeDisplayName(backlink.v)}
											</button>
											<span class="backlink-predicate" title={backlink.a}>
												via {getPredicateLabel(backlink.a)}
											</span>
										</div>
									{/each}
								</div>
							{/if}

							<!-- Other References -->
							{#if organizedBacklinks.references.length > 0}
								<div class="subsection">
									<div class="subsection-title">Referenced By ({organizedBacklinks.references.length})</div>
									{#each organizedBacklinks.references as backlink}
										<div class="form-row">
											<button class="entity-link" on:click={() => onNavigateToNode(backlink.v)}>
												{backlink.v_label || getNodeDisplayName(backlink.v)}
											</button>
											<span class="backlink-predicate" title={backlink.a}>
												via {getPredicateLabel(backlink.a)}
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
		background: rgba(30, 30, 30, 0.95);
		backdrop-filter: blur(10px);
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

	.panel-header h3 {
		margin: 0;
		font-family: 'Science Gothic Medium', 'Science Gothic', sans-serif;
		font-size: 16px;
		font-weight: 500;
		color: #ffffff;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
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

	.stats-badges {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}

	.stat-badge {
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		font-size: 11px;
		padding: 6px 12px;
		border-radius: 6px;
		font-weight: 500;
	}

	.stat-badge.children {
		background: rgba(100, 150, 255, 0.15);
		color: rgba(100, 150, 255, 0.9);
	}

	.stat-badge.synonyms {
		background: rgba(150, 100, 255, 0.15);
		color: rgba(150, 100, 255, 0.9);
	}

	.stat-badge.related {
		background: rgba(255, 140, 66, 0.15);
		color: rgba(255, 140, 66, 0.9);
	}

	.stat-badge.examples {
		background: rgba(66, 200, 150, 0.15);
		color: rgba(66, 200, 150, 0.9);
	}

	.stat-badge.backlinks {
		background: rgba(255, 100, 150, 0.15);
		color: rgba(255, 100, 150, 0.9);
	}

	.form-row {
		margin-bottom: 12px;
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
</style>
