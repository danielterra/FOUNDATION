<script>
	import { onDestroy } from 'svelte';
	import { fade, fly } from 'svelte/transition';
	import { cubicOut } from 'svelte/easing';
	import { invoke } from '@tauri-apps/api/core';
	import { convertFileSrc } from '@tauri-apps/api/core';
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

	let activeClassFilter = $state(null);
	let showClassAutocomplete = $state(false);
	let classQuery = $state('');
	let classResults = $state([]);
	let classSelectedIndex = $state(-1);
	let classDebounceTimer;

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
		activeClassFilter = null;
		showClassAutocomplete = false;
		classResults = [];
		classQuery = '';
		classSelectedIndex = -1;
	}

	async function fetchClasses(query) {
		try {
			const json = await invoke('graph__search_entities', {
				query: query.trim(),
				limit: 50,
				typeIri: 'owl:Class'
			});
			classResults = JSON.parse(json);
			classSelectedIndex = classResults.length > 0 ? 0 : -1;
		} catch (err) {
			console.error('Class search failed:', err);
			classResults = [];
			classSelectedIndex = -1;
		}
	}

	async function performSearch(query) {
		const trimmed = query.trim();
		if (!activeClassFilter && trimmed.length < 2) {
			searchResults = [];
			showResults = false;
			selectedIndex = -1;
			return;
		}

		isSearching = true;
		try {
			const args = {
				query: trimmed,
				limit: 100
			};
			if (activeClassFilter) {
				args.typeIri = activeClassFilter.iri;
			}
			const resultsJson = await invoke('graph__search_entities', args);
			const results = JSON.parse(resultsJson);
			searchResults = results;
			showResults = results.length > 0;
			selectedIndex = results.length > 0 ? 0 : -1;
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
		if (debounceTimer) clearTimeout(debounceTimer);
		if (classDebounceTimer) clearTimeout(classDebounceTimer);

		const isClassTrigger = searchQuery.startsWith('@');

		if (isClassTrigger && !activeClassFilter) {
			classQuery = searchQuery.slice(1);
			showClassAutocomplete = true;
			showResults = false;
			classDebounceTimer = setTimeout(() => {
				fetchClasses(classQuery);
			}, 200);
			return;
		}

		showClassAutocomplete = false;

		if (!activeClassFilter && searchQuery.trim().length < 2) {
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

	function selectClass(cls) {
		activeClassFilter = { iri: cls.id, label: cls.label, icon: cls.icon ?? null };
		searchQuery = '';
		showClassAutocomplete = false;
		classResults = [];
		classQuery = '';
		classSelectedIndex = -1;
		performSearch(searchQuery);
	}

	function clearClassFilter() {
		activeClassFilter = null;
		performSearch(searchQuery);
	}

	function handleResultClick(result) {
		if (onSelectResult) {
			onSelectResult(result.id, result.label);
		}
		close();
	}

	function scrollToClassSelected() {
		setTimeout(() => {
			const el = document.querySelector('.class-item.selected');
			if (el) el.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
		}, 0);
	}

	function handleKeyDown(e) {
		if (showClassAutocomplete && classResults.length > 0) {
			switch (e.key) {
				case 'ArrowDown':
					e.preventDefault();
					classSelectedIndex = (classSelectedIndex + 1) % classResults.length;
					scrollToClassSelected();
					return;
				case 'ArrowUp':
					e.preventDefault();
					const last = classResults.length - 1;
					classSelectedIndex = classSelectedIndex <= 0 ? last : classSelectedIndex - 1;
					scrollToClassSelected();
					return;
				case 'Enter':
					e.preventDefault();
					if (classSelectedIndex >= 0 && classSelectedIndex < classResults.length) {
						selectClass(classResults[classSelectedIndex]);
					}
					return;
				case 'Escape':
					e.preventDefault();
					showClassAutocomplete = false;
					classResults = [];
					searchQuery = '';
					return;
				case 'Backspace':
					if (classQuery === '') {
						e.preventDefault();
						showClassAutocomplete = false;
						classResults = [];
						searchQuery = '';
					}
					return;
			}
		}

		if (!showResults || searchResults.length === 0) {
			if (e.key === 'Escape') {
				e.preventDefault();
				close();
			}
			if (e.key === 'Backspace' && activeClassFilter && searchQuery === '') {
				clearClassFilter();
			}
			return;
		}

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
			case 'Backspace':
				if (activeClassFilter && searchQuery === '') {
					clearClassFilter();
				}
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

	function isIconUrl(icon) {
		if (!icon) return false;
		return icon.startsWith('http://') || icon.startsWith('https://') ||
			icon.startsWith('data:') || icon.startsWith('file://') || icon.startsWith('/');
	}

	function getIconUrl(icon) {
		if (!icon) return '';
		if (icon.startsWith('http://') || icon.startsWith('https://') || icon.startsWith('data:'))
			return icon;
		if (icon.startsWith('file://')) return convertFileSrc(icon.replace(/^file:\/\//, ''));
		if (icon.startsWith('/')) return convertFileSrc(icon);
		return icon;
	}

	function shortenIri(iri) {
		const colonIndex = iri.indexOf(':');
		return colonIndex >= 0 ? iri.slice(colonIndex + 1) : iri;
	}

	onDestroy(() => {
		if (debounceTimer) clearTimeout(debounceTimer);
		if (classDebounceTimer) clearTimeout(classDebounceTimer);
	});
</script>

{#if isOpen}
	<div class="search-overlay" role="presentation" transition:fade={{ duration: 200 }}>
		<div class="search-container" transition:fly={{ y: -16, duration: 220, easing: cubicOut }}>
			<Card>
				<div class="search-input-wrapper">
					<span class="material-symbols-outlined search-icon">search</span>
					{#if activeClassFilter}
						<span class="class-filter-pill">
							<span class="material-symbols-outlined pill-icon">
								{activeClassFilter.icon ?? 'category'}
							</span>
							@{activeClassFilter.label}
						</span>
					{/if}
					<input
						bind:this={inputElement}
						type="text"
						bind:value={searchQuery}
						placeholder={activeClassFilter
								? `Buscar em ${activeClassFilter.label}...`
								: 'Buscar entidades... (@ para filtrar por classe)'}
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

			{#if showClassAutocomplete && classResults.length > 0}
				<div class="search-results">
					<Card>
						<div class="results-list">
							{#each classResults as cls, index}
								<button
									type="button"
									class="result-item class-item"
									class:selected={index === classSelectedIndex}
									onmousedown={(e) => { e.preventDefault(); selectClass(cls); }}
								>
									<div class="result-content">
										{#if cls.icon}
											{#if isIconUrl(cls.icon)}
												<img src={getIconUrl(cls.icon)} alt="" class="result-icon-image" />
											{:else}
												<span class="material-symbols-outlined result-icon">{cls.icon}</span>
											{/if}
										{:else}
											<span class="material-symbols-outlined result-icon">category</span>
										{/if}
										<div class="result-text">
											<div class="result-label">@{cls.label}</div>
										</div>
									</div>
								</button>
							{/each}
						</div>
					</Card>
				</div>
			{/if}

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
											{#if isIconUrl(result.icon)}
												<img src={getIconUrl(result.icon)} alt="" class="result-icon-image" />
											{:else}
												<span class="material-symbols-outlined result-icon">{result.icon}</span>
											{/if}
										{:else}
											<div class="result-placeholder"></div>
										{/if}
										<div class="result-text">
											<div class="result-label">{result.label}</div>
											{#if result.classType}
												<div class="class-type">
													{#if result.classType.icon}
														<span class="material-symbols-outlined class-type-icon">{result.classType.icon}</span>
													{/if}
													<span>{result.classType.label}</span>
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
		background: color-mix(in srgb, var(--color-black) 40%, transparent);
		backdrop-filter: blur(6px);
		-webkit-backdrop-filter: blur(6px);
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
		flex-wrap: wrap;
	}

	.search-icon {
		color: var(--color-neutral);
		font-size: 20px;
		flex-shrink: 0;
	}

	.class-filter-pill {
		display: inline-flex;
		align-items: center;
		gap: 0.25rem;
		padding: 0.125rem 0.5rem;
		border-radius: 999px;
		background: color-mix(in srgb, var(--color-interactive) 15%, transparent);
		color: var(--color-interactive);
		font-size: 0.75rem;
		font-weight: 600;
		flex-shrink: 0;
		white-space: nowrap;
	}

	.pill-icon {
		font-size: 14px;
	}

	.search-input {
		flex: 1;
		min-width: 120px;
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
	}

	.result-item.selected {
		background: color-mix(in srgb, var(--color-interactive) 10%, transparent);
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

	.result-icon-image {
		width: 24px;
		height: 24px;
		object-fit: contain;
		flex-shrink: 0;
	}

	.result-placeholder {
		width: 24px;
		height: 24px;
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

	.class-type {
		display: flex;
		align-items: center;
		gap: 0.25rem;
		color: var(--color-neutral);
		font-size: 0.75rem;
	}

	.class-type-icon {
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
