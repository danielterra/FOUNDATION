<script>
	import { Card } from '$lib/components/ui/card';
	import { Button } from '$lib/components/ui/button';

	let { onRecenter, onOpenSearch, screenName = 'Ontology Graph' } = $props();
</script>

<div class="floating-top-bar">
	<div class="left-controls">
		<Card class="py-2 px-3 gap-0">
			<header class="floating-header">
				<span class="screen-indicator">{screenName}</span>
				<button class="search-trigger" onclick={onOpenSearch} aria-label="Search (/)">
					<span class="material-symbols-outlined">search</span>
				</button>
			</header>
		</Card>

		<Button variant="ghost" size="icon" title="Recenter graph (⌘0)" onclick={onRecenter}><span class="material-symbols-outlined">center_focus_strong</span></Button>
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

	.screen-indicator {
		font-size: 0.875rem;
		color: var(--color-neutral);
	}

	.search-trigger {
		background: none;
		border: none;
		cursor: pointer;
		color: var(--color-neutral);
		padding: 0.25rem;
		display: flex;
		align-items: center;
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
