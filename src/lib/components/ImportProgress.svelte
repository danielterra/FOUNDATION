<script>
	import { onMount, onDestroy } from 'svelte';
	import { listen } from '@tauri-apps/api/event';
	import Activity from './Activity.svelte';

	let { onComplete = () => {} } = $props();

	let progress = $state(null);
	let unlistenProgress = $state(null);
	let unlistenComplete = $state(null);
	let unlistenError = $state(null);
	let error = $state(null);

	let percentage = $derived(progress ? Math.round((progress.current / progress.total) * 100) : 0);
	let message = $derived(
		progress
			? `${progress.stage}, ${progress.current_file}, ${progress.current} / ${progress.total} files • ${progress.triples.toLocaleString()} triples`
			: 'Initializing...'
	);

	onMount(async () => {

		// Listen for progress updates
		unlistenProgress = await listen('import-progress', (event) => {
			progress = event.payload;
		});

		// Listen for completion
		unlistenComplete = await listen('import-complete', () => {
			setTimeout(() => {
				onComplete();
			}, 500);
		});

		// Listen for errors
		unlistenError = await listen('import-error', (event) => {
			console.error('ImportProgress: Error event received:', event.payload);
			error = event.payload;
		});

	});

	onDestroy(() => {
		if (unlistenProgress) unlistenProgress();
		if (unlistenComplete) unlistenComplete();
		if (unlistenError) unlistenError();
	});
</script>

<div class="import-container">
	{#if error}
		<div class="error">
			<div class="error-icon">❌</div>
			<h2>Import Error</h2>
			<p>{error}</p>
		</div>
	{:else}
		<Activity {message} progress={progress ? percentage : null} />
	{/if}
</div>

<style>
	.import-container {
		display: flex;
		align-items: center;
		justify-content: center;
		min-height: 100vh;
	}

	.error {
		text-align: center;
		color: var(--color-danger);
	}

	.error-icon {
		font-size: 3rem;
		margin-bottom: 1rem;
	}

	.error h2 {
		font-size: 1.5rem;
		margin: 0 0 1rem 0;
		color: var(--color-danger);
	}

	.error p {
		font-size: 0.9rem;
		line-height: 1.6;
		color: var(--color-danger);
	}
</style>
