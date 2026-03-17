<script>
	let {
		conversationAgent = null,
		onOpenAgentInspector,
		onNewConversation,
		onDownloadChat
	} = $props();
</script>

<div class="chat-header">
	<div class="chat-header-left">
		<button class="agent-avatar" onclick={onOpenAgentInspector} title={conversationAgent ? `Open ${conversationAgent.label} in Inspector` : 'AI Assistant'}>
			<span class="material-symbols-outlined">{conversationAgent?.icon || 'smart_toy'}</span>
		</button>
		<span class="agent-name">{conversationAgent?.label || 'AI Assistant'}</span>
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
		flex-shrink: 0;
		transition: all 0.2s;
	}

	.agent-avatar:hover {
		background: color-mix(in srgb, var(--color-interactive) 28%, transparent);
		border-color: var(--color-interactive);
	}

	.agent-avatar .material-symbols-outlined {
		font-size: 18px;
	}

	.agent-name {
		font-size: 14px;
		font-weight: 600;
		color: var(--color-neutral-active);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
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

	.header-action-btn:hover {
		background: color-mix(in srgb, var(--color-white) 10%, transparent);
		color: var(--color-neutral-active);
	}

	.header-action-btn .material-symbols-outlined {
		font-size: 18px;
	}
</style>
