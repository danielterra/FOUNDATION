<script>
	import Card from './Card.svelte';

	let { message = '', progress = null } = $props();

	let isIndefinite = $derived(progress === null);
	let percentage = $derived(progress !== null ? Math.round(progress) : 0);
</script>

<Card>
	<div class="activity">
		<p class="message">{message}</p>

		{#if isIndefinite}
			<!-- Spinner para progresso indefinido -->
			<div class="spinner"></div>
		{:else}
			<!-- Progress bar para progresso definido -->
			<div class="progress-container">
				<div class="progress-bar">
					<div class="progress-fill" style="width: {percentage}%"></div>
				</div>
				<span class="percentage">{percentage}%</span>
			</div>
		{/if}
	</div>
</Card>

<style>
	.activity {
		display: flex;
		flex-direction: row;
		align-items: center;
		justify-content: center;
		gap: 1.5rem;
	}

	.message {
		color: var(--color-neutral);
		font-size: 0.875rem;
		margin: 0;
		max-width: 300px;
		line-height: 1.4;
		display: -webkit-box;
		-webkit-line-clamp: 2;
		line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	/* Spinner para progresso indefinido */
	.spinner {
		width: 32px;
		height: 32px;
		border: 3px solid var(--color-black);
		border-top-color: var(--color-transition);
		border-radius: 50%;
		flex-shrink: 0;
		animation: spin 0.8s linear infinite, glow 2s ease-in-out infinite;
	}

	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}

	/* Progress bar para progresso definido */
	.progress-container {
		display: flex;
		flex-direction: row;
		align-items: center;
		gap: 0.75rem;
		flex-shrink: 0;
	}

	.progress-bar {
		width: 200px;
		height: 8px;
		background: var(--color-black);
		border-radius: 4px;
		overflow: hidden;
	}

	.progress-fill {
		height: 100%;
		background: var(--color-transition);
		border-radius: 4px;
		transition: width 0.3s ease;
		position: relative;
		overflow: hidden;
	}

	.progress-fill::after {
		content: '';
		position: absolute;
		top: 0;
		left: -100%;
		width: 100%;
		height: 100%;
		background: linear-gradient(
			90deg,
			transparent,
			rgba(255, 255, 255, 0.4),
			transparent
		);
		animation: shimmer 2s ease-in-out infinite;
	}

	.percentage {
		color: var(--color-neutral);
		font-size: 0.875rem;
		font-weight: 600;
		flex-shrink: 0;
		min-width: 3ch;
	}

	/* Animação de shimmer - luz pulsante da esquerda para direita */
	@keyframes shimmer {
		0% {
			left: -100%;
		}
		50% {
			left: 100%;
		}
		100% {
			left: 100%;
		}
	}

	/* Animação de glow pulsante para o spinner */
	@keyframes glow {
		0%,
		100% {
			filter: brightness(1) drop-shadow(0 0 4px var(--color-transition));
		}
		50% {
			filter: brightness(1.4) drop-shadow(0 0 12px var(--color-transition));
		}
	}
</style>
