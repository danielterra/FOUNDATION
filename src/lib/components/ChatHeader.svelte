<script>
	import AgentPicker from './AgentPicker.svelte';

	let {
		conversationAgent = null,
		agents = [],
		onOpenAgentInspector,
		onSwitchAgent,
		onNewConversation,
		onDownloadChat
	} = $props();

	let pickerOpen = $state(false);

	function togglePicker() {
		pickerOpen = !pickerOpen;
	}

	function handleSelect(agentIri) {
		onSwitchAgent?.(agentIri);
		pickerOpen = false;
	}
</script>

<div class="chat-header">
	<div class="chat-header-left">
		<div class="agent-area">
			<button
				class="agent-avatar"
				class:picker-open={pickerOpen}
				onclick={togglePicker}
				title="Switch assistant"
			>
				<span class="material-symbols-outlined">{conversationAgent?.icon || 'smart_toy'}</span>
			</button>
			<button class="agent-name-btn" onclick={onOpenAgentInspector} title="Open in inspector">
				<span class="agent-name">{conversationAgent?.label || 'AI Assistant'}</span>
			</button>

			{#if pickerOpen}
				<AgentPicker
					{agents}
					activeAgentIri={conversationAgent?.iri}
					onSelect={handleSelect}
					onClose={() => pickerOpen = false}
				/>
			{/if}
		</div>
	</div>
	<div class="chat-header-right">
		<button class="header-action-btn" onclick={onNewConversation} title="New conversation">
			<span class="material-symbols-outlined">add</span>
		</button>
		<button class="header-action-btn" onclick={onDownloadChat} title="Download chat">
			<span class="material-symbols-outlined">download</span>
		</button>
	</div>
</div>

<style>
	.chat-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 10px 14px;
		border-bottom: 1px solid color-mix(in srgb, var(--color-white) 10%, transparent);
		flex-shrink: 0;
		gap: 8px;
	}

	.chat-header-left {
		display: flex;
		align-items: center;
		gap: 10px;
		min-width: 0;
		flex: 1;
	}

	.agent-area {
		position: relative;
		flex-shrink: 0;
	}

	.agent-avatar {
		width: 36px;
		height: 36px;
		border-radius: 50%;
		background: color-mix(in srgb, var(--color-interactive) 18%, transparent);
		border: 1px solid color-mix(in srgb, var(--color-interactive) 35%, transparent);
		color: var(--color-interactive);
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		transition: all 0.2s;
	}

	.agent-avatar.picker-open {
		background: color-mix(in srgb, var(--color-interactive) 28%, transparent);
		border-color: var(--color-interactive);
	}

	.agent-avatar .material-symbols-outlined {
		font-size: 18px;
	}

	.agent-name-btn {
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
		min-width: 0;
		flex: 1;
	}

	.agent-name {
		display: block;
		font-size: 14px;
		font-weight: 600;
		color: var(--color-neutral-active);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.agent-name-btn:hover .agent-name {
		color: var(--color-interactive);
	}

	.chat-header-right {
		display: flex;
		align-items: center;
		gap: 4px;
		flex-shrink: 0;
	}

	.header-action-btn {
		width: 32px;
		height: 32px;
		border-radius: 6px;
		background: transparent;
		border: none;
		color: var(--color-neutral);
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		transition: all 0.15s;
	}

	.header-action-btn .material-symbols-outlined {
		font-size: 18px;
	}
</style>
