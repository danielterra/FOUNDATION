<script>
	import ThinkingDots from './ThinkingDots.svelte';

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
			<ThinkingDots />
			<span class="ai-status-text">{aiStatus.status} ({elapsedSeconds}s)</span>
		</div>
		{#if onCancelAI}
			<button class="cancel-ai-btn" onclick={onCancelAI} title="Stop (ESC)">
				<span class="material-symbols-outlined">stop_circle</span>
			</button>
		{/if}
	</div>
{/if}

<!-- Edit mode banner -->
{#if editingMessageIri}
	<div class="edit-banner">
		<span class="material-symbols-outlined">edit</span>
		<span class="edit-banner-text">Editing message</span>
		<button class="edit-cancel-btn" onclick={onCancelEdit} aria-label="Cancel edit">
			<span class="material-symbols-outlined">close</span>
		</button>
	</div>
{/if}

<!-- Input -->
<div class="chat-input">
	<button
		class="attach-btn"
		onclick={openFilePicker}
		aria-label="Attach file"
	>
		<span class="material-symbols-outlined">attach_file</span>
	</button>
	<textarea
		bind:this={textareaElement}
		bind:value={inputText}
		onkeydown={onKeydown}
		onpaste={onPaste}
		placeholder="Ask me anything..."
		rows="1"
	></textarea>
	<button
		onclick={onSend}
		disabled={!inputText.trim() && !hasPendingAttachments}
		aria-label="Send"
		class:loading={isLoading}
	>
		<span class="material-symbols-outlined">
			{isLoading ? 'hourglass_empty' : 'send'}
		</span>
	</button>
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
		animation: fadeIn 0.3s;
	}

	.ai-status-content {
		display: flex;
		align-items: center;
		gap: 12px;
	}

	.cancel-ai-btn {
		background: none;
		border: none;
		cursor: pointer;
		color: var(--color-transition);
		padding: 0;
		display: flex;
		align-items: center;
		opacity: 0.6;
		flex-shrink: 0;
	}

	.cancel-ai-btn .material-symbols-outlined {
		font-size: 20px;
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

	.edit-cancel-btn {
		background: none;
		border: none;
		cursor: pointer;
		color: var(--color-interactive);
		padding: 0;
		display: flex;
		align-items: center;
		opacity: 0.7;
		flex-shrink: 0;
	}

	.edit-cancel-btn .material-symbols-outlined {
		font-size: 16px;
	}

	/* Input */
	.chat-input {
		display: flex;
		gap: 8px;
		flex-shrink: 0;
		align-items: flex-end;
	}

	.attach-btn {
		width: 40px;
		height: 40px;
		background: transparent;
		border: none;
		color: var(--color-neutral);
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}

	.attach-btn:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}

	.attach-btn .material-symbols-outlined {
		font-size: 20px;
	}

	.chat-input textarea {
		flex: 1;
		border: none;
		padding: 10px 16px;
		font-family: inherit;
		font-size: 14px;
		line-height: 1.5;
		resize: none;
		min-height: 40px;
		max-height: 300px;
		transition: height 0.1s;
		background: color-mix(in srgb, var(--color-white) 5%, transparent);
		color: var(--color-neutral-active);
		overflow-y: auto;
	}

	.chat-input textarea::placeholder {
		color: var(--color-neutral-disabled);
	}

	.chat-input textarea:focus {
		outline: none;
	}

	.chat-input textarea:disabled {
		background: color-mix(in srgb, var(--color-white) 3%, transparent);
		cursor: not-allowed;
		opacity: 0.5;
	}

	.chat-input button {
		width: 40px;
		height: 40px;
		background: var(--color-interactive);
		color: var(--color-neutral-on-interactive);
		border: none;
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}

	.chat-input button:disabled {
		background: var(--color-neutral-disabled);
		cursor: not-allowed;
		opacity: 0.5;
	}

	.chat-input button.loading {
		background: var(--color-transition);
	}

	.chat-input button .material-symbols-outlined {
		font-size: 20px;
	}

	.chat-input button.loading .material-symbols-outlined {
		animation: pulse 1.5s ease-in-out infinite;
	}

	@keyframes pulse {
		0%, 100% { opacity: 1; }
		50% { opacity: 0.5; }
	}
</style>
