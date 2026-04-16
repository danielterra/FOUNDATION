<script>
	let { errorMessage, onDismiss } = $props();
</script>

{#if errorMessage}
	<div class="error-banner">
		<span class="material-symbols-outlined">error</span>
		<span class="error-text">{errorMessage}</span>
		<button class="error-dismiss" onclick={onDismiss} aria-label="Dismiss error">
			<span class="material-symbols-outlined">close</span>
		</button>
	</div>
{/if}

<style>
	.error-banner {
		display: flex;
		align-items: flex-start;
		gap: 10px;
		padding: 12px 14px;
		background: color-mix(in srgb, var(--color-danger) 15%, transparent);
		margin-bottom: 12px;
		flex-shrink: 0;
	}

	.error-banner .material-symbols-outlined {
		font-size: 18px;
		color: var(--color-danger-hover);
		flex-shrink: 0;
		margin-top: 1px;
	}

	.error-text {
		flex: 1;
		font-size: 14px;
		color: var(--color-danger-active);
		line-height: 1.4;
		word-break: break-word;
	}

	.error-dismiss {
		background: none;
		border: none;
		cursor: pointer;
		color: var(--color-danger-hover);
		padding: 0;
		display: flex;
		align-items: center;
		flex-shrink: 0;
		opacity: 0.7;
		transition: opacity 0.15s;
	}

	.error-dismiss:hover {
		opacity: 1;
	}

	.error-dismiss .material-symbols-outlined {
		font-size: 16px;
		margin-top: 0;
	}
</style>
