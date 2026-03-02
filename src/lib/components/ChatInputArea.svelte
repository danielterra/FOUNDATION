<script>
	let {
		inputText = $bindable(''),
		isLoading,
		hasPendingAttachments,
		aiStatus,
		elapsedSeconds,
		onSend,
		onKeydown,
		onFileSelect,
		textareaElement = $bindable(null),
		fileInputElement = $bindable(null),
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
	accept="image/png,image/jpeg,image/webp,image/gif,application/pdf"
	multiple
	style="display: none;"
/>

<!-- AI Status Indicator -->
{#if aiStatus}
	<div class="ai-status-indicator">
		<div class="ai-status-content">
			<div class="thinking-dots">
				<span></span>
				<span></span>
				<span></span>
			</div>
			<span class="ai-status-text">{aiStatus.status} ({elapsedSeconds}s)</span>
		</div>
	</div>
{/if}

<!-- Input -->
<div class="chat-input">
	<button
		class="attach-btn"
		onclick={openFilePicker}
		disabled={isLoading}
		aria-label="Attach file"
	>
		<span class="material-symbols-outlined">attach_file</span>
	</button>
	<textarea
		bind:this={textareaElement}
		bind:value={inputText}
		onkeydown={onKeydown}
		placeholder="Ask me anything..."
		rows="1"
		disabled={isLoading}
	></textarea>
	<button
		onclick={onSend}
		disabled={(!inputText.trim() && !hasPendingAttachments) || isLoading}
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
		justify-content: center;
		padding: 12px 16px;
		margin-bottom: 12px;
		background: color-mix(in srgb, var(--color-interactive) 10%, transparent);
		border: 1px solid color-mix(in srgb, var(--color-interactive) 30%, transparent);
		border-radius: 12px;
		animation: fadeIn 0.3s;
	}

	.ai-status-content {
		display: flex;
		align-items: center;
		gap: 12px;
	}

	.ai-status-text {
		font-size: 14px;
		color: var(--color-interactive);
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

	.thinking-dots {
		display: flex;
		gap: 6px;
		align-items: center;
	}

	.thinking-dots span {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--color-interactive);
		animation: thinking-bounce 1.4s infinite ease-in-out;
	}

	.thinking-dots span:nth-child(1) {
		animation-delay: -0.32s;
	}

	.thinking-dots span:nth-child(2) {
		animation-delay: -0.16s;
	}

	@keyframes thinking-bounce {
		0%, 80%, 100% {
			transform: scale(0.8);
			opacity: 0.5;
		}
		40% {
			transform: scale(1.2);
			opacity: 1;
		}
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
		border-radius: 50%;
		background: transparent;
		border: 1px solid color-mix(in srgb, var(--color-white) 20%, transparent);
		color: var(--color-neutral);
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		transition: all 0.2s;
		flex-shrink: 0;
	}

	.attach-btn:hover:not(:disabled) {
		background: color-mix(in srgb, var(--color-white) 10%, transparent);
		border-color: var(--color-interactive);
		color: var(--color-interactive);
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
		border: 1px solid color-mix(in srgb, var(--color-white) 20%, transparent);
		border-radius: 20px;
		padding: 10px 16px;
		font-family: inherit;
		font-size: 14px;
		line-height: 1.5;
		resize: none;
		min-height: 40px;
		max-height: 300px;
		transition: border-color 0.2s, height 0.1s;
		background: color-mix(in srgb, var(--color-white) 5%, transparent);
		color: var(--color-neutral-active);
		overflow-y: auto;
	}

	.chat-input textarea::placeholder {
		color: var(--color-neutral-disabled);
	}

	.chat-input textarea:focus {
		outline: none;
		border-color: var(--color-interactive);
	}

	.chat-input textarea:disabled {
		background: color-mix(in srgb, var(--color-white) 3%, transparent);
		cursor: not-allowed;
		opacity: 0.5;
	}

	.chat-input button {
		width: 40px;
		height: 40px;
		border-radius: 50%;
		background: var(--color-interactive);
		color: var(--color-neutral-on-interactive);
		border: none;
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		transition: all 0.2s;
		flex-shrink: 0;
	}

	.chat-input button:hover:not(:disabled) {
		background: var(--color-interactive-hover);
		transform: scale(1.05);
	}

	.chat-input button:active:not(:disabled) {
		background: var(--color-interactive-active);
	}

	.chat-input button:disabled {
		background: var(--color-neutral-disabled);
		cursor: not-allowed;
		opacity: 0.5;
	}

	.chat-input button.loading {
		background: var(--color-transition);
	}

	.chat-input button.loading:hover {
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
