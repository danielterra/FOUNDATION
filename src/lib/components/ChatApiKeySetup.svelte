<script>
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';

	let { apiKey = $bindable(''), onSave } = $props();
</script>

<div class="api-key-setup">
	<h3>Setup Claude API</h3>
	<p>Enter your Anthropic API key to enable AI chat:</p>
	<Input
		type="password"
		bind:value={apiKey}
		placeholder="sk-ant-..."
		onkeydown={(e) => e.key === 'Enter' && onSave()}
	/>
	<Button onclick={onSave} disabled={!apiKey.trim()}>
		Save API Key
	</Button>
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
		color: var(--accent-foreground);
		font-size: 18px;
	}

	.api-key-setup p {
		margin: 0;
		color: var(--foreground);
		font-size: 14px;
	}

	.api-key-setup small {
		color: var(--muted-foreground);
		font-size: 12px;
	}
</style>
