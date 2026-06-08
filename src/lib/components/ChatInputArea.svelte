<script>
	import ThinkingDots from './ThinkingDots.svelte';
	import { Button } from '$lib/components/ui/button';

	let {
		inputText = $bindable(''),
		isLoading,
		hasPendingAttachments,
		aiStatus,
		elapsedSeconds,
		onSend,
		onKeydown,
		onFileSelect,
		onPaste,
		textareaElement = $bindable(null),
		fileInputElement = $bindable(null),
		editingMessageIri = null,
		onCancelEdit = null,
		onCancelAI = null,
	} = $props();

	function openFilePicker() {
		fileInputElement?.click();
	}
</script>

<!-- Hidden file input -->
<input
	type="file"
	bind:this={fileInputElement}
	onchange={onFileSelect}
	accept="image/png,image/jpeg,image/webp,image/gif,application/pdf,text/csv,text/plain"
	multiple
	style="display: none;"
/>

<!-- AI Status Indicator -->
{#if aiStatus}
	<div class="ai-status-indicator">
		<div class="ai-status-content">
			{#if aiStatus.phase === 'tool'}
				<span class="material-symbols-outlined phase-icon">build</span>
			{:else if aiStatus.phase === 'prep'}
				<span class="material-symbols-outlined phase-icon">schedule</span>
			{:else}
				<ThinkingDots />
			{/if}
			<span class="ai-status-text">{aiStatus.status} ({elapsedSeconds}s)</span>
		</div>
		{#if onCancelAI}
			<Button variant="ghost" size="icon" onclick={onCancelAI} title="Stop (ESC)" aria-label="Stop AI">
				<span class="material-symbols-outlined">stop_circle</span>
			</Button>
		{/if}
	</div>
{/if}

<!-- Edit mode banner -->
{#if editingMessageIri}
	<div class="edit-banner">
		<span class="material-symbols-outlined">edit</span>
		<span class="edit-banner-text">Editing message</span>
		<Button variant="ghost" size="icon" onclick={onCancelEdit} aria-label="Cancel edit">
			<span class="material-symbols-outlined">close</span>
		</Button>
	</div>
{/if}

<!-- Input -->
<div class="chat-input">
	<Button variant="ghost" size="icon" onclick={openFilePicker} aria-label="Attach file">
		<span class="material-symbols-outlined">attach_file</span>
	</Button>
	<textarea
		bind:this={textareaElement}
		bind:value={inputText}
		onkeydown={onKeydown}
		onpaste={onPaste}
		placeholder="Ask me anything..."
		rows="1"
	></textarea>
	<Button
		onclick={onSend}
		disabled={!inputText.trim() && !hasPendingAttachments}
		aria-label="Send"
		class={isLoading ? 'send-btn-loading' : ''}
	>
		<span class="material-symbols-outlined">
			{isLoading ? 'hourglass_empty' : 'send'}
		</span>
	</Button>
</div>

<style>
	/* AI Status Indicator */
	.ai-status-indicator {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 12px 16px;
		margin-bottom: 12px;
		background: color-mix(in srgb, var(--color-transition) 10%, transparent);
		border-radius: var(--radius);
		animation: fadeIn 0.3s;
	}

	.ai-status-content {
		display: flex;
		align-items: center;
		gap: 12px;
	}

	.phase-icon {
		font-size: 18px;
		color: var(--color-transition);
		animation: thinking-pulse 1.5s ease-in-out infinite;
	}

	@keyframes thinking-pulse {
		0%, 100% { opacity: 0.7; }
		50% { opacity: 1; }
	}

	.ai-status-text {
		font-size: 14px;
		color: var(--color-transition);
		font-weight: 500;
		animation: pulse-opacity 1.5s ease-in-out infinite;
	}

	@keyframes pulse-opacity {
		0%, 100% {
			opacity: 0.7;
		}
		50% {
			opacity: 1;
		}
	}

	@keyframes fadeIn {
		from { opacity: 0; }
		to { opacity: 1; }
	}

	/* Edit banner */
	.edit-banner {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 12px;
		margin-bottom: 8px;
		background: color-mix(in srgb, var(--color-interactive) 10%, transparent);
		border-radius: var(--radius);
		font-size: 14px;
		color: var(--color-interactive);
		animation: fadeIn 0.2s;
	}

	.edit-banner .material-symbols-outlined {
		font-size: 16px;
		flex-shrink: 0;
	}

	.edit-banner-text {
		flex: 1;
		font-weight: 500;
	}

	/* Input */
	.chat-input {
		display: flex;
		gap: 8px;
		flex-shrink: 0;
		align-items: flex-end;
	}

	.chat-input textarea {
		flex: 1;
		border: none;
		border-radius: var(--radius);
		padding: 10px 16px;
		font-family: inherit;
		font-size: 14px;
		line-height: 1.5;
		resize: none;
		min-height: 40px;
		max-height: 300px;
		transition: height 0.1s;
		background: var(--muted);
		color: var(--foreground);
		overflow-y: auto;
	}

	.chat-input textarea::placeholder {
		color: var(--muted-foreground);
	}

	.chat-input textarea:focus {
		outline: none;
	}

	.chat-input textarea:disabled {
		background: var(--input);
		cursor: not-allowed;
		opacity: 0.5;
	}

	:global(.send-btn-loading) {
		background: var(--color-transition) !important;
	}

	:global(.send-btn-loading .material-symbols-outlined) {
		animation: pulse 1.5s ease-in-out infinite;
	}

	@keyframes pulse {
		0%, 100% { opacity: 1; }
		50% { opacity: 0.5; }
	}
</style>
