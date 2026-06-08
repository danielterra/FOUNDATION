<script>
	import AgentPicker from './AgentPicker.svelte';
	import { Button } from '$lib/components/ui/button';
	import { convertFileSrc } from '@tauri-apps/api/core';

	let {
		conversationAgent = null,
		agents = [],
		onOpenAgentInspector,
		onSwitchAgent,
		onNewConversation,
		onDownloadChat,
		cameraEnabled = true,
		onToggleCamera = null,
		thinkingEnabled = true,
		onToggleThinking = null,
		activeModelInfo = null,
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
				{#if activeModelInfo?.modelLabel}
					<span class="agent-model">{activeModelInfo.modelLabel}</span>
				{/if}
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
			<span class:toggle-off={!cameraEnabled}>
				<Button variant="ghost" size="icon"
					title={cameraEnabled ? 'Camera vision on' : 'Camera vision off'}
					onclick={onToggleCamera}
				><span class="material-symbols-outlined">{cameraEnabled ? 'videocam' : 'videocam_off'}</span></Button>
			</span>
		{/if}
		{#if onToggleThinking}
			<span class:toggle-off={!thinkingEnabled}>
				<Button variant="ghost" size="icon"
					title={thinkingEnabled ? 'Extended thinking on' : 'Extended thinking off'}
					onclick={onToggleThinking}
				><span class="material-symbols-outlined">{thinkingEnabled ? 'psychology' : 'psychology_alt'}</span></Button>
			</span>
		{/if}
		<Button variant="ghost" size="icon" title="New conversation" onclick={onNewConversation}><span class="material-symbols-outlined">add</span></Button>
		<Button variant="ghost" size="icon" title="Download chat" onclick={onDownloadChat}><span class="material-symbols-outlined">download</span></Button>
	</div>
</div>

<style>
	.chat-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 10px 14px;
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
		background: color-mix(in srgb, var(--primary) 18%, transparent);
		border: none;
		border-radius: var(--radius);
		color: var(--primary);
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		transition: all 0.2s;
	}

	.agent-avatar.picker-open {
		background: color-mix(in srgb, var(--primary) 28%, transparent);
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
		display: flex;
		flex-direction: column;
		align-items: flex-start;
	}

	.agent-name {
		display: block;
		font-size: 14px;
		font-weight: 600;
		color: var(--accent-foreground);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.agent-model {
		font-size: 10px;
		color: var(--foreground);
		white-space: nowrap;
	}

	.agent-name-btn:hover .agent-name {
		color: var(--primary);
	}

	.chat-header-right {
		display: flex;
		align-items: center;
		gap: 4px;
		flex-shrink: 0;
	}

	.toggle-off { opacity: 0.5; }
</style>
