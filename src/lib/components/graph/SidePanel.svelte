<script>
	import { hierarchicalPredicates, semanticPredicates } from '$lib/utils/predicateCategories';
	import { getPredicateLabel } from '$lib/utils/formatters';
	import PanelHeader from './side-panel/PanelHeader.svelte';
	import QuickStats from './side-panel/QuickStats.svelte';
	import DescriptionSection from './side-panel/DescriptionSection.svelte';
	import PropertiesSection from './side-panel/PropertiesSection.svelte';
	import RelatedConceptsSection from './side-panel/RelatedConceptsSection.svelte';
	import UsedBySection from './side-panel/UsedBySection.svelte';

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
</script>

<aside class="floating-side-panel">
	<PanelHeader {currentNodeLabel} {currentNodeIcon} />

	{#if loadingTriples}
		<div class="loading-state">Loading data...</div>
	{:else}
		<div class="panel-content">
			<QuickStats {nodeStatistics} {organizedTriples} {applicableProperties} />

			<DescriptionSection
				{organizedTriples}
				{expandedSections}
				{toggleSection}
				{onNavigateToNode}
				{getNodeDisplayName}
			/>

			<PropertiesSection
				{groupedProperties}
				{applicableProperties}
				{expandedSections}
				{toggleSection}
				{onNavigateToNode}
				{currentNodeLabel}
				{currentNodeIcon}
			/>

			<RelatedConceptsSection
				{organizedTriples}
				{expandedSections}
				{toggleSection}
				{onNavigateToNode}
				{getNodeDisplayName}
			/>

			<UsedBySection
				{groupedUsedBy}
				{expandedSections}
				{toggleSection}
				{onNavigateToNode}
			/>
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

	:global(.panel-header) {
		padding: 20px 24px;
		border-bottom: 1px solid rgba(255, 255, 255, 0.1);
		flex-shrink: 0;
	}

	:global(.node-title-section) {
		display: flex;
		justify-content: space-between;
		align-items: center;
		gap: 12px;
	}

	:global(.node-icon) {
		font-size: 24px;
		color: rgba(255, 180, 100, 0.9);
		flex-shrink: 0;
	}

	:global(.node-icon-image) {
		width: 24px;
		height: 24px;
		border-radius: 50%;
		object-fit: cover;
		flex-shrink: 0;
	}

	:global(.node-type-badge) {
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

	:global(.panel-header h3) {
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

	:global(.quick-stats-grid) {
		display: grid;
		grid-template-columns: repeat(4, 1fr);
		gap: 8px;
		padding: 16px 24px;
		background: rgba(255, 255, 255, 0.02);
		border-bottom: 1px solid rgba(255, 255, 255, 0.05);
	}

	:global(.stat-box) {
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

	:global(.stat-box .stat-value) {
		font-family: 'Science Gothic Medium', 'Science Gothic', sans-serif;
		font-size: 20px;
		color: rgba(255, 140, 66, 0.9);
		font-weight: 600;
		line-height: 1;
	}

	:global(.stat-box .stat-label) {
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		font-size: 9px;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		color: rgba(255, 255, 255, 0.4);
		text-align: center;
	}

	:global(.section-help-text) {
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

	:global(.panel-section) {
		border-bottom: 1px solid rgba(255, 255, 255, 0.05);
	}

	:global(.section-header) {
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

	:global(.section-header:hover) {
		background: rgba(255, 255, 255, 0.05);
	}

	:global(.section-icon) {
		font-size: 10px;
		color: rgba(255, 140, 66, 0.8);
		width: 12px;
	}

	:global(.section-title) {
		flex: 1;
		text-align: left;
	}

	:global(.section-count) {
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		font-size: 11px;
		color: rgba(255, 140, 66, 0.6);
		background: rgba(255, 140, 66, 0.1);
		padding: 2px 8px;
		border-radius: 4px;
	}

	:global(.section-content) {
		padding: 8px 24px 16px 24px;
	}

	:global(.form-row) {
		margin-bottom: 12px;
	}

	:global(.is-a-row) {
		margin-bottom: 8px;
	}

	:global(.form-label) {
		display: block;
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		font-size: 10px;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		color: rgba(255, 255, 255, 0.5);
		margin-bottom: 4px;
	}

	:global(.form-value) {
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		font-size: 13px;
		color: rgba(255, 255, 255, 0.9);
		word-break: break-word;
	}

	:global(.entity-link) {
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

	:global(.entity-link:hover) {
		color: #ffb380;
		text-decoration: underline;
	}

	/* Entity badge (for property references with icon) */
	:global(.entity-badge) {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		background: rgba(255, 140, 66, 0.1);
		border: 1px solid rgba(255, 140, 66, 0.3);
		border-radius: 6px;
		padding: 4px 10px;
		cursor: pointer;
		transition: all 0.2s;
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		font-size: 13px;
		color: rgba(255, 255, 255, 0.9);
	}

	:global(.entity-badge:hover) {
		background: rgba(255, 140, 66, 0.2);
		border-color: rgba(255, 140, 66, 0.5);
		transform: translateY(-1px);
	}

	:global(.entity-badge .badge-icon) {
		font-size: 16px;
		color: #ff8c42;
	}

	:global(.entity-badge .badge-label) {
		color: rgba(255, 255, 255, 0.95);
	}

	/* Entity link inline (used in "Used by" section) */
	:global(.entity-link-inline) {
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

	:global(.entity-link-inline:hover) {
		color: #ffb380;
	}

	:global(.entity-link-inline .entity-name) {
		font-weight: 600;
		letter-spacing: 0.3px;
	}

	:global(.inline-icon-symbol) {
		font-size: 16px;
		flex-shrink: 0;
	}

	:global(.inline-icon-img) {
		width: 16px;
		height: 16px;
		object-fit: cover;
		border-radius: 2px;
		flex-shrink: 0;
	}

	/* Entity links with icons */
	:global(.entity-link-with-icon) {
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

	:global(.entity-link-with-icon:hover) {
		color: #ffb380;
		background: rgba(255, 140, 66, 0.1);
	}

	:global(.entity-icon) {
		font-size: 16px;
		color: rgba(255, 180, 100, 0.9);
		flex-shrink: 0;
	}

	:global(.entity-icon-img) {
		width: 16px;
		height: 16px;
		object-fit: cover;
		border-radius: 2px;
		flex-shrink: 0;
	}

	:global(.entity-label) {
		flex: 1;
	}

	:global(.hierarchical-list) {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	:global(.literal-value) {
		color: rgba(255, 255, 255, 0.8);
	}

	:global(.subsection) {
		margin-bottom: 16px;
	}

	:global(.subsection-title) {
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		font-size: 11px;
		text-transform: uppercase;
		letter-spacing: 0.5px;
		color: rgba(255, 140, 66, 0.7);
		margin-bottom: 8px;
		font-weight: 500;
	}

	:global(.backlink-predicate) {
		display: block;
		font-size: 10px;
		color: rgba(255, 255, 255, 0.4);
		margin-top: 2px;
	}

	:global(.property-group-header) {
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

	:global(.property-group-header:first-child) {
		margin-top: 0;
	}

	/* Inherited properties layout with sticky icon */
	:global(.inherited-group-container) {
		display: flex;
		gap: 16px;
		margin-top: 16px;
		margin-bottom: 16px;
		padding-bottom: 16px;
		border-bottom: 1px solid rgba(255, 255, 255, 0.08);
	}

	:global(.inherited-icon-sticky) {
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

	:global(.inherited-icon-symbol) {
		font-size: 24px;
		color: rgba(255, 180, 100, 0.9);
	}

	:global(.inherited-icon-img) {
		width: 24px;
		height: 24px;
		object-fit: cover;
		border-radius: 4px;
	}

	:global(.inherited-icon-fallback) {
		font-size: 8px;
		color: rgba(255, 180, 100, 0.7);
		text-align: center;
		text-transform: uppercase;
	}

	:global(.inherited-properties-list) {
		flex: 1;
		min-width: 0;
	}

	:global(.property-item) {
		padding: 10px 0;
		border-bottom: 1px solid rgba(255, 255, 255, 0.05);
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
	}

	:global(.property-item:last-child) {
		border-bottom: none;
	}

	:global(.property-header) {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	:global(.property-title-line) {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}

	:global(.property-name) {
		color: rgba(255, 255, 255, 0.9);
		font-weight: 500;
		font-size: 13px;
		flex-shrink: 1;
	}

	:global(.property-badges) {
		display: flex;
		align-items: center;
		gap: 6px;
		flex-shrink: 0;
	}

	:global(.property-description) {
		font-size: 11px;
		color: rgba(255, 255, 255, 0.5);
		line-height: 1.5;
		padding-left: 2px;
	}

	:global(.cardinality-badge) {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 2px 6px;
		border-radius: 4px;
		transition: all 0.2s ease;
		cursor: help;
	}

	:global(.cardinality-badge .material-symbols-outlined) {
		font-size: 14px;
		font-variation-settings: 'FILL' 1, 'wght' 600, 'GRAD' 0, 'opsz' 20;
	}

	:global(.cardinality-single) {
		background: rgba(255, 255, 255, 0.05);
		border: 1px solid rgba(255, 255, 255, 0.15);
		color: rgba(255, 255, 255, 0.6);
	}

	:global(.cardinality-single:hover) {
		background: rgba(255, 255, 255, 0.08);
		border-color: rgba(255, 255, 255, 0.2);
	}

	:global(.cardinality-multiple) {
		background: rgba(255, 255, 255, 0.05);
		border: 1px solid rgba(255, 255, 255, 0.15);
		color: rgba(255, 255, 255, 0.6);
	}

	:global(.cardinality-multiple:hover) {
		background: rgba(255, 255, 255, 0.08);
		border-color: rgba(255, 255, 255, 0.2);
	}

	:global(.type-badge) {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		padding: 2px 6px;
		border-radius: 4px;
		cursor: help;
		transition: all 0.2s ease;
	}

	:global(.type-badge .material-symbols-outlined) {
		font-size: 14px;
		font-variation-settings: 'FILL' 0, 'wght' 400, 'GRAD' 0, 'opsz' 20;
	}

	:global(.type-reference) {
		background: rgba(255, 255, 255, 0.05);
		border: 1px solid rgba(255, 255, 255, 0.15);
		color: rgba(255, 255, 255, 0.6);
	}

	:global(.type-datatype) {
		background: rgba(255, 255, 255, 0.05);
		border: 1px solid rgba(255, 255, 255, 0.15);
		color: rgba(255, 255, 255, 0.6);
	}

	:global(.range-badge) {
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

	:global(.range-badge:hover) {
		background: rgba(255, 180, 100, 0.25);
		border-color: rgba(255, 180, 100, 0.5);
	}

	:global(.range-icon-symbol) {
		font-size: 14px;
		color: rgba(255, 180, 100, 0.9);
		flex-shrink: 0;
	}

	:global(.range-icon-img) {
		width: 14px;
		height: 14px;
		object-fit: cover;
		border-radius: 2px;
		flex-shrink: 0;
	}

	:global(.range-badge-static) {
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

	:global(.range-badge-text) {
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

	:global(.entity-link-small) {
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

	:global(.entity-link-small:hover) {
		color: #ffb380;
		text-decoration: underline;
	}

	:global(.range-label) {
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
