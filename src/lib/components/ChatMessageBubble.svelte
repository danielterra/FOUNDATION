<script>
	import { convertFileSrc } from '@tauri-apps/api/core';
	import { openPath } from '@tauri-apps/plugin-opener';
	import { marked } from 'marked';

	// Configure marked for safe HTML
	marked.setOptions({
		breaks: true,
		gfm: true,
	});

	let { message, messages } = $props();

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

	/* Attachment Styles */
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

	/* Markdown Content Styles */
	.markdown-content :global(h1),
	.markdown-content :global(h2),
	.markdown-content :global(h3),
	.markdown-content :global(h4),
	.markdown-content :global(h5),
	.markdown-content :global(h6) {
		margin: 1.2em 0 0.6em 0;
		font-weight: 600;
		color: var(--color-neutral-active);
	}

	.markdown-content :global(h1:first-child),
	.markdown-content :global(h2:first-child),
	.markdown-content :global(h3:first-child),
	.markdown-content :global(h4:first-child) {
		margin-top: 0;
	}

	.markdown-content :global(h1) { font-size: 1.4em; }
	.markdown-content :global(h2) { font-size: 1.25em; }
	.markdown-content :global(h3) { font-size: 1.1em; }
	.markdown-content :global(h4) { font-size: 1em; }

	.markdown-content :global(p) {
		margin: 0.3em 0;
	}

	.markdown-content :global(p:first-child) {
		margin-top: 0;
	}

	.markdown-content :global(p:last-child) {
		margin-bottom: 0;
	}

	.markdown-content :global(code) {
		background: color-mix(in srgb, var(--color-black) 30%, transparent);
		padding: 2px 5px;
		border-radius: 3px;
		font-family: var(--font-code);
		font-size: 0.9em;
	}

	.markdown-content :global(pre) {
		background: color-mix(in srgb, var(--color-black) 40%, transparent);
		border: 1px solid color-mix(in srgb, var(--color-white) 10%, transparent);
		border-radius: 6px;
		padding: 10px;
		overflow-x: auto;
		margin: 0.5em 0;
	}

	.markdown-content :global(pre code) {
		background: transparent;
		padding: 0;
		border-radius: 0;
	}

	.markdown-content :global(ul),
	.markdown-content :global(ol) {
		margin: 0.2em 0;
		padding-left: 1.5em;
	}

	.markdown-content :global(li) {
		margin: 0.1em 0;
		line-height: 1.3;
	}

	/* Remove espaço entre parágrafo e lista */
	.markdown-content :global(p + ul),
	.markdown-content :global(p + ol) {
		margin-top: 0.1em;
	}

	.markdown-content :global(blockquote) {
		border-left: 3px solid var(--color-interactive);
		padding-left: 12px;
		margin: 0.5em 0;
		color: var(--color-neutral);
		font-style: italic;
	}

	.markdown-content :global(a) {
		color: var(--color-interactive);
		text-decoration: none;
	}

	.markdown-content :global(a:hover) {
		text-decoration: underline;
	}

	.markdown-content :global(strong) {
		font-weight: 400;
		color: var(--color-neutral-active);
	}

	.markdown-content :global(em) {
		font-style: italic;
	}

	.markdown-content :global(hr) {
		border: none;
		border-top: 1px solid color-mix(in srgb, var(--color-white) 15%, transparent);
		margin: 0.8em 0;
	}

	.markdown-content :global(table) {
		border-collapse: collapse;
		width: 100%;
		margin: 0.5em 0;
		font-size: 0.95em;
	}

	.markdown-content :global(th),
	.markdown-content :global(td) {
		border: 1px solid color-mix(in srgb, var(--color-white) 15%, transparent);
		padding: 6px 10px;
		text-align: left;
	}

	.markdown-content :global(th) {
		background: color-mix(in srgb, var(--color-white) 8%, transparent);
		font-weight: 600;
	}

	.message-time {
		font-size: 11px;
		color: var(--color-neutral-disabled);
	}

	/* Thinking indicator */
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

	/* Tool Execution Groups */
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
		border-left: 3px solid #4caf50;
	}

	.tool-group-summary.error {
		border-left: 3px solid #f44336;
	}

	.tool-group-summary.pending {
		border-left: 3px solid var(--color-neutral);
	}

	.tool-group-summary .material-symbols-outlined {
		font-size: 20px;
	}

	.tool-group-summary.success .material-symbols-outlined {
		color: #4caf50;
	}

	.tool-group-summary.error .material-symbols-outlined {
		color: #f44336;
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
		background: color-mix(in srgb, #4caf50 20%, transparent);
		color: #4caf50;
	}

	.tool-status-badge.error {
		background: color-mix(in srgb, #f44336 20%, transparent);
		color: #f44336;
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
		color: #e0e0e0;
		line-height: 1.6;
	}
</style>
