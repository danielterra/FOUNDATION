<script>
	import { getPredicateLabel } from '$lib/utils/formatters';

	export let organizedTriples = null;
	export let expandedSections = {};
	export let toggleSection;
	export let onNavigateToNode;
	export let getNodeDisplayName;
</script>

{#if organizedTriples?.semantic?.length > 0}
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
