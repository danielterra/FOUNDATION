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
		background: rgba(0, 0, 0, 0.8);
		backdrop-filter: blur(2px);
		-webkit-backdrop-filter: blur(2px);
		border-radius: 16px;
		padding: 1rem;
		border: 1px solid color-mix(in srgb, var(--color-white) 20%, transparent);
	}
</style>
