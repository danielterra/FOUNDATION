<script>
	import ChatMessageBubble from './ChatMessageBubble.svelte';

	let {
		messages,
		conversationId = '',
		isLoadingMessages,
		isLoadingMore,
		chatContainer = $bindable(null),
		onScroll,
		onEdit,
		onRetry,
		onEntityClick = null
	} = $props();

	// In column-reverse layout the DOM order is inverted: newest message is first
	// in the DOM (bottom of the visual list). We reverse here so the most-recent
	// message stays anchored at the bottom without any imperative scrollTop math.
	let reversedMessages = $derived([...messages].reverse());

	// The logically-last message (newest) is messages[messages.length - 1].
	// After reversing, it becomes reversedMessages[0] — but isLast still refers
	// to the original last-message IRI so ChatMessageBubble keeps its semantics.
	let lastMessageIri = $derived(messages.length > 0 ? messages[messages.length - 1].iri : null);

	// Track the set of IRIs that were present on the previous render cycle so
	// we can animate only genuinely new arrivals (not the bulk load on mount).
	let knownIris = new Set();
	function isNewMessage(iri) {
		if (knownIris.has(iri)) return false;
		knownIris.add(iri);
		return true;
	}
</script>

<div class="chat-messages" bind:this={chatContainer} onscroll={onScroll}>
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
		{#if isLoadingMore}
			<div class="loading-more">
				<span class="material-symbols-outlined spinning">refresh</span>
				<span>Loading more messages...</span>
			</div>
		{/if}
		{#each reversedMessages as unit (unit.iri)}
			<div class={isNewMessage(unit.iri) ? 'message-enter' : ''}>
				<ChatMessageBubble
					{unit}
					isLast={unit.iri === lastMessageIri}
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

<style>
	@keyframes message-enter {
		from { opacity: 0; transform: translateY(8px); }
		to   { opacity: 1; transform: translateY(0); }
	}

	.message-enter {
		animation: message-enter 380ms cubic-bezier(0.215, 0.61, 0.355, 1) both;
	}

	.chat-messages {
		flex: 1;
		display: flex;
		flex-direction: column-reverse;
		overflow-y: auto;
		overflow-x: hidden;
		min-height: 0;
		gap: 12px;
		padding-bottom: 12px;
	}

	.empty-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		flex: 1;
		color: var(--muted-foreground);
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
		color: var(--muted-foreground);
		font-size: 14px;
		background: var(--muted);
		margin-top: 12px;
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
		background: var(--border);
	}

	.chat-messages::-webkit-scrollbar-thumb:hover {
		background: var(--muted-foreground);
	}
</style>
