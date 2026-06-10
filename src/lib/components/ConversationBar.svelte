<script>
	import { invoke } from '@tauri-apps/api/core';
	import { tick } from 'svelte';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';

	let {
		conversations = $bindable([]),
		activeConversationIri = $bindable(null),
		onSwitch,
		onDelete,
	} = $props();

	let isOpen = $state(false);
	let searchQuery = $state('');
	let renamingIri = $state(null);
	let renameText = $state('');
	let confirmDeleteIri = $state(null);
	let searchInputEl = $state(null);
	let renameInputEl = $state(null);

	const activeLabel = $derived(
		conversations.find(c => c.iri === activeConversationIri)?.label ?? 'Select conversation'
	);

	const filteredConversations = $derived(
		searchQuery.trim()
			? conversations.filter(c => c.label.toLowerCase().includes(searchQuery.toLowerCase().trim()))
			: conversations
	);

	async function openDropdown() {
		isOpen = true;
		searchQuery = '';
		renamingIri = null;
		confirmDeleteIri = null;
		await tick();
		searchInputEl?.focus();
	}

	function closeDropdown() {
		isOpen = false;
		searchQuery = '';
		renamingIri = null;
		confirmDeleteIri = null;
	}

	function selectConversation(iri) {
		activeConversationIri = iri;
		closeDropdown();
		onSwitch(iri);
	}

	function startRename(iri) {
		const conv = conversations.find(c => c.iri === iri);
		renameText = conv?.label ?? '';
		renamingIri = iri;
		confirmDeleteIri = null;
	}

	async function confirmRename() {
		const label = renameText.trim();
		if (!label || !renamingIri) { renamingIri = null; return; }
		const iri = renamingIri;
		renamingIri = null;
		try {
			await invoke('chat__rename_conversation', { conversationId: iri, label });
			conversations = conversations.map(c => c.iri === iri ? { ...c, label } : c);
		} catch (err) {
			console.error('Failed to rename:', err);
			conversations = await invoke('chat__list_conversations');
		}
	}

	function startDelete(iri) {
		confirmDeleteIri = iri;
		renamingIri = null;
	}

	async function confirmDelete() {
		const iri = confirmDeleteIri;
		if (!iri) return;
		confirmDeleteIri = null;
		try {
			await invoke('chat__delete_conversation', { conversationId: iri });
			conversations = conversations.filter(c => c.iri !== iri);
			onDelete?.(iri);
		} catch (err) {
			console.error('Failed to delete:', err);
			conversations = await invoke('chat__list_conversations');
		}
	}

	$effect(() => {
		if (renamingIri && renameInputEl) {
			renameInputEl.focus();
		}
	});
</script>

