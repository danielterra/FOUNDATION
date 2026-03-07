<script>
	import { onDestroy } from 'svelte';
	import { invoke } from '@tauri-apps/api/core';
	import Card from '$lib/components/Card.svelte';

	let { onSelectResult } = $props();

	let isOpen = $state(false);
	let searchQuery = $state('');
	let searchResults = $state([]);
	let isSearching = $state(false);
	let showResults = $state(false);
	let selectedIndex = $state(-1);
	let inputElement = $state();
	let debounceTimer;

	export function open() {
		isOpen = true;
		setTimeout(() => {
			if (inputElement) inputElement.focus();
		}, 0);
	}

	export function close() {
		isOpen = false;
		searchQuery = '';
		searchResults = [];
		showResults = false;
		selectedIndex = -1;
	}

	async function performSearch(query) {
		if (!query || query.trim().length < 2) {
			searchResults = [];
			showResults = false;
			selectedIndex = -1;
			return;
		}

		isSearching = true;
		try {
			const resultsJson = await invoke('entity__search', {
				query: query.trim(),
				limit: 100
			});
			const results = JSON.parse(resultsJson);
			searchResults = results;
			showResults = results.length > 0;
			selectedIndex = results.length > 0 ? 0 : -1; // Auto-select first result
		} catch (err) {
			console.error('Search failed:', err);
			searchResults = [];
			showResults = false;
			selectedIndex = -1;
		} finally {
			isSearching = false;
		}
	}

	function handleInput() {
		if (debounceTimer) {
			clearTimeout(debounceTimer);
		}

		if (!searchQuery || searchQuery.trim().length < 2) {
			searchResults = [];
			showResults = false;
			selectedIndex = -1;
			isSearching = false;
			return;
		}

		isSearching = true;

		debounceTimer = setTimeout(() => {
			performSearch(searchQuery);
		}, 500);
	}

	function handleResultClick(result) {
		if (onSelectResult) {
			onSelectResult(result.id, result.label);
		}
		searchQuery = '';
		searchResults = [];
		showResults = false;
		selectedIndex = -1;
	}

	function handleKeyDown(e) {
		if (!showResults || searchResults.length === 0) return;

		switch (e.key) {
			case 'ArrowDown':
				e.preventDefault();
				selectedIndex = (selectedIndex + 1) % searchResults.length;
				scrollToSelected();
				break;
			case 'ArrowUp':
				e.preventDefault();
				selectedIndex = selectedIndex <= 0 ? searchResults.length - 1 : selectedIndex - 1;
				scrollToSelected();
				break;
			case 'Enter':
				e.preventDefault();
				if (selectedIndex >= 0 && selectedIndex < searchResults.length) {
					handleResultClick(searchResults[selectedIndex]);
				}
				break;
			case 'Escape':
				e.preventDefault();
				close();
				break;
		}
	}

	function scrollToSelected() {
		setTimeout(() => {
			const selectedEl = document.querySelector('.result-item.selected');
			if (selectedEl) {
				selectedEl.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
			}
		}, 0);
	}

	function handleBlur() {
		setTimeout(() => {
			close();
		}, 200);
	}

	function handleFocus() {
		if (searchResults.length > 0) {
			showResults = true;
			if (selectedIndex === -1) {
				selectedIndex = 0;
			}
		}
	}

	function shortenIri(iri) {
		const colonIndex = iri.indexOf(':');
		return colonIndex >= 0 ? iri.slice(colonIndex + 1) : iri;
	}

	onDestroy(() => {
		if (debounceTimer) {
			clearTimeout(debounceTimer);
		}
	});
</script>

