<script>
	import { getIconType } from '$lib/utils/formatters';

	export let currentNodeLabel = '';
	export let currentNodeIcon = null;
	export let nodeType = 'Class'; // 'Class' or 'Instance'

	$: iconType = getIconType(currentNodeIcon);
</script>

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
		<span class="node-type-badge">{nodeType}</span>
	</div>
</div>
