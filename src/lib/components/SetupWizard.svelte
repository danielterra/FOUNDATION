<script>
	import { invoke } from '@tauri-apps/api/core';
	import Card from './Card.svelte';
	import TextInput from './TextInput.svelte';

	let { onComplete = () => {} } = $props();

	let personName = $state('');
	let personEmail = $state('');
	let isSubmitting = $state(false);

	async function complete() {
		if (!personName.trim()) return;

		isSubmitting = true;

		try {
			const result = await invoke('setup__init', {
				userName: personName,
				email: personEmail || null
			});

			console.log('Setup completed:', result);
			onComplete(result);
		} catch (e) {
			console.error('Setup failed:', e);
			alert(`Setup failed: ${e}`);
			isSubmitting = false;
		}
	}
</script>

<div class="wizard-overlay">
	<Card>
		<div class="wizard-container">
			<h1>FOUNDATION</h1>

			<TextInput
				bind:value={personName}
				placeholder="What is your name?"
				autofocus
				onkeydown={(e) => e.key === 'Enter' && !personEmail && complete()}
			/>

			<button onclick={complete} disabled={isSubmitting || !personName.trim()}>
				{isSubmitting ? 'Setting up...' : 'Start'}
			</button>
		</div>
	</Card>
</div>

<style>
	.wizard-overlay {
		position: fixed;
		top: 0;
		left: 0;
		width: 100vw;
		height: 100vh;
		background: rgba(0, 0, 0, 0.95);
		display: flex;
		justify-content: center;
		align-items: center;
		z-index: 9999;
	}

	.wizard-container {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 2rem;
	}

	h1 {
		font-size: 1.5rem;
		color: var(--color-neutral);
		margin: 0;
		text-transform: uppercase;
		letter-spacing: 0.125rem;
	}

	.wizard-container :global(input) {
		width: 400px;
	}

	button {
		background: var(--color-interactive);
		color: var(--color-neutral-on-interactive);
		border: none;
		padding: 1rem 3rem;
		border-radius: 8px;
		font-size: 1.125rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.125rem;
		cursor: pointer;
		transition: all 0.2s;
	}

	button:hover:not(:disabled) {
		background: var(--color-interactive-hover);
		transform: translateY(-2px);
		box-shadow: 0 4px 16px color-mix(in srgb, var(--color-interactive) 40%, transparent);
	}

	button:disabled {
		background: var(--color-interactive-disabled);
		cursor: not-allowed;
		transform: none;
	}
</style>
