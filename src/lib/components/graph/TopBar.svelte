<script>
	import Card from '$lib/components/Card.svelte';
	import IconButton from '$lib/components/IconButton.svelte';

	let { onRecenter, onOpenSearch, screenName = 'Ontology Graph' } = $props();
</script>

<div class="floating-top-bar">
	<div class="left-controls">
		<Card>
			<header class="floating-header">
				<h1>FOUNDATION</h1>
				<span class="screen-indicator">{screenName}</span>
				<button class="search-trigger" onclick={onOpenSearch} aria-label="Search (/)">
					<span class="material-symbols-outlined">search</span>
				</button>
			</header>
		</Card>

		<IconButton icon="center_focus_strong" hint="Recenter graph (⌘0)" onclick={onRecenter} />
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

	.search-trigger {
		background: none;
		border: none;
		cursor: pointer;
		color: var(--color-neutral);
		padding: 0.25rem;
		display: flex;
		align-items: center;
		border-radius: 4px;
		transition: color 0.15s, background 0.15s;
		margin-left: 0.25rem;
	}

	.search-trigger:hover {
		color: var(--color-neutral-active);
		background: color-mix(in srgb, var(--color-white) 8%, transparent);
	}

	.search-trigger span {
		font-size: 20px;
	}
</style>
