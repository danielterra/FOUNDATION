<script>
	import { Button } from '$lib/components/ui/button';

	let { errorMessage, onDismiss } = $props();
</script>

{#if errorMessage}
	<div class="error-banner">
		<span class="material-symbols-outlined">error</span>
		<span class="error-text">{errorMessage}</span>
		<Button variant="ghost" size="icon" onclick={onDismiss} aria-label="Dismiss error">
			<span class="material-symbols-outlined">close</span>
		</Button>
	</div>
{/if}

<style>
	.error-banner {
		display: flex;
		align-items: flex-start;
		gap: 10px;
		padding: 12px 14px;
		background: color-mix(in srgb, var(--destructive) 15%, transparent);
		border-radius: var(--radius);
		margin-bottom: 12px;
		flex-shrink: 0;
	}

	.error-banner .material-symbols-outlined {
		font-size: 18px;
		color: var(--destructive);
		flex-shrink: 0;
		margin-top: 1px;
	}

	.error-text {
		flex: 1;
		font-size: 14px;
		color: var(--destructive-foreground);
		line-height: 1.4;
		word-break: break-word;
	}
</style>
