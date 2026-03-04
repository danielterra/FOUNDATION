<script>
	import { convertFileSrc } from '@tauri-apps/api/core';
	import { openPath } from '@tauri-apps/plugin-opener';
	import { marked } from 'marked';

	marked.setOptions({
		breaks: true,
		gfm: true,
	});

	let { message, messages, onEdit = null, onRetry = null } = $props();

	let copySuccess = $state(false);
	let copyTimeout = null;

	async function copyMessage() {
		const text = extractTextFromContent(message.content);
		try {
			await navigator.clipboard.writeText(text);
			copySuccess = true;
			if (copyTimeout) clearTimeout(copyTimeout);
			copyTimeout = setTimeout(() => {
				copySuccess = false;
			}, 2000);
		} catch (err) {
			console.error('Failed to copy:', err);
		}
	}

	function handleEdit() {
		const text = extractTextFromContent(message.content);
		onEdit?.(message.iri, text);
	}

	function handleRetry() {
		onRetry?.(message.iri);
	}

	function renderMarkdown(text) {
		if (!text) return '';
		return marked.parse(text);
	}

	function extractTextFromContent(content) {
		if (!content) return '';
		if (typeof content === 'string') return content;
		if (Array.isArray(content)) {
			return content
				.filter(block => block.type === 'text')
				.map(block => block.text)
				.join('\n\n');
		}
		return '';
	}

	function extractToolUses(content) {
		if (!Array.isArray(content)) return [];
		return content.filter(block => block.type === 'tool_use');
	}

	function hasToolUses(msg) {
		return Array.isArray(msg.content) &&
		       msg.content.some(block => block.type === 'tool_use');
	}

	function hasTextContent(msg) {
		if (!Array.isArray(msg.content)) return false;
		return msg.content.some(block => block.type === 'text');
	}

	function groupToolUsesWithResults(msg, allMessages) {
		const toolUses = extractToolUses(msg.content);

		const msgIndex = allMessages.findIndex(m => m.iri === msg.iri);
		const nextMessage = msgIndex >= 0 && msgIndex < allMessages.length - 1
			? allMessages[msgIndex + 1]
			: null;

		const toolResults = nextMessage && Array.isArray(nextMessage.content)
			? nextMessage.content.filter(block => block.type === 'tool_result')
			: [];

		const resultsMap = new Map();
		for (const result of toolResults) {
			resultsMap.set(result.tool_use_id, result);
		}

		return toolUses.map(toolUse => ({
			toolUse,
			toolResult: resultsMap.get(toolUse.id) || null
		}));
	}

	function formatFileSize(bytes) {
		if (bytes === 0) return '0 Bytes';
		const k = 1024;
		const sizes = ['Bytes', 'KB', 'MB', 'GB'];
		const i = Math.floor(Math.log(bytes) / Math.log(k));
		return Math.round(bytes / Math.pow(k, i) * 100) / 100 + ' ' + sizes[i];
	}

	async function openFile(filePath) {
		if (!filePath) return;
		try {
			await openPath(filePath);
		} catch (err) {
			console.error('Failed to open file:', err);
		}
	}
</script>

<div
	class="message {message.role === 'user' ? 'user' : 'ai'} {message.isThinking ? 'thinking' : ''}"