{#if isOpen}
	<div class="search-overlay" role="presentation">
		<div class="search-container">
			<Card>
				<div class="search-input-wrapper">
					<span class="material-symbols-outlined search-icon">search</span>
					<input
						bind:this={inputElement}
						type="text"
						bind:value={searchQuery}
						placeholder="Search entities..."
						oninput={handleInput}
						onfocus={handleFocus}
						onblur={handleBlur}
						onkeydown={handleKeyDown}
						class="search-input"
						autocomplete="off"
						autocorrect="off"
						autocapitalize="off"
						spellcheck="false"
					/>
					{#if isSearching}
						<span class="material-symbols-outlined loading-icon">progress_activity</span>
					{/if}
				</div>
			</Card>

			{#if showResults && searchResults.length > 0}
				<div class="search-results">
					<Card>
						<div class="results-list">
							{#each searchResults as result, index}
								<button
									type="button"
									class="result-item"
									class:selected={index === selectedIndex}
									onclick={() => handleResultClick(result)}
								>
									<div class="result-content">
										{#if result.icon}
											<span class="material-symbols-outlined result-icon">{result.icon}</span>
										{:else}
											<div class="result-placeholder"></div>
										{/if}
										<div class="result-text">
											<div class="result-label">{result.label}</div>
											{#if result.conceptType}
												<div class="concept-type">
													{#if result.conceptType.icon}
														<span class="material-symbols-outlined concept-type-icon">{result.conceptType.icon}</span>
													{/if}
													<span>{result.conceptType.label}</span>
												</div>
											{/if}
											{#if result.status}
												<div class="status-badge" style="color: {result.status.color}">
													{#if result.status.icon}
														<span class="material-symbols-outlined status-icon">{result.status.icon}</span>
													{/if}
													<span>{result.status.label}</span>
												</div>
											{/if}
											{#if result.matchedProperties && result.matchedProperties.length > 0}
												<div class="matched-props">
													{#each result.matchedProperties as mp}
														<div class="matched-prop-row">
															<span class="matched-prop-key">{shortenIri(mp.detail_iri)}</span>
															<span class="matched-prop-value">{mp.value}</span>
														</div>
													{/each}
												</div>
											{/if}
										</div>
									</div>
								</button>
							{/each}
						</div>
					</Card>
				</div>
			{/if}
		</div>
	</div>
{/if}

<style>
	.search-overlay {
		position: fixed;
		inset: 0;
		z-index: 2000;
		display: flex;
		justify-content: center;
		padding-top: 80px;
	}

	.search-container {
		position: relative;
		width: 480px;
		height: fit-content;
	}

	.search-input-wrapper {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		position: relative;
	}

	.search-icon {
		color: var(--color-neutral);
		font-size: 20px;
		flex-shrink: 0;
	}

	.search-input {
		flex: 1;
		background: transparent;
		border: none;
		color: var(--color-neutral-active);
		font-size: 0.875rem;
		padding: 0;
		outline: none;
	}

	.search-input::placeholder {
		color: var(--color-neutral);
	}

	.loading-icon {
		color: var(--color-interactive);
		font-size: 18px;
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		from {
			transform: rotate(0deg);
		}
		to {
			transform: rotate(360deg);
		}
	}

	.search-results {
		position: absolute;
		top: calc(100% + 0.5rem);
		left: 0;
		right: 0;
		z-index: 1001;
	}

	.results-list {
		display: flex;
		flex-direction: column;
		gap: 0;
		max-height: 500px;
		overflow-y: auto;
		scroll-behavior: smooth;
	}

	.result-item {
		width: 100%;
		padding: 0.75rem;
		background: transparent;
		border: none;
		cursor: pointer;
		text-align: left;
		transition: background 0.15s;
		border-bottom: 1px solid color-mix(in srgb, var(--color-white) 5%, transparent);
	}

	.result-item:last-child {
		border-bottom: none;
	}

	.result-item:hover,
	.result-item.selected {
		background: color-mix(in srgb, var(--color-interactive) 10%, transparent);
	}

	.result-item.selected {
		outline: 2px solid var(--color-interactive);
		outline-offset: -2px;
	}

	.result-content {
		display: flex;
		align-items: center;
		gap: 0.75rem;
	}

	.result-icon {
		color: var(--color-interactive);
		font-size: 24px;
		flex-shrink: 0;
	}

	.result-placeholder {
		width: 24px;
		height: 24px;
		border-radius: 50%;
		background: color-mix(in srgb, var(--color-interactive) 30%, transparent);
		flex-shrink: 0;
	}

	.result-text {
		display: flex;
		flex-direction: column;
		gap: 0.25rem;
		overflow: hidden;
	}

	.result-label {
		color: var(--color-neutral-active);
		font-size: 0.875rem;
		font-weight: 500;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.concept-type {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		color: var(--color-neutral);
		font-size: 0.75rem;
	}

	.concept-type-icon {
		font-size: 14px;
	}

	.status-badge {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		font-size: 0.75rem;
		font-weight: 500;
	}

	.status-icon {
		font-size: 14px;
	}

	.matched-props {
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
		margin-top: 0.25rem;
	}

	.matched-prop-row {
		display: flex;
		gap: 0.375rem;
		font-size: 0.7rem;
		color: var(--color-neutral);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.matched-prop-key {
		font-weight: 600;
		flex-shrink: 0;
	}

	.matched-prop-value {
		overflow: hidden;
		text-overflow: ellipsis;
	}
</style>