<div class="conversation-bar">
	<Button
		variant="ghost"
		class={`conversation-trigger${isOpen ? ' open' : ''}`}
		onclick={() => isOpen ? closeDropdown() : openDropdown()}
		title="Switch conversation"
	>
		<span class="trigger-label">{activeLabel}</span>
		<span class="material-symbols-outlined trigger-chevron" class:rotated={isOpen}>expand_more</span>
	</Button>

	{#if isOpen}
		<div class="dropdown-backdrop" onclick={closeDropdown} role="presentation"></div>
		<div class="conversation-dropdown">
			<div class="dropdown-search-row">
				<span class="material-symbols-outlined search-icon">search</span>
				<Input
					bind:ref={searchInputEl}
					bind:value={searchQuery}
					class="dropdown-search-input"
					placeholder="Search conversations…"
					onkeydown={(e) => e.key === 'Escape' && closeDropdown()}
				/>
			</div>
			<div class="dropdown-list">
				{#each filteredConversations as conv (conv.iri)}
					<div class="dropdown-item" class:active={conv.iri === activeConversationIri}>
						{#if renamingIri === conv.iri}
							<Input
								bind:ref={renameInputEl}
								class="item-rename-input"
								bind:value={renameText}
								onkeydown={(e) => {
									if (e.key === 'Enter') confirmRename();
									else if (e.key === 'Escape') { renamingIri = null; }
								}}
							/>
							<Button variant="ghost" size="icon-sm" onclick={confirmRename} title="Confirm" aria-label="Confirm rename">
								<span class="material-symbols-outlined">check</span>
							</Button>
							<Button variant="ghost" size="icon-sm" onclick={() => renamingIri = null} title="Cancel" aria-label="Cancel rename">
								<span class="material-symbols-outlined">close</span>
							</Button>
						{:else if confirmDeleteIri === conv.iri}
							<span class="item-delete-label">Delete?</span>
							<Button variant="ghost" size="icon-sm" onclick={confirmDelete} title="Confirm delete" aria-label="Confirm delete">
								<span class="material-symbols-outlined">check</span>
							</Button>
							<Button variant="ghost" size="icon-sm" onclick={() => confirmDeleteIri = null} title="Cancel" aria-label="Cancel delete">
								<span class="material-symbols-outlined">close</span>
							</Button>
						{:else}
							<Button variant="ghost" class="item-name" onclick={() => selectConversation(conv.iri)}>
								{conv.label}
							</Button>
							<div class="item-actions">
								<Button
									variant="ghost"
									size="icon-sm"
									onclick={(e) => { e.stopPropagation(); startRename(conv.iri); }}
									title="Rename"
									aria-label="Rename conversation"
								>
									<span class="material-symbols-outlined">edit</span>
								</Button>
								<Button
									variant="ghost"
									size="icon-sm"
									onclick={(e) => { e.stopPropagation(); startDelete(conv.iri); }}
									title="Delete"
									aria-label="Delete conversation"
									class="danger-btn"
								>
									<span class="material-symbols-outlined">delete</span>
								</Button>
							</div>
						{/if}
					</div>
				{/each}
				{#if filteredConversations.length === 0}
					<div class="dropdown-empty">No conversations found</div>
				{/if}
			</div>
		</div>
	{/if}
</div>

<style>
	.conversation-bar {
		position: relative;
		display: flex;
		align-items: center;
		padding: 0 10px 0 14px;
		flex-shrink: 0;
	}

	:global([data-slot="button"].conversation-trigger) {
		flex: 1;
		display: flex;
		align-items: center;
		gap: 4px;
		color: var(--foreground);
		font-size: 12px;
		padding: 6px 0;
		min-width: 0;
		height: auto;
		justify-content: flex-start;
	}

	:global([data-slot="button"].conversation-trigger:hover) {
		background: transparent;
		color: var(--accent-foreground);
	}

	.trigger-label {
		flex: 1;
		overflow: hidden;
		white-space: nowrap;
		text-overflow: ellipsis;
	}

	.trigger-chevron {
		font-size: 16px;
		flex-shrink: 0;
		transition: transform 0.15s;
	}

	.trigger-chevron.rotated {
		transform: rotate(180deg);
	}

	.dropdown-backdrop {
		position: fixed;
		inset: 0;
		z-index: 99;
	}

	.conversation-dropdown {
		position: absolute;
		top: calc(100% + 1px);
		left: 0;
		right: 0;
		z-index: 100;
		background: var(--popover);
		box-shadow: 0 8px 24px color-mix(in srgb, var(--color-black) 60%, transparent);
		border-radius: var(--radius);
		display: flex;
		flex-direction: column;
		max-height: 320px;
		overflow: hidden;
	}

	.dropdown-search-row {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 8px 10px;
		flex-shrink: 0;
	}

	.search-icon {
		font-size: 14px;
		color: var(--muted-foreground);
		flex-shrink: 0;
	}

	:global(.dropdown-search-input) {
		background: transparent !important;
		border: none !important;
		box-shadow: none !important;
		font-size: 12px !important;
		height: auto !important;
		padding: 0 !important;
	}

	.dropdown-list {
		overflow-y: auto;
		flex: 1;
	}

	.dropdown-item {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 0 6px 0 10px;
		min-height: 32px;
	}

	.dropdown-item:hover .item-actions,
	.dropdown-item.active .item-actions {
		opacity: 1;
	}

	.dropdown-item.active {
		background: color-mix(in srgb, var(--primary) 12%, transparent);
	}

	.dropdown-item:hover:not(.active) {
		background: var(--accent);
	}

	:global([data-slot="button"].item-name) {
		flex: 1;
		color: var(--foreground);
		font-size: 12px;
		text-align: left;
		padding: 8px 0;
		height: auto;
		justify-content: flex-start;
		overflow: hidden;
		white-space: nowrap;
		text-overflow: ellipsis;
		min-width: 0;
	}

	:global([data-slot="button"].item-name:hover) {
		background: transparent;
		color: var(--accent-foreground);
	}

	.dropdown-item.active :global([data-slot="button"].item-name) {
		color: var(--accent-foreground);
	}

	.item-actions {
		display: flex;
		align-items: center;
		gap: 2px;
		flex-shrink: 0;
		opacity: 0;
		transition: opacity 0.1s;
	}

	:global(.item-rename-input) {
		flex: 1 !important;
		background: transparent !important;
		border: none !important;
		box-shadow: none !important;
		font-size: 12px !important;
		height: auto !important;
		padding: 2px 0 !important;
		min-width: 0 !important;
	}

	.item-delete-label {
		flex: 1;
		font-size: 12px;
		color: var(--destructive);
	}

	:global(.danger-btn:hover) {
		color: var(--destructive) !important;
	}

	.dropdown-empty {
		padding: 12px 10px;
		font-size: 12px;
		color: var(--muted-foreground);
		text-align: center;
	}
</style>
