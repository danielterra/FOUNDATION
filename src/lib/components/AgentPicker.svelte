<script>
	import { convertFileSrc } from '@tauri-apps/api/core';

	let {
		agents = [],
		activeAgentIri = null,
		onSelect,
		onClose,
	} = $props();

	function resolveIcon(icon) {
		if (!icon) return null;
		if (icon.startsWith('file://')) return convertFileSrc(icon.replace(/^file:\/\//, ''));
		return icon;
	}

	function handleSelect(agent) {
		if (!agent.available) return;
		onSelect(agent.iri);
		onClose();
	}

	function handleBackdropClick() {
		onClose();
	}

	function handleKeydown(e) {
		if (e.key === 'Escape') onClose();
	}
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="backdrop" onclick={handleBackdropClick} role="presentation"></div>

<div class="picker" role="listbox" aria-label="Select AI Assistant">
	<div class="picker-title">Switch Assistant</div>
	{#each agents as agent (agent.iri)}
		<button
			class="agent-row"
			class:active={agent.iri === activeAgentIri}
			class:unavailable={!agent.available}
			disabled={!agent.available}
			onclick={() => handleSelect(agent)}
			role="option"
			aria-selected={agent.iri === activeAgentIri}
		>
			<span class="agent-icon">
				{#if agent.icon?.startsWith('http') || agent.icon?.startsWith('file') || agent.icon?.startsWith('data')}
					<img src={resolveIcon(agent.icon)} alt={agent.label} onerror={(e) => { e.currentTarget.style.display='none'; e.currentTarget.nextElementSibling.style.display='inline'; }} />
					<span class="material-symbols-outlined" style="display:none">smart_toy</span>
				{:else}
					<span class="material-symbols-outlined">{agent.icon || 'smart_toy'}</span>
				{/if}
			</span>
			<span class="agent-label">{agent.label}</span>
			{#if agent.iri === activeAgentIri}
				<span class="active-badge material-symbols-outlined">check</span>
			{/if}
			{#if !agent.available}
				<span class="unavailable-badge">unavailable</span>
			{/if}
		</button>
	{/each}
</div>

<style>
	.backdrop {
		position: fixed;
		inset: 0;
		z-index: 100;
	}

	.picker {
		position: absolute;
		top: calc(100% + 6px);
		left: 0;
		z-index: 101;
		min-width: 220px;
		background: var(--popover);
		padding: 6px;
		border-radius: var(--radius);
		box-shadow: 0 8px 24px color-mix(in srgb, var(--color-black) 60%, transparent);
	}

	.picker-title {
		font-size: 11px;
		font-weight: 600;
		color: var(--muted-foreground);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		padding: 4px 8px 6px;
	}

	.agent-row {
		display: flex;
		align-items: center;
		gap: 10px;
		width: 100%;
		padding: 8px 10px;
		border: none;
		background: transparent;
		color: var(--accent-foreground);
		cursor: pointer;
		text-align: left;
		transition: background 0.12s;
		border-radius: var(--radius);
	}

	.agent-row:hover:not(:disabled) {
		background: var(--accent);
	}

	.agent-row.active {
		background: color-mix(in srgb, var(--primary) 14%, transparent);
		color: var(--primary);
	}

	.agent-row.unavailable {
		opacity: 0.4;
		cursor: not-allowed;
	}

	.agent-icon {
		width: 28px;
		height: 28px;
		background: var(--muted);
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		overflow: hidden;
		border-radius: var(--radius);
	}

	.agent-icon img {
		width: 100%;
		height: 100%;
		object-fit: cover;
	}

	.agent-icon .material-symbols-outlined {
		font-size: 16px;
	}

	.agent-label {
		flex: 1;
		font-size: 14px;
		font-weight: 500;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.active-badge {
		font-size: 16px;
		color: var(--primary);
	}

	.unavailable-badge {
		font-size: 10px;
		color: var(--muted-foreground);
		background: var(--muted);
		padding: 2px 5px;
		border-radius: var(--radius);
	}
</style>
