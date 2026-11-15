<script>
	import { Button } from '$lib/components/ui/button';
	import chatgptLogo from '$lib/assets/providers/chatgpt-logo-white.svg';

	let { errorMessage, onDismiss } = $props();
	let usageLimitReached = $derived(/subscription_sharing_usage_(limit_exceeded|unavailable)|usage[_ ]limit/i.test(String(errorMessage ?? '')));
</script>

{#if errorMessage}
	<div class="error-banner" class:usage-limit={usageLimitReached} role={usageLimitReached ? 'alert' : undefined}>
		{#if usageLimitReached}
			<img class="chatgpt-error-logo" src={chatgptLogo} alt="ChatGPT" />
		{:else}
			<span class="material-symbols-outlined">error</span>
		{/if}
		<div class="error-copy">
			<span class="error-text">{usageLimitReached ? 'O limite de uso do plano ChatGPT foi atingido.' : errorMessage}</span>
			{#if usageLimitReached}
				<span class="usage-limit-description">Revise os limites da sua conta ChatGPT para continuar.</span>
			{/if}
		</div>
		{#if usageLimitReached}
			<a class="manage-usage" href="https://chatgpt.com/settings/usage" target="_blank" rel="noopener noreferrer">Manage usage</a>
		{/if}
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

	.error-banner.usage-limit {
		align-items: center;
		background: color-mix(in srgb, var(--color-warning) 12%, transparent);
		border-inline-start: var(--size-hud-frame-line) solid var(--color-warning);
	}

	.error-copy {
		display: grid;
		gap: var(--spacing-label-to-value);
		flex: 1;
		min-width: 0;
	}

	.chatgpt-error-logo {
		width: var(--type-icon-medium);
		height: var(--type-icon-medium);
		flex: 0 0 auto;
	}

	.usage-limit-description {
		color: var(--text-secondary);
		font-size: var(--type-body-compact);
		line-height: var(--line-height-body);
	}

	.manage-usage {
		flex: 0 0 auto;
		padding: var(--spacing-control-internal);
		border: var(--size-hud-frame-line) solid var(--color-interactive);
		border-radius: var(--radius);
		color: var(--text-accent);
		font-size: var(--type-body-compact);
		text-decoration: none;
		cursor: pointer;
	}

	.manage-usage:hover {
		background: color-mix(in srgb, var(--color-interactive) 12%, transparent);
	}

	.error-text {
		flex: 1;
		font-size: 14px;
		color: var(--destructive-foreground);
		line-height: 1.4;
		word-break: break-word;
	}
</style>
