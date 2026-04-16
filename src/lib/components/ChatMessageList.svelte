<script>
	import ChatMessageBubble from './ChatMessageBubble.svelte';

	let {
		messages,
		conversationId = '',
		isLoadingMessages,
		isLoadingMore,
		chatContainer = $bindable(null),
		contentEl = $bindable(null),
		onScroll,
		onEdit,
		onRetry,
		onEntityClick = null
	} = $props();

</script>

<div class="chat-messages" bind:this={chatContainer} onscroll={onScroll}>
	<div bind:this={contentEl} class="chat-messages-inner">
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
			{#each messages as unit (unit.iri)}
				<div class="message-enter">
					<ChatMessageBubble
						{unit}
						isLast={unit.iri === messages[messages.length - 1].iri}
						{conversationId}
						isStreaming={unit.iri === '__streaming__'}
						{onEdit}
						{onRetry}
						{onEntityClick}
					/>
				</div>
			{/each}
		{/if}
	</div>
</div>

<style>
	@keyframes message-enter {
		from { opacity: 0; transform: translateY(24px); }
		to   { opacity: 1; transform: translateY(0); }
	}

	.message-enter {
		animation: message-enter 380ms cubic-bezier(0.215, 0.61, 0.355, 1) both;
	}

	.chat-messages {
		flex: 1;
		overflow-y: auto;
		overflow-x: hidden;
		min-height: 0;
	}

	.chat-messages-inner {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding-bottom: 12px;
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
		font-size: 14px;
		background: color-mix(in srgb, var(--color-white) 5%, transparent);
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
	}

	.chat-messages::-webkit-scrollbar-thumb:hover {
		background: color-mix(in srgb, var(--color-white) 30%, transparent);
	}
</style>
