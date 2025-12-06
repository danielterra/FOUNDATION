<script>
	import { invoke } from '@tauri-apps/api/core';
	import { onMount } from 'svelte';

	export let onComplete = () => {};

	let personName = '';
	let computerName = '';
	let computerProcessor = '';
	let computerMemory = '';
	let isSubmitting = false;

	onMount(async () => {
		// Get computer info from system automatically
		try {
			const sysInfo = await invoke('get_system_info');
			computerName = sysInfo.hostname || 'My Computer';
			computerProcessor = sysInfo.cpu_brand || '';
			computerMemory = Math.round(sysInfo.total_memory_gb).toString();
		} catch (e) {
			console.error('Failed to get system info:', e);
			computerName = 'My Computer';
		}
	});

	async function complete() {
		if (!personName.trim()) return;

		isSubmitting = true;

		try {
			const setupData = {
				person_name: personName,
				person_email: null,
				computer_name: computerName || 'My Computer',
				computer_processor: computerProcessor || null,
				computer_memory: computerMemory ? parseInt(computerMemory) : null
			};

			await invoke('complete_initial_setup', {
				setupData: JSON.stringify(setupData)
			});

			onComplete();
		} catch (e) {
			console.error('Setup failed:', e);
			isSubmitting = false;
		}
	}
</script>

<div class="wizard-overlay">
	<div class="wizard-container">
		<h1>Setup</h1>

		<input
			type="text"
			bind:value={personName}
			placeholder="Your name"
			autofocus
			on:keydown={(e) => e.key === 'Enter' && complete()}
		/>

		<button on:click={complete} disabled={isSubmitting || !personName.trim()}>
			DONE
		</button>
	</div>
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
		gap: 32px;
		padding: 60px;
	}

	h1 {
		font-family: 'Science Gothic SemiCondensed', sans-serif;
		font-size: 24px;
		color: rgba(255, 255, 255, 0.5);
		margin: 0;
		text-transform: uppercase;
		letter-spacing: 2px;
	}

	input {
		width: 400px;
		padding: 16px 20px;
		background: rgba(0, 0, 0, 0.3);
		border: 1px solid rgba(255, 255, 255, 0.1);
		border-radius: 8px;
		color: #fff;
		font-family: 'Science Gothic SemiCondensed Light', sans-serif;
		font-size: 18px;
		text-align: center;
		transition: all 0.2s;
	}

	input:focus {
		outline: none;
		border-color: rgba(255, 255, 255, 0.3);
		background: rgba(0, 0, 0, 0.4);
	}

	input::placeholder {
		color: rgba(255, 255, 255, 0.3);
	}

	button {
		background: #ff8c42;
		color: #000;
		border: none;
		padding: 16px 48px;
		border-radius: 8px;
		font-family: 'Science Gothic SemiCondensed', sans-serif;
		font-size: 18px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 2px;
		cursor: pointer;
		transition: all 0.2s;
	}

	button:hover:not(:disabled) {
		background: #ffa366;
		transform: translateY(-2px);
		box-shadow: 0 4px 16px rgba(255, 140, 66, 0.4);
	}

	button:disabled {
		opacity: 0.3;
		cursor: not-allowed;
		transform: none;
	}
</style>
