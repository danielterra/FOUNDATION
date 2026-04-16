<script>
	let { apiKey = $bindable(''), onSave } = $props();
</script>

<div class="api-key-setup">
	<h3>Setup Claude API</h3>
	<p>Enter your Anthropic API key to enable AI chat:</p>
	<input
		type="password"
		bind:value={apiKey}
		placeholder="sk-ant-..."
		onkeydown={(e) => e.key === 'Enter' && onSave()}
	/>
	<button onclick={onSave} disabled={!apiKey.trim()}>
		Save API Key
	</button>
	<small>Your API key is securely stored in your local ontology database</small>
</div>

<style>
	.api-key-setup {
		display: flex;
		flex-direction: column;
		gap: 16px;
		padding: 20px;
	}

	.api-key-setup h3 {
		margin: 0;
		color: var(--color-neutral-active);
		font-size: 18px;
	}

	.api-key-setup p {
		margin: 0;
		color: var(--color-neutral);
		font-size: 14px;
	}

	.api-key-setup input {
		padding: 12px 16px;
		border: none;
		background: color-mix(in srgb, var(--color-white) 5%, transparent);
		color: var(--color-neutral-active);
		font-family: inherit;
		font-size: 14px;
	}

	.api-key-setup input:focus {
		outline: none;
	}

	.api-key-setup button {
		padding: 12px 24px;
		border: none;
		background: var(--color-interactive);
		color: var(--color-neutral-on-interactive);
		font-size: 14px;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.2s;
	}

	.api-key-setup button:hover:not(:disabled) {
		background: var(--color-interactive-hover);
	}

	.api-key-setup button:disabled {
		background: var(--color-neutral-disabled);
		cursor: not-allowed;
		opacity: 0.5;
	}

	.api-key-setup small {
		color: var(--color-neutral-disabled);
		font-size: 12px;
	}
</style>
