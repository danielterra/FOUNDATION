<script>
	import { invoke } from '@tauri-apps/api/core';
	import { tick } from 'svelte';

	let {
		conversations = $bindable([]),
		activeConversationIri = $bindable(null),
		onSwitch
	} = $props();

	let isRenaming = $state(false);
	let renameText = $state('');

	function startRename() {
		const current = conversations.find(c => c.iri === activeConversationIri);
		renameText = current?.label ?? '';
		isRenaming = true;
	}

	async function confirmRename() {
		const label = renameText.trim();
		if (!label || !activeConversationIri) { cancelRename(); return; }
		const iri = activeConversationIri;
		try {
			await invoke('chat__rename_conversation', { conversationId: iri, label });
			conversations = conversations.map(c => c.iri === iri ? { ...c, label } : c);
			activeConversationIri = null;
			await tick();
			activeConversationIri = iri;
		} catch (err) {
			console.error('Failed to rename conversation:', err);
			conversations = await invoke('chat__list_conversations');
		} finally {
			isRenaming = false;
		}
	}

	function cancelRename() {
		isRenaming = false;
		renameText = '';
	}
</script>

<div class="conversation-bar">
	{#if isRenaming}
		<input
			class="conversation-rename-input"
			bind:value={renameText}
			onkeydown={(e) => {
				if (e.key === 'Enter') confirmRename();
				else if (e.key === 'Escape') cancelRename();
			}}
			autofocus
		/>
		<button class="conversation-bar-btn confirm" onclick={confirmRename} title="Confirm">
			<span class="material-symbols-outlined">check</span>
		</button>
		<button class="conversation-bar-btn cancel" onclick={cancelRename} title="Cancel">
			<span class="material-symbols-outlined">close</span>
		</button>
	{:else}
		<select
			class="conversation-select"
			bind:value={activeConversationIri}
			onchange={() => onSwitch(activeConversationIri)}
		>
			{#each conversations as conv}
				<option value={conv.iri}>{conv.label}</option>
			{/each}
		</select>
		<button class="conversation-bar-btn" onclick={startRename} title="Rename conversation">
			<span class="material-symbols-outlined">edit</span>
		</button>
	{/if}
</div>

<style>
	.conversation-bar {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 4px 10px 4px 14px;
		border-bottom: 1px solid color-mix(in srgb, var(--color-white) 7%, transparent);
		flex-shrink: 0;
	}

	.conversation-select {
		flex: 1;
		background: transparent;
		border: none;
		color: var(--color-neutral);
		font-size: 12px;
		cursor: pointer;
		padding: 2px 0;
		min-width: 0;
	}

	.conversation-select:focus {
		outline: none;
	}

	.conversation-rename-input {
		flex: 1;
		background: transparent;
		border: none;
		border-bottom: 1px solid var(--color-interactive);
		color: var(--color-neutral-active);
		font-size: 12px;
		padding: 2px 0;
		min-width: 0;
	}

	.conversation-rename-input:focus {
		outline: none;
	}

	.conversation-bar-btn {
		width: 22px;
		height: 22px;
		border-radius: 4px;
		background: transparent;
		border: none;
		color: var(--color-neutral-disabled);
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		transition: all 0.15s;
		padding: 0;
	}

	.conversation-bar-btn:hover {
		color: var(--color-neutral-active);
		background: color-mix(in srgb, var(--color-white) 8%, transparent);
	}

	.conversation-bar-btn.confirm:hover {
		color: var(--color-success);
	}

	.conversation-bar-btn.cancel:hover {
		color: var(--color-danger);
	}

	.conversation-bar-btn .material-symbols-outlined {
		font-size: 14px;
	}
</style>
