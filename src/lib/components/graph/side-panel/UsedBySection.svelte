<script>
	import { getIconType } from '$lib/utils/formatters';

	export let groupedUsedBy = {};
	export let expandedSections = {};
	export let toggleSection;
	export let onNavigateToNode;
</script>

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
