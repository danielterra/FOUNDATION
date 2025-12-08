<script>
	import Card from './Card.svelte';

	let { icon, hint = '', onclick } = $props();
</script>

<Card>
	<button class="icon-button" {onclick} title={hint}>
		<span class="material-symbols-outlined">{icon}</span>
	</button>
</Card>

<style>
	.icon-button {
		background: transparent;
		border: none;
		padding: 0;
		cursor: pointer;
		color: var(--color-interactive);
		transition: all 0.2s;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.icon-button:hover {
		color: var(--color-interactive-hover);
		transform: scale(1.1);
	}

	.icon-button .material-symbols-outlined {
		font-size: 1.5rem;
	}
</style>
