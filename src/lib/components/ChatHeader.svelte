<script>
	import AgentPicker from './AgentPicker.svelte';
	import { Button } from '$lib/components/ui/button';
	import { convertFileSrc } from '@tauri-apps/api/core';

	let {
		conversationAgent = null,
		agents = [],
		onOpenAgentInspector,
		onSwitchAgent,
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
			<Button
				variant="ghost"
				class={`agent-avatar${pickerOpen ? ' picker-open' : ''}${isImageIcon(conversationAgent?.icon) ? ' has-image' : ''}`}
				onclick={togglePicker}
				title="Switch assistant"
			>
				{#if isImageIcon(conversationAgent?.icon)}
					<img src={resolveIcon(conversationAgent.icon)} alt={conversationAgent.label} />
				{:else}
					<span class="material-symbols-outlined">{conversationAgent?.icon || 'smart_toy'}</span>
				{/if}
			</Button>
			<Button variant="ghost" class="agent-name-btn" onclick={onOpenAgentInspector} title="Open in inspector">
				<span class="agent-name">{conversationAgent?.label || 'AI Assistant'}</span>
				{#if activeModelInfo?.modelLabel}
					<span class="agent-model">{activeModelInfo.modelLabel}</span>
				{/if}
			</Button>

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
		<Button variant="ghost" size="icon" title="Download chat" onclick={onDownloadChat}><span class="material-symbols-outlined">download</span></Button>
	</div>
</div>

<style>
	.chat-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: var(--spacing-chat-inset-block) var(--spacing-widget-header-inline);
		flex-shrink: 0;
		gap: var(--spacing-inline-related);
		border-bottom: 1px solid var(--color-hud-line-subtle);
	}

	.chat-header-left {
		display: flex;
		align-items: center;
		gap: var(--spacing-inline-related);
		min-width: 0;
		flex: 1;
	}

	.agent-area {
		position: relative;
		flex-shrink: 0;
		display: flex;
		align-items: center;
		gap: var(--spacing-control-internal);
	}

	:global([data-slot="button"].agent-avatar) {
		width: var(--size-touch-target);
		height: var(--size-touch-target);
		background: color-mix(in srgb, var(--color-interactive) 12%, transparent);
		border-radius: var(--radius);
		color: var(--color-interactive);
		padding: 0;
		flex-shrink: 0;
	}

	:global([data-slot="button"].agent-avatar:hover) {
		background: color-mix(in srgb, var(--color-interactive) 20%, transparent);
		color: var(--color-interactive);
	}

	:global([data-slot="button"].agent-avatar.picker-open) {
		background: color-mix(in srgb, var(--color-interactive) 20%, transparent);
	}

	:global([data-slot="button"].agent-avatar .material-symbols-outlined) {
		font-size: var(--type-icon-medium);
	}

	:global([data-slot="button"].agent-avatar.has-image) {
		background: transparent;
		padding: 0;
		overflow: hidden;
	}

	:global([data-slot="button"].agent-avatar img) {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	:global([data-slot="button"].agent-name-btn) {
		padding: 0;
		min-width: 0;
		flex: 1;
		height: auto;
		display: flex;
		flex-direction: column;
		align-items: flex-start;
	}

	:global([data-slot="button"].agent-name-btn:hover) {
		background: transparent;
	}

	.agent-name {
		display: block;
		font-size: var(--type-body);
		color: var(--color-neutral);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.agent-model {
		font-size: var(--type-micro);
		color: var(--color-neutral-secondary);
		white-space: nowrap;
	}

	:global([data-slot="button"].agent-name-btn:hover) .agent-name {
		color: var(--color-interactive);
	}

	.chat-header-right {
		display: flex;
		align-items: center;
		gap: var(--spacing-inline-tight);
		flex-shrink: 0;
	}

	.toggle-off { opacity: 0.5; }
</style>
