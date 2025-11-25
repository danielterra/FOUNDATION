<script>
	import { invoke } from '@tauri-apps/api/core';
	import { createEventDispatcher } from 'svelte';

	export let onSelectClass;

	const dispatch = createEventDispatcher();

	let searchQuery = '';
	let searchResults = [];
	let showResults = false;
	let searching = false;
	let selectedIndex = -1;

	let debounceTimer;

	async function performSearch() {
		if (searchQuery.trim().length < 2) {
			searchResults = [];
			showResults = false;
			return;
		}

		searching = true;
		try {
			const resultsJson = await invoke('search_classes', { query: searchQuery });
			searchResults = JSON.parse(resultsJson);
			showResults = searchResults.length > 0;
			selectedIndex = -1;
		} catch (err) {
			console.error('Search failed:', err);
			searchResults = [];
		} finally {
			searching = false;
		}
	}

	function handleInput() {
		clearTimeout(debounceTimer);
		debounceTimer = setTimeout(performSearch, 300);
	}

	function selectResult(result) {
		searchQuery = '';
		searchResults = [];
		showResults = false;
		selectedIndex = -1;
		if (onSelectClass) {
			onSelectClass(result.id, result.label);
		}
	}

	function handleKeydown(event) {
		if (!showResults || searchResults.length === 0) return;

		if (event.key === 'ArrowDown') {
			event.preventDefault();
			selectedIndex = Math.min(selectedIndex + 1, searchResults.length - 1);
		} else if (event.key === 'ArrowUp') {
			event.preventDefault();
			selectedIndex = Math.max(selectedIndex - 1, -1);
		} else if (event.key === 'Enter' && selectedIndex >= 0) {
			event.preventDefault();
			selectResult(searchResults[selectedIndex]);
		} else if (event.key === 'Escape') {
			showResults = false;
			selectedIndex = -1;
		}
	}

	function handleBlur() {
		// Delay to allow click on results
		setTimeout(() => {
			showResults = false;
		}, 200);
	}

	// Color mapping for groups
	const groupColors = {
		1: '#4a9eff',
		2: '#ff6b9d',
		3: '#50fa7b'
	};
</script>

<div class="search-container">
	<div class="search-input-wrapper">
		<input
			type="text"
			bind:value={searchQuery}
			on:input={handleInput}
			on:keydown={handleKeydown}
			on:blur={handleBlur}
			on:focus={() => {
				if (searchResults.length > 0) showResults = true;
			}}
			placeholder="Search classes..."
			class="search-input"
		/>
		{#if searching}
			<div class="search-spinner">⏳</div>
		{/if}
	</div>

	{#if showResults}
		<div class="search-results">
			{#each searchResults as result, index}
				<button
					class="search-result-item"
					class:selected={index === selectedIndex}
					on:click={() => selectResult(result)}
				>
					<div class="result-header">
						<span class="result-indicator" style="background-color: {groupColors[result.group]}" />
						<span class="result-label">{result.label}</span>
					</div>
					{#if result.definition}
						<div class="result-definition">
							{result.definition.substring(0, 100)}{result.definition.length > 100 ? '...' : ''}
						</div>
					{/if}
				</button>
			{/each}
		</div>
	{/if}
</div>

<style>
	.search-container {
		position: fixed;
		bottom: 40px;
		left: 50%;
		transform: translateX(-50%);
		z-index: 1000;
		width: 600px;
		max-width: 90vw;
	}

	.search-input-wrapper {
		position: relative;
		width: 100%;
	}

	.search-input {
		width: 100%;
		padding: 14px 20px;
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		font-size: 16px;
		background: rgba(20, 20, 20, 0.95);
		border: 1px solid rgba(255, 255, 255, 0.2);
		border-radius: 8px;
		color: rgba(255, 255, 255, 0.9);
		outline: none;
		transition: all 0.2s ease;
		backdrop-filter: blur(10px);
	}

	.search-input:focus {
		border-color: rgba(255, 255, 255, 0.4);
		box-shadow: 0 0 0 3px rgba(255, 255, 255, 0.1);
	}

	.search-input::placeholder {
		color: rgba(255, 255, 255, 0.4);
	}

	.search-spinner {
		position: absolute;
		right: 16px;
		top: 50%;
		transform: translateY(-50%);
		font-size: 18px;
	}

	.search-results {
		position: absolute;
		bottom: 100%;
		left: 0;
		right: 0;
		margin-bottom: 8px;
		background: rgba(20, 20, 20, 0.98);
		border: 1px solid rgba(255, 255, 255, 0.2);
		border-radius: 8px;
		overflow: hidden;
		backdrop-filter: blur(10px);
		box-shadow: 0 -4px 20px rgba(0, 0, 0, 0.5);
	}

	.search-result-item {
		width: 100%;
		padding: 12px 16px;
		text-align: left;
		background: transparent;
		border: none;
		border-bottom: 1px solid rgba(255, 255, 255, 0.1);
		cursor: pointer;
		transition: background 0.15s ease;
		color: inherit;
		font-family: inherit;
	}

	.search-result-item:last-child {
		border-bottom: none;
	}

	.search-result-item:hover,
	.search-result-item.selected {
		background: rgba(255, 255, 255, 0.1);
	}

	.result-header {
		display: flex;
		align-items: center;
		gap: 10px;
		margin-bottom: 4px;
	}

	.result-indicator {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		flex-shrink: 0;
	}

	.result-label {
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		font-size: 14px;
		font-weight: 500;
		color: rgba(255, 255, 255, 0.95);
	}

	.result-definition {
		font-family: 'Science Gothic SemiCondensed Light', 'Science Gothic', sans-serif;
		font-size: 12px;
		color: rgba(255, 255, 255, 0.6);
		line-height: 1.4;
		margin-left: 18px;
	}
</style>