>
	{#if !message.isThinking}
		<div class="message-action-bar">
			<button class="action-btn" onclick={copyMessage} title="Copy message">
				<span class="material-symbols-outlined">{copySuccess ? 'check' : 'content_copy'}</span>
			</button>
			{#if message.role === 'user' && onEdit}
				<button class="action-btn" onclick={handleEdit} title="Edit message">
					<span class="material-symbols-outlined">edit</span>
				</button>
			{/if}
			{#if message.role === 'assistant' && onRetry}
				<button class="action-btn" onclick={handleRetry} title="Retry">
					<span class="material-symbols-outlined">refresh</span>
				</button>
			{/if}
		</div>
	{/if}
	<div class="message-content">
		{#if message.isThinking}
			<div class="thinking-indicator">
				<div class="thinking-dots">
					<span></span>
					<span></span>
					<span></span>
				</div>
				<span class="thinking-text">AI is thinking...</span>
			</div>
		{:else if hasTextContent(message)}
			<div class="message-text markdown-content">
				{@html renderMarkdown(extractTextFromContent(message.content))}
			</div>
		{/if}

		{#if message.attachments && message.attachments.length > 0}
			<div class="message-attachments">
				{#each message.attachments as attachment}
					{#if attachment.mimeType.startsWith('image/')}
						<button
							class="attachment-thumbnail attachment-image"
							onclick={() => openFile(attachment.filePath)}
							title="Click to open in default app"
						>
							{#if attachment.filePath}
								<img
									src={convertFileSrc(attachment.filePath)}
									alt={attachment.fileName}
								/>
							{/if}
							<div class="attachment-info">
								<span class="material-symbols-outlined">image</span>
								<span class="attachment-name">{attachment.fileName}</span>
								<span class="attachment-size">{formatFileSize(attachment.fileSize)}</span>
							</div>
						</button>
					{:else if attachment.mimeType === 'application/pdf'}
						<button
							class="attachment-thumbnail attachment-pdf"
							onclick={() => openFile(attachment.filePath)}
							title="Click to open in default app"
						>
							<div class="pdf-preview">
								<span class="material-symbols-outlined">picture_as_pdf</span>
								<span class="pdf-label">PDF</span>
							</div>
							<div class="attachment-info">
								<span class="attachment-name">{attachment.fileName}</span>
								<span class="attachment-size">{formatFileSize(attachment.fileSize)}</span>
							</div>
						</button>
					{:else}
						<button
							class="attachment-thumbnail attachment-file"
							onclick={() => openFile(attachment.filePath)}
							title="Click to open in default app"
						>
							<div class="file-preview">
								<span class="material-symbols-outlined">attach_file</span>
							</div>
							<div class="attachment-info">
								<span class="attachment-name">{attachment.fileName}</span>
								<span class="attachment-size">{formatFileSize(attachment.fileSize)}</span>
							</div>
						</button>
					{/if}
				{/each}
			</div>
		{/if}

		{#if hasToolUses(message)}
			{@const toolGroups = groupToolUsesWithResults(message, messages)}
			{#if toolGroups.length > 0}
			<div class="tool-execution-groups">
				<div class="tool-header">
					<span class="material-symbols-outlined">construction</span>
					<span>Tool Executions ({toolGroups.length})</span>
				</div>
				{#each toolGroups as group}
					<details class="tool-group">
						<summary
						class="tool-group-summary {group.toolResult
							? (group.toolResult.is_error ? 'error' : 'success')
							: 'pending'}"
					>
							<span class="material-symbols-outlined">
								{group.toolResult
									? (group.toolResult.is_error ? 'error' : 'check_circle')
									: 'pending'}
							</span>
							<span class="tool-group-title">
								{group.toolUse ? group.toolUse.name : 'Unknown Tool'}
							</span>
							{#if group.toolResult}
								<span class="tool-status-badge {group.toolResult.is_error ? 'error' : 'success'}">
									{group.toolResult.is_error ? '✗ Failed' : '✓ Success'}
								</span>
							{/if}
						</summary>
						<div class="tool-group-content">
							{#if group.toolUse}
								<div class="tool-section">
									<div class="tool-section-header">
										<span class="material-symbols-outlined">call_made</span>
										<strong>Request</strong>
									</div>
									<div class="tool-meta">
										<strong>Tool Use ID:</strong> <code>{group.toolUse.id}</code>
									</div>
									{#if group.toolUse.input}
										<div class="tool-input">
											<strong>Input Parameters:</strong>
											<pre class="tool-input-json">{JSON.stringify(group.toolUse.input, null, 2)}</pre>
										</div>
									{/if}
								</div>
							{/if}

							{#if group.toolResult}
								<div class="tool-section">
									<div class="tool-section-header">
										<span class="material-symbols-outlined">call_received</span>
										<strong>Response</strong>
									</div>
									<div class="tool-meta">
										<strong>Tool Use ID:</strong> <code>{group.toolResult.tool_use_id}</code>
									</div>
									<div class="tool-result-content-wrapper">
										<strong>Result:</strong>
										<pre class="tool-result-content">{(() => {
											const content = group.toolResult.content;
											if (typeof content === 'object') {
												return JSON.stringify(content, null, 2);
											}
											if (typeof content === 'string') {
												try {
													return JSON.stringify(JSON.parse(content), null, 2);
												} catch {
													return content;
												}
											}
											return String(content);
										})()}</pre>
									</div>
								</div>
							{/if}
						</div>
					</details>
				{/each}
			</div>
			{/if}
		{/if}

		<div class="message-time">
			{message.sentAt && !isNaN(new Date(message.sentAt).getTime())
				? new Date(message.sentAt).toLocaleTimeString()
				: ''}
		</div>
	</div>
</div>

<style>
	.message {
		display: flex;
		flex-direction: column;
		gap: 4px;
		animation: fadeIn 0.3s;
		width: 100%;
	}

	.message.user {
		align-items: flex-end;
	}

	.message.ai {
		align-items: flex-start;
	}

	.message-action-bar {
		display: none;
		gap: 2px;
		margin-bottom: 2px;
	}

	.message:hover .message-action-bar {
		display: flex;
	}

	.message.user .message-action-bar {
		justify-content: flex-end;
	}

	.message.ai .message-action-bar {
		justify-content: flex-start;
	}

	.action-btn {
		width: 24px;
		height: 24px;
		border-radius: 4px;
		background: transparent;
		border: none;
		color: var(--color-neutral-disabled);
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		transition: color 0.15s, background 0.15s;
		padding: 0;
	}

	.action-btn:hover {
		color: var(--color-neutral-active);
		background: color-mix(in srgb, var(--color-white) 10%, transparent);
	}

	.action-btn .material-symbols-outlined {
		font-size: 14px;
	}

	@keyframes fadeIn {
		from {
			opacity: 0;
			transform: translateY(10px);
		}
		to {
			opacity: 1;
			transform: translateY(0);
		}
	}

	.message-content {
		max-width: 90%;
		display: flex;
		flex-direction: column;
		gap: 4px;
		min-width: 0;
	}

	.message-text {
		background: color-mix(in srgb, var(--color-white) 8%, transparent);
		padding: 8px 12px;
		border-radius: 10px;
		line-height: 1.4;
		font-size: 13px;
		color: var(--color-neutral-active);
		border: 1px solid color-mix(in srgb, var(--color-white) 15%, transparent);
		word-wrap: break-word;
		overflow-wrap: break-word;
		max-width: 100%;
		box-sizing: border-box;
	}

	.message.ai .message-text {
		background: color-mix(in srgb, var(--color-white) 10%, transparent);
		border-color: color-mix(in srgb, var(--color-white) 20%, transparent);
	}

	.message-attachments {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
		margin-top: 8px;
	}

	.attachment-thumbnail {
		display: flex;
		flex-direction: column;
		gap: 6px;
		background: color-mix(in srgb, var(--color-white) 8%, transparent);
		border: 1px solid color-mix(in srgb, var(--color-white) 15%, transparent);
		border-radius: 8px;
		padding: 8px;
		cursor: pointer;
		transition: all 0.2s;
		min-width: 150px;
		max-width: 200px;
		text-align: left;
	}

	.attachment-thumbnail:hover {
		background: color-mix(in srgb, var(--color-white) 12%, transparent);
		border-color: color-mix(in srgb, var(--color-white) 25%, transparent);
		transform: translateY(-1px);
	}

	.attachment-thumbnail:active {
		transform: translateY(0);
	}

	.attachment-image img {
		width: 100%;
		height: 120px;
		border-radius: 6px;
		object-fit: cover;
		background: color-mix(in srgb, var(--color-black) 5%, transparent);
	}

	.pdf-preview,
	.file-preview {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		height: 120px;
		background: color-mix(in srgb, var(--color-white) 5%, transparent);
		border-radius: 6px;
	}

	.pdf-preview .material-symbols-outlined,
	.file-preview .material-symbols-outlined {
		font-size: 48px;
		opacity: 0.4;
	}

	.pdf-label {
		font-size: 14px;
		font-weight: 600;
		opacity: 0.6;
		margin-top: 4px;
	}

	.attachment-info {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 11px;
		color: var(--color-neutral-secondary);
	}

	.attachment-info .material-symbols-outlined {
		font-size: 14px;
		opacity: 0.6;
	}

	.attachment-name {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-size: 11px;
	}

	.attachment-size {
		opacity: 0.7;
		font-size: 10px;
		white-space: nowrap;
	}

	.message-time {
		font-size: 11px;
		color: var(--color-neutral-disabled);
	}

	.thinking-indicator {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 8px 0;
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

	.thinking-text {
		font-size: 14px;
		color: var(--color-neutral);
		font-style: italic;
		animation: thinking-pulse 1.5s infinite ease-in-out;
	}

	@keyframes thinking-pulse {
		0%, 100% {
			opacity: 0.6;
		}
		50% {
			opacity: 1;
		}
	}

	.message.thinking {
		animation: slide-in 0.3s ease-out;
	}

	@keyframes slide-in {
		from {
			opacity: 0;
			transform: translateY(10px);
		}
		to {
			opacity: 1;
			transform: translateY(0);
		}
	}

	.tool-execution-groups {
		margin-top: 12px;
		padding: 12px;
		background: color-mix(in srgb, var(--color-white) 5%, transparent);
		border: 1px solid color-mix(in srgb, var(--color-white) 15%, transparent);
		border-radius: 8px;
		font-size: 13px;
	}

	.tool-header {
		display: flex;
		align-items: center;
		gap: 6px;
		font-weight: 600;
		color: var(--color-neutral-active);
		margin-bottom: 12px;
	}

	.tool-header .material-symbols-outlined {
		font-size: 18px;
	}

	.tool-group {
		margin-bottom: 8px;
		border: 1px solid color-mix(in srgb, var(--color-white) 10%, transparent);
		border-radius: 8px;
		background: color-mix(in srgb, var(--color-white) 3%, transparent);
		overflow: hidden;
	}

	.tool-group-summary {
		padding: 12px 14px;
		cursor: pointer;
		user-select: none;
		display: flex;
		align-items: center;
		gap: 10px;
		transition: background 0.2s;
		font-weight: 600;
	}

	.tool-group-summary:hover {
		background: color-mix(in srgb, var(--color-white) 8%, transparent);
	}

	.tool-group-summary.success {
		border-left: 3px solid var(--color-success);
	}

	.tool-group-summary.error {
		border-left: 3px solid var(--color-error);
	}

	.tool-group-summary.pending {
		border-left: 3px solid var(--color-neutral);
	}

	.tool-group-summary .material-symbols-outlined {
		font-size: 20px;
	}

	.tool-group-summary.success .material-symbols-outlined {
		color: var(--color-success);
	}

	.tool-group-summary.error .material-symbols-outlined {
		color: var(--color-error);
	}

	.tool-group-summary.pending .material-symbols-outlined {
		color: var(--color-neutral);
	}

	.tool-group-title {
		flex: 1;
		color: var(--color-interactive);
	}

	.tool-status-badge {
		font-size: 11px;
		padding: 4px 8px;
		border-radius: 4px;
		font-weight: 600;
	}

	.tool-status-badge.success {
		background: color-mix(in srgb, var(--color-success) 20%, transparent);
		color: var(--color-success);
	}

	.tool-status-badge.error {
		background: color-mix(in srgb, var(--color-error) 20%, transparent);
		color: var(--color-error);
	}

	.tool-group-content {
		padding: 0;
	}

	.tool-section {
		padding: 14px;
		border-top: 1px solid color-mix(in srgb, var(--color-white) 10%, transparent);
	}

	.tool-section:first-child {
		border-top: none;
	}

	.tool-section-header {
		display: flex;
		align-items: center;
		gap: 6px;
		margin-bottom: 10px;
		color: var(--color-neutral-active);
		font-size: 13px;
	}

	.tool-section-header .material-symbols-outlined {
		font-size: 16px;
		color: var(--color-interactive);
	}

	.tool-meta {
		margin-bottom: 8px;
		color: var(--color-neutral);
		font-size: 12px;
	}

	.tool-meta strong {
		color: var(--color-neutral-active);
	}

	.tool-meta code {
		background: color-mix(in srgb, var(--color-black) 20%, transparent);
		padding: 2px 6px;
		border-radius: 3px;
		font-family: 'SF Mono', 'Monaco', 'Courier New', monospace;
		font-size: 11px;
		color: var(--color-neutral-active);
	}

	.tool-input,
	.tool-result-content-wrapper {
		margin-top: 12px;
	}

	.tool-input strong,
	.tool-result-content-wrapper strong {
		display: block;
		margin-bottom: 6px;
		color: var(--color-neutral-active);
		font-size: 12px;
	}

	.tool-input-json,
	.tool-result-content {
		margin: 0;
		padding: 12px;
		background: color-mix(in srgb, var(--color-black) 40%, transparent);
		border: 1px solid color-mix(in srgb, var(--color-white) 10%, transparent);
		border-radius: 6px;
		font-size: 11px;
		font-family: 'SF Mono', 'Monaco', 'Courier New', monospace;
		overflow-x: auto;
		white-space: pre-wrap;
		word-break: break-all;
		max-height: 300px;
		overflow-y: auto;
		color: var(--color-neutral);
		line-height: 1.6;
	}
</style>
