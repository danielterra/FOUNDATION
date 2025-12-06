<script>
	export let currentNodeLabel = '';
	export let currentNodeIcon = null;
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
		usedby: true
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

	// Group properties by source (own vs inherited, and by parent class)
	$: groupedProperties = {
		own: applicableProperties.filter(p => !p.is_inherited),
		inheritedByClass: applicableProperties
			.filter(p => p.is_inherited)
			.reduce((acc, prop) => {
				const key = prop.source_class;
				if (!acc[key]) {
					acc[key] = {
						className: prop.source_class_label,
						classIcon: prop.source_class_icon,
						properties: []
					};
				}
				acc[key].properties.push(prop);
				return acc;
			}, {})
	};

	// Organize triples by category
	$: organizedTriples = {
		metadata: nodeTriples.filter(t => ['http://www.w3.org/2000/01/rdf-schema#label', 'http://www.w3.org/2000/01/rdf-schema#comment', 'http://www.w3.org/2004/02/skos/core#altLabel', 'http://www.w3.org/2004/02/skos/core#example'].some(p => t.a === p)),
		hierarchical: nodeTriples.filter(t => hierarchicalPredicates.some(p => t.a === p)),
		semantic: nodeTriples.filter(t => semanticPredicates.some(p => t.a === p))
	};

	// Filter for "Used by" section: only show rdfs:range relationships
	// This shows which properties have this class as their range
	$: organizedBacklinks = {
		children: nodeBacklinks.filter(t => hierarchicalPredicates.some(p => t.a === p)),
		references: nodeBacklinks.filter(t => t.a === 'http://www.w3.org/2000/01/rdf-schema#range')
	};

	// Group "Used by" references by domain class (similar to inherited properties grouping)
	$: groupedUsedBy = organizedBacklinks.references.reduce((acc, backlink) => {
		const domainKey = backlink.domain || 'unknown';
		if (!acc[domainKey]) {
			acc[domainKey] = {
				className: backlink.domain_label || getNodeDisplayName(domainKey),
				classIcon: backlink.domain_icon,
				classId: domainKey,
				properties: []
			};
		}
		acc[domainKey].properties.push({
			label: backlink.v_label || getPredicateLabel(backlink.v),
			comment: backlink.a_comment,
			id: backlink.v
		});
		return acc;
	}, {});


	function getPredicateLabel(predicate) {
		return predicate.split(/[/#]/).pop();
	}

	// Helper to detect icon type
	function getIconType(icon) {
		if (!icon) return null;
		if (icon.startsWith('http://') || icon.startsWith('https://') ||
		    icon.startsWith('file://') || icon.startsWith('data:')) {
			return 'image';
		}
		return 'material-symbol';
	}

	// Map XSD datatypes to appropriate Material Symbols icons
	function getDatatypeIcon(rangeLabel) {
		if (!rangeLabel) return 'text_fields';

		const label = rangeLabel.toLowerCase();

		// String types
		if (label.includes('string') || label.includes('literal')) return 'text_fields';

		// Numeric types
		if (label.includes('integer') || label.includes('int') || label.includes('long') ||
		    label.includes('short') || label.includes('byte')) return '123';
		if (label.includes('decimal') || label.includes('float') || label.includes('double')) return 'decimal';

		// Boolean
		if (label.includes('boolean')) return 'toggle_on';

		// Date/Time types
		if (label.includes('datetime')) return 'calendar_clock';
		if (label.includes('date')) return 'calendar_today';
		if (label.includes('time')) return 'schedule';

		// URI/URL
		if (label.includes('uri') || label.includes('url') || label.includes('anyuri')) return 'link';

		// Binary/Data
		if (label.includes('base64') || label.includes('hexbinary')) return 'data_object';

		// Default
		return 'text_fields';
	}

	$: iconType = getIconType(currentNodeIcon);
</script>

<aside class="floating-side-panel">
	<div class="panel-header">
		<div class="node-title-section">
			{#if currentNodeIcon}
				{#if iconType === 'image'}
					<img src={currentNodeIcon} alt="icon" class="node-icon-image" />
				{:else}
					<span class="node-icon material-symbols-outlined">{currentNodeIcon}</span>
				{/if}
			{/if}
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
			{#if organizedTriples.metadata.length > 0 || organizedTriples.hierarchical.length > 0}
				<section class="panel-section">
					<button class="section-header" on:click={() => toggleSection('metadata')}>
						<span class="section-icon">{expandedSections.metadata ? '▼' : '▶'}</span>
						<span class="section-title">Description</span>
					</button>
					{#if expandedSections.metadata}
						<div class="section-content">
							<!-- Is a type of (hierarchical relationships) -->
							{#if organizedTriples.hierarchical.length > 0}
								<div class="form-row">
									<label class="form-label">Is a type of</label>
									<div class="form-value hierarchical-list">
										{#each organizedTriples.hierarchical as triple}
											<button class="entity-link-with-icon" on:click={() => onNavigateToNode(triple.v)}>
												{#if triple.v_icon}
													{#if getIconType(triple.v_icon) === 'image'}
														<img src={triple.v_icon} alt="icon" class="entity-icon-img" />
													{:else}
														<span class="material-symbols-outlined entity-icon">{triple.v_icon}</span>
													{/if}
												{/if}
												<span class="entity-label">{triple.v_label || getNodeDisplayName(triple.v)}</span>
											</button>
										{/each}
									</div>
								</div>
							{/if}

							<!-- Other metadata -->
							{#each organizedTriples.metadata as triple}
								<div class="form-row">
									<label class="form-label" title={triple.a}>
										{getPredicateLabel(triple.a)}
									</label>
									<div class="form-value">
										{#if triple.v_type === 'ref'}
											<button class="entity-link-with-icon" on:click={() => onNavigateToNode(triple.v)}>
												{#if triple.v_icon}
													{#if getIconType(triple.v_icon) === 'image'}
														<img src={triple.v_icon} alt="icon" class="entity-icon-img" />
													{:else}
														<span class="material-symbols-outlined entity-icon">{triple.v_icon}</span>
													{/if}
												{/if}
												<span class="entity-label">{getNodeDisplayName(triple.v)}</span>
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

			<!-- Properties Section -->
			{#if applicableProperties.length > 0}
				<section class="panel-section">
					<button class="section-header" on:click={() => toggleSection('properties')}>
						<span class="section-icon">{expandedSections.properties ? '▼' : '▶'}</span>
						<span class="section-title">Properties</span>
					</button>
					{#if expandedSections.properties}
						<div class="section-content">
							<!-- Own Properties -->
							{#if groupedProperties.own.length > 0}
								<div class="inherited-group-container">
									<div class="inherited-icon-sticky" title="Own properties of {currentNodeLabel}">
										{#if currentNodeIcon}
											{#if getIconType(currentNodeIcon) === 'image'}
												<img src={currentNodeIcon} alt={currentNodeLabel} class="inherited-icon-img" />
											{:else}
												<span class="material-symbols-outlined inherited-icon-symbol">{currentNodeIcon}</span>
											{/if}
										{:else}
											<span class="inherited-icon-fallback">{currentNodeLabel}</span>
										{/if}
									</div>
									<div class="inherited-properties-list">
										{#each groupedProperties.own as prop}
									<div class="property-item">
										<div class="property-header">
											<div class="property-title-line">
												<span class="property-name">{prop.property_label}</span>
												<div class="property-badges">
													{#if prop.cardinality}
														<span
															class="cardinality-badge {prop.cardinality === 'exactly one' ? 'cardinality-single' : 'cardinality-multiple'}"
															title={prop.cardinality}
														>
															<span class="material-symbols-outlined">
																{prop.cardinality === 'exactly one' ? 'looks_one' : 'all_inclusive'}
															</span>
														</span>
													{/if}
													{#if prop.property_type === 'object'}
														<span class="type-badge type-reference" title="Reference">
															<span class="material-symbols-outlined">link</span>
														</span>
														{#if prop.range_label || prop.range}
															{#if prop.range}
																<button class="range-badge" on:click={() => onNavigateToNode(prop.range)} title={prop.range_label || prop.range.split(/[/#]/).pop()}>
																	{#if prop.range_icon}
																		{#if getIconType(prop.range_icon) === 'image'}
																			<img src={prop.range_icon} alt="icon" class="range-icon-img" />
																		{:else}
																			<span class="material-symbols-outlined range-icon-symbol">{prop.range_icon}</span>
																		{/if}
																	{:else}
																		{prop.range_label || prop.range.split(/[/#]/).pop()}
																	{/if}
																</button>
															{:else}
																<span class="range-badge-static">{prop.range_label}</span>
															{/if}
														{/if}
													{:else}
														<span class="type-badge type-datatype" title={prop.range_label || 'String'}>
															<span class="material-symbols-outlined">{getDatatypeIcon(prop.range_label)}</span>
														</span>
													{/if}
												</div>
											</div>
											{#if prop.description}
												<div class="property-description">{prop.description}</div>
											{/if}
										</div>
									</div>
										{/each}
									</div>
								</div>
							{/if}

							<!-- Inherited Properties grouped by parent class -->
							{#each Object.entries(groupedProperties.inheritedByClass) as [classId, classGroup]}
								<div class="inherited-group-container">
									<div class="inherited-icon-sticky" title="Inherited from {classGroup.className}">
										{#if classGroup.classIcon}
											{#if getIconType(classGroup.classIcon) === 'image'}
												<img src={classGroup.classIcon} alt={classGroup.className} class="inherited-icon-img" />
											{:else}
												<span class="material-symbols-outlined inherited-icon-symbol">{classGroup.classIcon}</span>
											{/if}
										{:else}
											<span class="inherited-icon-fallback">{classGroup.className}</span>
										{/if}
									</div>
									<div class="inherited-properties-list">
										{#each classGroup.properties as prop}
									<div class="property-item">
										<div class="property-header">
											<div class="property-title-line">
												<span class="property-name">{prop.property_label}</span>
												<div class="property-badges">
													{#if prop.cardinality}
														<span
															class="cardinality-badge {prop.cardinality === 'exactly one' ? 'cardinality-single' : 'cardinality-multiple'}"
															title={prop.cardinality}
														>
															<span class="material-symbols-outlined">
																{prop.cardinality === 'exactly one' ? 'looks_one' : 'all_inclusive'}
															</span>
														</span>
													{/if}
													{#if prop.property_type === 'object'}
														<span class="type-badge type-reference" title="Reference">
															<span class="material-symbols-outlined">link</span>
														</span>
														{#if prop.range_label || prop.range}
															{#if prop.range}
																<button class="range-badge" on:click={() => onNavigateToNode(prop.range)} title={prop.range_label || prop.range.split(/[/#]/).pop()}>
																	{#if prop.range_icon}
																		{#if getIconType(prop.range_icon) === 'image'}
																			<img src={prop.range_icon} alt="icon" class="range-icon-img" />
																		{:else}
																			<span class="material-symbols-outlined range-icon-symbol">{prop.range_icon}</span>
																		{/if}
																	{:else}
																		{prop.range_label || prop.range.split(/[/#]/).pop()}
																	{/if}
																</button>
															{:else}
																<span class="range-badge-static">{prop.range_label}</span>
															{/if}
														{/if}
													{:else}
														<span class="type-badge type-datatype" title={prop.range_label || 'String'}>
															<span class="material-symbols-outlined">{getDatatypeIcon(prop.range_label)}</span>
														</span>
													{/if}
												</div>
											</div>
											{#if prop.description}
												<div class="property-description">{prop.description}</div>
											{/if}
										</div>
									</div>
										{/each}
									</div>
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

			<!-- Used By Section -->
			{#if Object.keys(groupedUsedBy).length > 0}
				<section class="panel-section">
					<button class="section-header" on:click={() => toggleSection('usedby')}>
						<span class="section-icon">{expandedSections.usedby ? '▼' : '▶'}</span>
						<span class="section-title">Used by</span>
					</button>
					{#if expandedSections.usedby}
						<div class="section-content">
							{#each Object.entries(groupedUsedBy) as [classId, classGroup]}
								<div class="inherited-group-container">
									<button class="inherited-icon-sticky" on:click={() => onNavigateToNode(classId)} title="{classGroup.className}">
										{#if classGroup.classIcon}
											{#if getIconType(classGroup.classIcon) === 'image'}
												<img src={classGroup.classIcon} alt={classGroup.className} class="inherited-icon-img" />
											{:else}
												<span class="material-symbols-outlined inherited-icon-symbol">{classGroup.classIcon}</span>
											{/if}
										{:else}
											<span class="inherited-icon-fallback">{classGroup.className.substring(0, 1)}</span>
										{/if}
									</button>
									<div class="inherited-properties-list">
										{#each classGroup.properties as prop}
											<div class="property-item">
												<div class="property-header">
													<div class="property-title-line">
														<span class="property-name">{prop.label}</span>
													</div>
												</div>
												{#if prop.comment}
													<div class="property-description">{prop.comment}</div>
												{/if}
											</div>
										{/each}
									</div>
								</div>
							{/each}
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

	.node-icon {
		font-size: 24px;
		color: rgba(255, 180, 100, 0.9);
		flex-shrink: 0;
	}

	.node-icon-image {
		width: 24px;
		height: 24px;
		border-radius: 50%;
		object-fit: cover;
		flex-shrink: 0;
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

	/* Entity link inline (used in "Used by" section) */
	.entity-link-inline {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		background: none;
		border: none;
		color: #ff8c42;
		cursor: pointer;
		text-decoration: none;
		padding: 0;
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		font-size: 11px;
		transition: color 0.2s;
	}

	.entity-link-inline:hover {
		color: #ffb380;
	}

	.entity-link-inline .entity-name {
		font-weight: 600;
		letter-spacing: 0.3px;
	}

	.inline-icon-symbol {
		font-size: 16px;
		flex-shrink: 0;
	}

	.inline-icon-img {
		width: 16px;
		height: 16px;
		object-fit: cover;
		border-radius: 2px;
		flex-shrink: 0;
	}

	/* Entity links with icons */
	.entity-link-with-icon {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		background: none;
		border: none;
		color: #ff8c42;
		cursor: pointer;
		text-decoration: none;
		padding: 4px 8px;
		font-family: inherit;
		font-size: inherit;
		text-align: left;
		transition: all 0.2s;
		border-radius: 4px;
	}

	.entity-link-with-icon:hover {
		color: #ffb380;
		background: rgba(255, 140, 66, 0.1);
	}

	.entity-icon {
		font-size: 16px;
		color: rgba(255, 180, 100, 0.9);
		flex-shrink: 0;
	}

	.entity-icon-img {
		width: 16px;
		height: 16px;
		object-fit: cover;
		border-radius: 2px;
		flex-shrink: 0;
	}

	.entity-label {
		flex: 1;
	}

	.hierarchical-list {
		display: flex;
		flex-direction: column;
		gap: 4px;
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

	.property-group-header {
		display: flex;
		align-items: center;
		justify-content: center;
		margin-top: 12px;
		margin-bottom: 6px;
		padding: 2px 0;
		font-size: 0.75em;
		font-weight: 400;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: rgba(255, 180, 100, 0.7);
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
	}

	.property-group-header:first-child {
		margin-top: 0;
	}

	/* Inherited properties layout with sticky icon */
	.inherited-group-container {
		display: flex;
		gap: 16px;
		margin-top: 16px;
		margin-bottom: 16px;
		padding-bottom: 16px;
		border-bottom: 1px solid rgba(255, 255, 255, 0.08);
	}

	.inherited-icon-sticky {
		position: sticky;
		top: 0;
		width: 40px;
		min-width: 40px;
		height: 40px;
		display: flex;
		align-items: center;
		justify-content: center;
		background: rgba(255, 180, 100, 0.1);
		border-radius: 8px;
		border: 1px solid rgba(255, 180, 100, 0.2);
		flex-shrink: 0;
		z-index: 1;
	}

	.inherited-icon-symbol {
		font-size: 24px;
		color: rgba(255, 180, 100, 0.9);
	}

	.inherited-icon-img {
		width: 24px;
		height: 24px;
		object-fit: cover;
		border-radius: 4px;
	}

	.inherited-icon-fallback {
		font-size: 8px;
		color: rgba(255, 180, 100, 0.7);
		text-align: center;
		text-transform: uppercase;
	}

	.inherited-properties-list {
		flex: 1;
		min-width: 0;
	}

	.property-item {
		padding: 10px 0;
		border-bottom: 1px solid rgba(255, 255, 255, 0.05);
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
	}

	.property-item:last-child {
		border-bottom: none;
	}

	.property-header {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.property-title-line {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}

	.property-name {
		color: rgba(255, 255, 255, 0.9);
		font-weight: 500;
		font-size: 13px;
		flex-shrink: 1;
	}

	.property-badges {
		display: flex;
		align-items: center;
		gap: 6px;
		flex-shrink: 0;
	}

	.property-description {
		font-size: 11px;
		color: rgba(255, 255, 255, 0.5);
		line-height: 1.5;
		padding-left: 2px;
	}

	.cardinality-badge {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 2px 6px;
		border-radius: 4px;
		transition: all 0.2s ease;
		cursor: help;
	}

	.cardinality-badge .material-symbols-outlined {
		font-size: 14px;
		font-variation-settings: 'FILL' 1, 'wght' 600, 'GRAD' 0, 'opsz' 20;
	}

	.cardinality-single {
		background: rgba(255, 255, 255, 0.05);
		border: 1px solid rgba(255, 255, 255, 0.15);
		color: rgba(255, 255, 255, 0.6);
	}

	.cardinality-single:hover {
		background: rgba(255, 255, 255, 0.08);
		border-color: rgba(255, 255, 255, 0.2);
	}

	.cardinality-multiple {
		background: rgba(255, 255, 255, 0.05);
		border: 1px solid rgba(255, 255, 255, 0.15);
		color: rgba(255, 255, 255, 0.6);
	}

	.cardinality-multiple:hover {
		background: rgba(255, 255, 255, 0.08);
		border-color: rgba(255, 255, 255, 0.2);
	}

	.type-badge {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 2px 6px;
		border-radius: 4px;
		cursor: help;
		transition: all 0.2s ease;
	}

	.type-badge .material-symbols-outlined {
		font-size: 14px;
		font-variation-settings: 'FILL' 0, 'wght' 400, 'GRAD' 0, 'opsz' 20;
	}

	.type-reference {
		background: rgba(255, 255, 255, 0.05);
		border: 1px solid rgba(255, 255, 255, 0.15);
		color: rgba(255, 255, 255, 0.6);
	}

	.type-datatype {
		background: rgba(255, 255, 255, 0.05);
		border: 1px solid rgba(255, 255, 255, 0.15);
		color: rgba(255, 255, 255, 0.6);
	}

	.range-badge {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		background: rgba(255, 180, 100, 0.15);
		border: 1px solid rgba(255, 180, 100, 0.3);
		border-radius: 4px;
		padding: 2px 8px;
		font-size: 9px;
		color: rgba(255, 180, 100, 0.9);
		cursor: pointer;
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		font-weight: 600;
		transition: all 0.2s ease;
	}

	.range-badge:hover {
		background: rgba(255, 180, 100, 0.25);
		border-color: rgba(255, 180, 100, 0.5);
	}

	.range-icon-symbol {
		font-size: 14px;
		color: rgba(255, 180, 100, 0.9);
		flex-shrink: 0;
	}

	.range-icon-img {
		width: 14px;
		height: 14px;
		object-fit: cover;
		border-radius: 2px;
		flex-shrink: 0;
	}

	.range-badge-static {
		background: rgba(255, 255, 255, 0.05);
		border: 1px solid rgba(255, 255, 255, 0.15);
		border-radius: 4px;
		padding: 2px 8px;
		font-size: 9px;
		color: rgba(255, 255, 255, 0.6);
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		font-weight: 600;
	}

	.range-badge-text {
		background: rgba(255, 255, 255, 0.05);
		border: 1px solid rgba(255, 255, 255, 0.1);
		border-radius: 4px;
		padding: 2px 8px;
		font-size: 10px;
		color: rgba(255, 255, 255, 0.5);
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		text-transform: uppercase;
		letter-spacing: 0.5px;
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
