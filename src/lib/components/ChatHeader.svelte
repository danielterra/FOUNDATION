<script>
	import AgentPicker from './AgentPicker.svelte';
	import { convertFileSrc } from '@tauri-apps/api/core';

	let {
		conversationAgent = null,
		agents = [],
		onOpenAgentInspector,
		onSwitchAgent,
		onNewConversation,
		onDownloadChat,
		onOpenSettings,
		cameraEnabled = true,
		onToggleCamera = null,
		thinkingEnabled = true,
		onToggleThinking = null,
	} = $props();

	function resolveIcon(icon) {
		if (!icon) return null;
		if (icon.startsWith('file://')) return convertFileSrc(icon.replace(/^file:\/\//, ''));
		return icon;
	}

	function isImageIcon(icon) {
		return icon?.startsWith('http') || icon?.startsWith('file') || icon?.startsWith('data');
	}

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
				class:has-image={isImageIcon(conversationAgent?.icon)}
				onclick={togglePicker}
				title="Switch assistant"
			>
				{#if isImageIcon(conversationAgent?.icon)}
					<img src={resolveIcon(conversationAgent.icon)} alt={conversationAgent.label} />
				{:else}
					<span class="material-symbols-outlined">{conversationAgent?.icon || 'smart_toy'}</span>
				{/if}
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
		{#if onToggleCamera}
			<button
				class="header-action-btn"
				class:toggle-off={!cameraEnabled}
				onclick={onToggleCamera}
				title={cameraEnabled ? 'Camera vision on' : 'Camera vision off'}
			>
				<span class="material-symbols-outlined">
					{cameraEnabled ? 'videocam' : 'videocam_off'}
				</span>
			</button>
		{/if}
		{#if onToggleThinking}
			<button
				class="header-action-btn"
				class:toggle-off={!thinkingEnabled}
				onclick={onToggleThinking}
				title={thinkingEnabled ? 'Extended thinking on' : 'Extended thinking off'}
			>
				<span class="material-symbols-outlined">
					{thinkingEnabled ? 'psychology' : 'psychology_alt'}
				</span>
			</button>
		{/if}
		<button class="header-action-btn" onclick={onNewConversation} title="New conversation">
			<span class="material-symbols-outlined">add</span>
		</button>
		<button class="header-action-btn" onclick={onDownloadChat} title="Download chat">
			<span class="material-symbols-outlined">download</span>
		</button>
		<button class="header-action-btn" onclick={onOpenSettings} title="Settings">
			<span class="material-symbols-outlined">settings</span>
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
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.agent-avatar {
		width: 36px;
		height: 36px;
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

	.agent-avatar.has-image {
		background: transparent;
		border-color: transparent;
		padding: 0;
		overflow: hidden;
	}

	.agent-avatar img {
		width: 100%;
		height: 100%;
		object-fit: cover;
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
		background: transparent;
		border: none;
		color: var(--color-interactive);
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		transition: all 0.15s;
	}

	.header-action-btn .material-symbols-outlined {
		font-size: 18px;
	}

	.header-action-btn.toggle-off {
		opacity: 0.5;
	}
</style>
