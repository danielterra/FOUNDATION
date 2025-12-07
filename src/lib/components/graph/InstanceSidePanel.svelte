<script>
	import PanelHeader from './side-panel/PanelHeader.svelte';
	import InstancePropertiesSection from './side-panel/InstancePropertiesSection.svelte';

	export let currentNodeLabel = '';
	export let currentNodeIcon = null;
	export let nodeTriples = [];
	export let loadingTriples = false;
	export let onNavigateToNode;
	export let getNodeDisplayName;
	export let getNodeIcon;

	// Organize triples by predicate (using correct field names from backend)
	$: organizedTriples = nodeTriples.reduce((acc, triple) => {
		const predicate = triple.a; // 'a' is the predicate field
		if (!acc[predicate]) {
			acc[predicate] = [];
		}
		acc[predicate].push(triple);
		return acc;
	}, {});
</script>

<aside class="floating-side-panel">
	<PanelHeader {currentNodeLabel} {currentNodeIcon} nodeType="Instance" />

	{#if loadingTriples}
		<div class="loading-state">Loading data...</div>
	{:else}
		<div class="panel-content">
			<InstancePropertiesSection
				{organizedTriples}
				{onNavigateToNode}
				{getNodeDisplayName}
				{getNodeIcon}
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

	.loading-state {
		padding: 40px 24px;
		text-align: center;
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		font-size: 14px;
		color: rgba(255, 255, 255, 0.5);
	}

	.panel-content {
		overflow-y: auto;
		flex: 1;
		padding: 8px 0;
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

	/* Entity badge styles */
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
</style>
