<script>
	import { invoke } from '@tauri-apps/api/core';
	import { tick } from 'svelte';
	import { focus } from '$lib/utils/actions';

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
</script>

<div class="conversation-bar">
	<button
		class="conversation-trigger"
		class:open={isOpen}
		onclick={() => isOpen ? closeDropdown() : openDropdown()}
		title="Switch conversation"
	>
		<span class="trigger-label">{activeLabel}</span>
		<span class="material-symbols-outlined trigger-chevron" class:rotated={isOpen}>expand_more</span>
	</button>

	{#if isOpen}
		<div class="dropdown-backdrop" onclick={closeDropdown} role="presentation"></div>
		<div class="conversation-dropdown">
			<div class="dropdown-search-row">
				<span class="material-symbols-outlined search-icon">search</span>
				<input
					bind:this={searchInputEl}
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
							<input
								class="item-rename-input"
								bind:value={renameText}
								onkeydown={(e) => {
									if (e.key === 'Enter') confirmRename();
									else if (e.key === 'Escape') { renamingIri = null; }
								}}
								use:focus
							/>
							<button class="item-icon-btn confirm" onclick={confirmRename} title="Confirm">
								<span class="material-symbols-outlined">check</span>
							</button>
							<button class="item-icon-btn cancel" onclick={() => renamingIri = null} title="Cancel">
								<span class="material-symbols-outlined">close</span>
							</button>
						{:else if confirmDeleteIri === conv.iri}
							<span class="item-delete-label">Delete?</span>
							<button class="item-icon-btn confirm" onclick={confirmDelete} title="Confirm delete">
								<span class="material-symbols-outlined">check</span>
							</button>
							<button class="item-icon-btn cancel" onclick={() => confirmDeleteIri = null} title="Cancel">
								<span class="material-symbols-outlined">close</span>
							</button>
						{:else}
							<button class="item-name" onclick={() => selectConversation(conv.iri)}>
								{conv.label}
							</button>
							<div class="item-actions">
								<button
									class="item-icon-btn"
									onclick={(e) => { e.stopPropagation(); startRename(conv.iri); }}
									title="Rename"
								>
									<span class="material-symbols-outlined">edit</span>
								</button>
								<button
									class="item-icon-btn danger"
									onclick={(e) => { e.stopPropagation(); startDelete(conv.iri); }}
									title="Delete"
								>
									<span class="material-symbols-outlined">delete</span>
								</button>
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

	.conversation-trigger {
		flex: 1;
		display: flex;
		align-items: center;
		gap: 4px;
		background: transparent;
		border: none;
		color: var(--color-neutral);
		font-size: 12px;
		cursor: pointer;
		padding: 6px 0;
		min-width: 0;
		text-align: left;
	}

	.conversation-trigger:hover {
		color: var(--color-neutral-active);
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
		background: color-mix(in srgb, var(--color-white) 6%, var(--color-black));
		box-shadow: 0 8px 24px color-mix(in srgb, var(--color-black) 60%, transparent);
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
		color: var(--color-neutral-disabled);
		flex-shrink: 0;
	}

	.dropdown-search-input {
		flex: 1;
		background: transparent;
		border: none;
		color: var(--color-neutral-active);
		font-size: 12px;
		min-width: 0;
	}

	.dropdown-search-input:focus {
		outline: none;
	}

	.dropdown-search-input::placeholder {
		color: var(--color-neutral-disabled);
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
		background: color-mix(in srgb, var(--color-interactive) 12%, transparent);
	}

	.dropdown-item:hover:not(.active) {
		background: color-mix(in srgb, var(--color-white) 5%, transparent);
	}

	.item-name {
		flex: 1;
		background: transparent;
		border: none;
		color: var(--color-neutral);
		font-size: 12px;
		cursor: pointer;
		text-align: left;
		padding: 8px 0;
		overflow: hidden;
		white-space: nowrap;
		text-overflow: ellipsis;
		min-width: 0;
	}

	.dropdown-item.active .item-name {
		color: var(--color-neutral-active);
	}

	.item-name:hover {
		color: var(--color-neutral-active);
	}

	.item-actions {
		display: flex;
		align-items: center;
		gap: 2px;
		flex-shrink: 0;
		opacity: 0;
		transition: opacity 0.1s;
	}

	.item-rename-input {
		flex: 1;
		background: transparent;
		border: none;
		color: var(--color-neutral-active);
		font-size: 12px;
		padding: 2px 0;
		min-width: 0;
	}

	.item-rename-input:focus {
		outline: none;
	}

	.item-delete-label {
		flex: 1;
		font-size: 12px;
		color: var(--color-danger);
	}

	.item-icon-btn {
		width: 22px;
		height: 22px;
		background: transparent;
		border: none;
		color: var(--color-neutral-disabled);
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		padding: 0;
		transition: all 0.1s;
	}

	.item-icon-btn:hover {
		color: var(--color-neutral-active);
		background: color-mix(in srgb, var(--color-white) 8%, transparent);
	}

	.item-icon-btn.confirm:hover {
		color: var(--color-success);
	}

	.item-icon-btn.cancel:hover {
		color: var(--color-danger);
	}

	.item-icon-btn.danger:hover {
		color: var(--color-danger);
	}

	.item-icon-btn .material-symbols-outlined {
		font-size: 14px;
	}

	.dropdown-empty {
		padding: 12px 10px;
		font-size: 12px;
		color: var(--color-neutral-disabled);
		text-align: center;
	}
</style>
