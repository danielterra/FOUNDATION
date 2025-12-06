<script>
	import { getIconType, getPredicateLabel } from '$lib/utils/formatters';

	export let organizedTriples = null;
	export let expandedSections = {};
	export let toggleSection;
	export let onNavigateToNode;
	export let getNodeDisplayName;
</script>

{#if organizedTriples && (organizedTriples.metadata?.length > 0 || organizedTriples.hierarchical?.length > 0)}
	<section class="panel-section">
		<button class="section-header" on:click={() => toggleSection('metadata')}>
			<span class="section-icon">{expandedSections.metadata ? '▼' : '▶'}</span>
			<span class="section-title">Description</span>
		</button>
		{#if expandedSections.metadata}
			<div class="section-content">
				<!-- Is a type of (hierarchical relationships) -->
				{#if organizedTriples.hierarchical?.length > 0}
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
