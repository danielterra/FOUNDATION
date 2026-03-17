<script>
	import { fly } from 'svelte/transition';
	import { cubicOut } from 'svelte/easing';
	import ChatMessageBubble from './ChatMessageBubble.svelte';

	let {
		messages,
		isLoadingMessages,
		isLoadingMore,
		chatContainer = $bindable(null),
		onScroll,
		shouldDisplayMessage,
		onEdit,
		onRetry,
		onEntityClick = null
	} = $props();
</script>

<div class="chat-messages" bind:this={chatContainer} onscroll={onScroll}>
	{#if isLoadingMore}
		<div class="loading-more">
			<span class="material-symbols-outlined spinning">refresh</span>
			<span>Loading more messages...</span>
		</div>
	{/if}
	{#if isLoadingMessages}
		<div class="empty-state">
			<span class="material-symbols-outlined spinning">progress_activity</span>
			<p>Loading messages...</p>
		</div>
	{:else if messages.length === 0}
		<div class="empty-state">
			<span class="material-symbols-outlined">chat_bubble</span>
			<p>Start a conversation with the AI assistant</p>
		</div>
	{:else}
		{#each messages as message (message.iri)}
			{#if shouldDisplayMessage(message)}
				<div in:fly={{ y: 80, duration: 380, easing: cubicOut }}>
					<ChatMessageBubble
						{message}
						{messages}
						onEdit={onEdit}
						onRetry={onRetry}
						onEntityClick={onEntityClick}
					/>
				</div>
			{/if}
		{/each}
	{/if}
</div>

<style>
	.chat-messages {
		flex: 1;
		overflow-y: auto;
		overflow-x: hidden;
		display: flex;
		flex-direction: column;
		gap: 2px;
		margin-bottom: 12px;
		min-height: 0;
	}

	.empty-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		height: 100%;
		color: var(--color-neutral);
		gap: 12px;
	}

	.empty-state .material-symbols-outlined {
		font-size: 48px;
		opacity: 0.3;
	}

	.loading-more {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 8px;
		padding: 12px;
		color: var(--color-neutral);
		font-size: 13px;
		background: color-mix(in srgb, var(--color-white) 5%, transparent);
		border-radius: 8px;
		margin-bottom: 12px;
	}

	.loading-more .material-symbols-outlined {
		font-size: 18px;
	}

	.spinning {
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

	.chat-messages::-webkit-scrollbar {
		width: 6px;
	}

	.chat-messages::-webkit-scrollbar-track {
		background: transparent;
	}

	.chat-messages::-webkit-scrollbar-thumb {
		background: color-mix(in srgb, var(--color-white) 20%, transparent);
		border-radius: 3px;
	}

	.chat-messages::-webkit-scrollbar-thumb:hover {
		background: color-mix(in srgb, var(--color-white) 30%, transparent);
	}
</style>
