<script>
	import { fade, scale } from 'svelte/transition';

	let { backgroundColor = 'var(--color-black)', children } = $props();
</script>

<div in:fade={{ duration: 300 }} out:fade={{ duration: 200 }}>
	<div class="card" style="--bg-color: {backgroundColor}" in:scale={{ duration: 300, start: 0.9 }}>
		{@render children()}
	</div>
</div>

<style>
	.card {
		background: var(--color-surface-1);
		border-radius: var(--radius-md);
		padding: 1rem;
	}
</style>
