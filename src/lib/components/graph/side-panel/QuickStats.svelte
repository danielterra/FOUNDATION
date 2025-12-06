<script>
	export let nodeStatistics = null;
	export let organizedTriples = null;
	export let applicableProperties = [];
</script>

{#if nodeStatistics}
	<div class="quick-stats-grid">
		<div class="stat-box">
			<span class="stat-value">{nodeStatistics.children_count || 0}</span>
			<span class="stat-label">Types</span>
		</div>
		<div class="stat-box">
			<span class="stat-value">{organizedTriples?.hierarchical?.length || 0}</span>
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
