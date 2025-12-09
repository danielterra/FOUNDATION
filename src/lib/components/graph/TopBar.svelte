<script>
	import Card from '$lib/components/Card.svelte';
	import IconButton from '$lib/components/IconButton.svelte';
	import Search from '$lib/components/graph/Search.svelte';

	let { onRecenter, onSearch, screenName = 'Ontology Graph' } = $props();
	let searchComponent;

	// Expose focusSearch method to parent
	export function focusSearch() {
		if (searchComponent) {
			searchComponent.focus();
		}
	}
</script>

<div class="floating-top-bar">
	<div class="left-controls">
		<Card>
			<header class="floating-header">
				<h1>FOUNDATION</h1>
				<span class="screen-indicator">{screenName}</span>
			</header>
		</Card>

		<IconButton icon="center_focus_strong" hint="Recenter graph (⌘0)" onclick={onRecenter} />
	</div>

	<div class="right-controls">
		<Search bind:this={searchComponent} onSelectResult={onSearch} />
	</div>
</div>

<style>
	.floating-top-bar {
		position: fixed;
		top: 20px;
		left: 20px;
		right: 20px;
		display: flex;
		justify-content: space-between;
		align-items: center;
		z-index: 1000;
		pointer-events: none;
	}

	.left-controls {
		display: flex;
		align-items: center;
		gap: 1rem;
	}

	.left-controls :global(.card) {
		pointer-events: auto;
	}

	.right-controls {
		display: flex;
		align-items: center;
		gap: 1rem;
		pointer-events: auto;
	}

	.floating-header {
		display: flex;
		align-items: center;
		gap: 0.75rem;
	}

	.floating-header h1 {
		margin: 0;
		font-size: 1.25rem;
		font-weight: 500;
		color: var(--color-neutral-active);
		letter-spacing: 0.03rem;
	}

	.screen-indicator {
		font-size: 0.875rem;
		color: var(--color-neutral);
		padding-left: 0.75rem;
		border-left: 1px solid color-mix(in srgb, var(--color-white) 15%, transparent);
	}
</style>
