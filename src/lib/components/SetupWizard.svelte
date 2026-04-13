<script>
	import { invoke } from '@tauri-apps/api/core';
	import { onMount } from 'svelte';
	import Card from './Card.svelte';
	import TextInput from './TextInput.svelte';

	let { onComplete = () => {} } = $props();

	let personName = $state('');
	let personEmail = $state('');
	let isSubmitting = $state(false);
	let step = $state(1); // 1 = user info, 2 = AI config

	let aiServices = $state([]);
	let aiModels = $state([]);
	let selectedService = $state('');
	let selectedModel = $state('');
	let isLoadingServices = $state(false);
	let isLoadingModels = $state(false);

	onMount(async () => {
		// Load AI services
		try {
			isLoadingServices = true;
			aiServices = await invoke('setup__list_ai_services');

			// Auto-select Claude AI Service if available
			const claudeService = aiServices.find(s => s.iri === 'foundation:ClaudeAIService');
			if (claudeService) {
				selectedService = claudeService.iri;
				await loadModels(selectedService);
			}
		} catch (e) {
			console.error('Failed to load AI services:', e);
		} finally {
			isLoadingServices = false;
		}
	});

	async function loadModels(serviceIri) {
		if (!serviceIri) return;

		try {
			isLoadingModels = true;
			aiModels = await invoke('setup__list_ai_models', { serviceIri });

			// Auto-select default model if available
			const defaultModel = aiModels.find(m => m.isDefault);
			if (defaultModel) {
				selectedModel = defaultModel.iri;
			} else if (aiModels.length > 0) {
				selectedModel = aiModels[0].iri;
			}
		} catch (e) {
			console.error('Failed to load AI models:', e);
		} finally {
			isLoadingModels = false;
		}
	}

	async function handleServiceChange(e) {
		selectedService = e.target.value;
		selectedModel = '';
		await loadModels(selectedService);
	}

	function nextStep() {
		if (!personName.trim()) return;
		step = 2;
	}

	function previousStep() {
		step = 1;
	}

	async function complete() {
		if (!personName.trim() || !selectedService || !selectedModel) return;

		isSubmitting = true;

		try {
			const result = await invoke('setup__init', {
				userName: personName,
				email: personEmail || null,
				aiServiceIri: selectedService,
				aiModelIri: selectedModel
			});

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

			{#if step === 1}
				<!-- Step 1: User Information -->
				<div class="step">
					<h2>Welcome</h2>
					<TextInput
						bind:value={personName}
						placeholder="What is your name?"
						autofocus
						onkeydown={(e) => e.key === 'Enter' && nextStep()}
					/>

					<button onclick={nextStep} disabled={!personName.trim()}>
						Next
					</button>
				</div>
			{:else if step === 2}
				<!-- Step 2: AI Configuration -->
				<div class="step">
					<h2>AI Configuration</h2>

					{#if isLoadingServices}
						<p class="loading">Loading AI services...</p>
					{:else}
						<div class="form-group">
							<label for="service-select">AI Service</label>
							<select
								id="service-select"
								bind:value={selectedService}
								onchange={handleServiceChange}
								disabled={isLoadingModels}
							>
								<option value="">Select a service</option>
								{#each aiServices as service}
									<option value={service.iri}>{service.label}</option>
								{/each}
							</select>
						</div>

						{#if selectedService}
							<div class="form-group">
								<label for="model-select">AI Model</label>
								{#if isLoadingModels}
									<p class="loading">Loading models...</p>
								{:else}
									<select id="model-select" bind:value={selectedModel}>
										<option value="">Select a model</option>
										{#each aiModels as model}
											<option value={model.iri}>
												{model.label}
												{#if model.isDefault}(default){/if}
											</option>
										{/each}
									</select>
								{/if}
							</div>
						{/if}
					{/if}

					<div class="button-group">
						<button onclick={previousStep} class="secondary" disabled={isSubmitting}>
							Back
						</button>
						<button
							onclick={complete}
							disabled={isSubmitting || !selectedService || !selectedModel}
						>
							{isSubmitting ? 'Setting up...' : 'Complete Setup'}
						</button>
					</div>
				</div>
			{/if}
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
		min-width: 500px;
	}

	h1 {
		font-size: 1.5rem;
		color: var(--color-neutral);
		margin: 0;
		text-transform: uppercase;
		letter-spacing: 0.125rem;
	}

	h2 {
		font-size: 1.125rem;
		color: var(--color-neutral);
		margin: 0 0 1.5rem 0;
		text-align: center;
	}

	.step {
		display: flex;
		flex-direction: column;
		gap: 1.5rem;
		width: 100%;
	}

	.wizard-container :global(input) {
		width: 400px;
	}

	.form-group {
		display: flex;
		flex-direction: column;
		gap: 0.5rem;
	}

	label {
		font-size: 0.875rem;
		color: var(--color-neutral);
		font-weight: 500;
		text-transform: uppercase;
		letter-spacing: 0.05rem;
	}

	select {
		background: var(--color-surface);
		color: var(--color-neutral);
		border: 1px solid var(--color-border);
		padding: 0.75rem 1rem;
		font-size: 1rem;
		cursor: pointer;
		transition: all 0.2s;
	}

	select:hover:not(:disabled) {
		border-color: var(--color-interactive);
	}

	select:focus {
		outline: none;
		border-color: var(--color-interactive);
		box-shadow: 0 0 0 2px color-mix(in srgb, var(--color-interactive) 20%, transparent);
	}

	select:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.loading {
		text-align: center;
		color: var(--color-neutral-muted);
		font-style: italic;
	}

	.button-group {
		display: flex;
		gap: 1rem;
		justify-content: center;
		margin-top: 1rem;
	}

	button {
		background: var(--color-interactive);
		color: var(--color-neutral-on-interactive);
		border: none;
		padding: 1rem 3rem;
		font-size: 1.125rem;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.125rem;
		cursor: pointer;
		transition: all 0.2s;
	}

	button.secondary {
		background: var(--color-surface);
		color: var(--color-neutral);
		border: 1px solid var(--color-border);
	}

	button:hover:not(:disabled) {
		background: var(--color-interactive-hover);
		transform: translateY(-2px);
		box-shadow: 0 4px 16px color-mix(in srgb, var(--color-interactive) 40%, transparent);
	}

	button.secondary:hover:not(:disabled) {
		background: var(--color-surface-hover);
		border-color: var(--color-interactive);
	}

	button:disabled {
		background: var(--color-interactive-disabled);
		cursor: not-allowed;
		transform: none;
	}
</style>
