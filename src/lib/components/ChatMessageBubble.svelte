<script>
	import { convertFileSrc } from '@tauri-apps/api/core';
	import ChatQuestionBlock from './ChatQuestionBlock.svelte';
	import { openPath } from '@tauri-apps/plugin-opener';
	import { marked } from 'marked';
	import { onMount, onDestroy } from 'svelte';
	import moment from 'moment';

	function subconsciousSummary(entities) {
		const relevant = entities.filter(e => !e.is_open_loop);
		const openLoops = entities.filter(e => e.is_open_loop);
		const groups = {};
		for (const e of relevant) {
			groups[e.type_label] = (groups[e.type_label] || 0) + 1;
		}
		const parts = Object.entries(groups).map(([type, count]) => `${count} ${type}`);
		if (openLoops.length > 0) {
			parts.push(`${openLoops.length} open loop${openLoops.length !== 1 ? 's' : ''}`);
		}
		return parts.join(', ');
	}

	function estimateTokens(text) {
		return Math.ceil(text.length / 4);
	}

	function formatSubconsciousContext(entities) {
		const relevant = entities.filter(e => !e.is_open_loop);
		const openLoops = entities.filter(e => e.is_open_loop);
		const lines = [];
		if (relevant.length > 0) {
			lines.push('## Memory Context');
			lines.push('Relevant entities from your knowledge graph (ranked by relevance):');
			relevant.forEach((e, i) => {
				lines.push(`${i + 1}. "${e.label}" [${e.type_label}] — ${e.iri}`);
				(e.properties ?? []).forEach(([key, val]) => lines.push(`   - ${key}: ${val}`));
			});
		}
		if (openLoops.length > 0) {
			if (lines.length > 0) lines.push('');
			lines.push('## Open Loops');
			lines.push('Pending problems and tasks requiring your attention:');
			openLoops.forEach(e => {
				lines.push(`- [${e.type_label}] "${e.label}" — ${e.iri}`);
				(e.properties ?? []).forEach(([key, val]) => lines.push(`   - ${key}: ${val}`));
			});
		}
		return lines.join('\n');
	}

	marked.setOptions({
		breaks: true,
		gfm: true,
	});

	function highlightJson(json) {
		return json
			.replace(/&/g, '&amp;')
			.replace(/</g, '&lt;')
			.replace(/>/g, '&gt;')
			.replace(
				/("(\\u[a-zA-Z0-9]{4}|\\[^u]|[^\\"])*"(\s*:)?|\b(true|false|null)\b|-?\d+(?:\.\d*)?(?:[eE][+\-]?\d+)?)/g,
				(match) => {
					if (/^"/.test(match)) {
						if (/:$/.test(match)) return `<span class="json-key">${match}</span>`;
						return `<span class="json-string">${match}</span>`;
					}
					if (/true|false/.test(match)) return `<span class="json-boolean">${match}</span>`;
					if (/null/.test(match)) return `<span class="json-null">${match}</span>`;
					return `<span class="json-number">${match}</span>`;
				}
			);
	}

	function formatJson(content) {
		if (typeof content === 'object') return JSON.stringify(content, null, 2);
		if (typeof content === 'string') {
			try { return JSON.stringify(JSON.parse(content), null, 2); } catch { return content; }
		}
		return String(content);
	}

	let { message, messages, conversationId = '', isStreaming = false, onEdit = null, onRetry = null, onEntityClick = null } = $props();

	let copySuccess = $state(false);
	let copyTimeout = null;
	let now = $state(Date.now());
	let ticker;

	onMount(() => {
		ticker = setInterval(() => { now = Date.now(); }, 30_000);
	});

	onDestroy(() => {
		clearInterval(ticker);
	});

	function getDisplayText(msg) {
		if (!Array.isArray(msg.content)) return '';
		if (msg.role === 'assistant') {
			const speak = msg.content.find(b => b.type === 'speak_output');
			return speak?.text ?? '';
		}
		return msg.content
			.filter(b => b.type === 'text')
			.map(b => b.text ?? '')
			.join('\n\n');
	}

	function getPreSpeakText(msg) {
		if (!Array.isArray(msg.content)) return '';
		return msg.content
			.filter(b => b.type === 'text')
			.map(b => b.text ?? '')
			.join('\n\n');
	}

	function extractToolUses(content) {
		if (!Array.isArray(content)) return [];
		return content.filter(block => block.type === 'tool_use');
	}

	function hasToolUses(msg) {
		return Array.isArray(msg.content) &&
		       msg.content.some(block => block.type === 'tool_use' && block.name !== 'speak' && block.name !== 'ask_question');
	}

	function hasQuestionOutput(msg) {
		return Array.isArray(msg.content) && msg.content.some(b => b.type === 'question_output');
	}

	function getQuestionOutput(msg) {
		if (!Array.isArray(msg.content)) return null;
		return msg.content.find(b => b.type === 'question_output') ?? null;
	}

	function getQuestionAnswer(q, currentMsg) {
		const msgIndex = messages.findIndex(m => m.iri === currentMsg.iri);
		if (msgIndex < 0 || msgIndex >= messages.length - 1) return null;
		const nextMsg = messages[msgIndex + 1];
		if (!Array.isArray(nextMsg?.content)) return null;
		const result = nextMsg.content.find(b => b.type === 'tool_result' && b.tool_use_id === q.id);
		return result?.content ?? null;
	}

	function isLastMessage(msg) {
		return messages.length > 0 && messages[messages.length - 1].iri === msg.iri;
	}

	function hasReasoningContent(msg) {
		if (!Array.isArray(msg.content)) return false;
		return msg.content.some(b => b.type === 'thinking' || b.type === 'redacted_thinking');
	}

	function getReasoningText(msg) {
		if (!Array.isArray(msg.content)) return '';
		return msg.content
			.filter(b => b.type === 'thinking')
			.map(b => b.thinking ?? '')
			.join('\n\n');
	}

	async function copyMessage() {
		const parts = [getPreSpeakText(message), getDisplayText(message)].filter(Boolean);
		try {
			await navigator.clipboard.writeText(parts.join('\n\n'));
			copySuccess = true;
			if (copyTimeout) clearTimeout(copyTimeout);
			copyTimeout = setTimeout(() => { copySuccess = false; }, 2000);
		} catch (err) {
			console.error('Failed to copy:', err);
		}
	}

	function handleEdit() {
		onEdit?.(message.iri, getDisplayText(message));
	}

	function handleRetry() {
		onRetry?.(message.iri);
	}

	function renderMarkdown(text) {
		if (!text) return '';
		return marked.parse(text)
			.replace(/<table>/g, '<div class="table-wrapper"><table>')
			.replace(/<\/table>/g, '</table></div>');
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

	function formatRelativeTime(sentAt) {
		if (!sentAt || isNaN(new Date(sentAt).getTime())) return '';
		now; // reactive dependency — recomputes every 30s
		const m = moment(sentAt);
		return m.isSame(moment(), 'day') ? m.fromNow() : m.calendar();
	}

	function formatAbsoluteTime(sentAt) {
		if (!sentAt || isNaN(new Date(sentAt).getTime())) return '';
		return new Date(sentAt).toLocaleString(undefined, {
			day: 'numeric', month: 'short', year: 'numeric',
			hour: '2-digit', minute: '2-digit'
		});
	}
</script>

<div
	class="message {message.role === 'user' ? 'user' : 'ai'} {message.isThinking ? 'thinking' : ''}"
>
	<div class="message-content">
		{#if message.role === 'user' && message.subconscious_entities?.length > 0}
			{@const ctx = formatSubconsciousContext(message.subconscious_entities)}
			<details class="subconscious-chip">
				<summary class="subconscious-chip-summary">
					<span class="material-symbols-outlined subconscious-icon">neurology</span>
					<span class="subconscious-summary-text">{subconsciousSummary(message.subconscious_entities)}</span>
					<span class="subconscious-tokens">~{estimateTokens(ctx).toLocaleString()} tokens</span>
				</summary>
				<div class="subconscious-context markdown-body">{@html marked.parse(ctx)}</div>
			</details>
		{/if}
		<div class="message-bubble" class:streaming={isStreaming}>
		{#if message.isThinking}
			<div class="thinking-indicator">
				<div class="thinking-dots">
					<span></span>
					<span></span>
					<span></span>
				</div>
				<span class="thinking-text">AI is thinking...</span>
			</div>
		{:else if message.role === 'assistant'}
			{#if hasReasoningContent(message)}
				<details class="reasoning-block">
					<summary class="reasoning-summary">
						<span class="material-symbols-outlined reasoning-icon">psychology</span>
						<span class="reasoning-label">Reasoning</span>
					</summary>
					<div class="reasoning-content markdown-content">
						{@html renderMarkdown(getReasoningText(message))}
					</div>
				</details>
			{/if}
			{@const preSpeakText = getPreSpeakText(message)}
			{#if preSpeakText}
				<details class="reasoning-block" open={isStreaming || undefined}>
					<summary class="reasoning-summary">
						<span class="material-symbols-outlined reasoning-icon">psychology</span>
						<span class="reasoning-label">Reasoning</span>
					</summary>
					{#if isStreaming}
						<div class="reasoning-content streaming-text">{preSpeakText}<span class="stream-cursor">▋</span></div>
					{:else}
						<div class="reasoning-content markdown-content">{@html renderMarkdown(preSpeakText)}</div>
					{/if}
				</details>
			{/if}
			{@const displayText = getDisplayText(message)}
			{#if displayText}
				{#if isStreaming}
					<div class="message-text streaming-text">{displayText}<span class="stream-cursor">▋</span></div>
				{:else}
					<div class="message-text markdown-content">{@html renderMarkdown(displayText)}</div>
				{/if}
			{/if}
			{#if hasQuestionOutput(message)}
				{@const q = getQuestionOutput(message)}
				{#if q}
					<ChatQuestionBlock {q} answer={getQuestionAnswer(q, message)} isLast={isLastMessage(message)} {conversationId} />
				{/if}
			{/if}
		{:else if message.role === 'user'}
			{@const displayText = getDisplayText(message)}
			{#if displayText}
				<div class="message-text markdown-content">
					{@html renderMarkdown(displayText)}
				</div>
			{/if}
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
			<div class="tool-chips">
				{#each toolGroups as group}
					<details class="tool-chip">
						<summary class="tool-chip-summary {group.toolResult ? (group.toolResult.is_error ? 'error' : 'success') : 'pending'}">
							<span class="material-symbols-outlined tool-chip-icon">
								{group.toolResult
									? (group.toolResult.is_error ? 'error' : 'check_circle')
									: 'pending'}
							</span>
							<span class="tool-chip-name">{group.toolUse ? group.toolUse.name : 'unknown'}</span>
						</summary>
						<div class="tool-chip-content">
							{#if group.toolUse?.input}
								<div class="tool-chip-section">
									<span class="tool-chip-label">Request</span>
									<pre class="tool-chip-json">{@html highlightJson(JSON.stringify(group.toolUse.input, null, 2))}</pre>
								</div>
							{/if}
							{#if group.toolResult}
								<div class="tool-chip-section">
									<span class="tool-chip-label {group.toolResult.is_error ? 'error' : ''}">Response</span>
									<pre class="tool-chip-json tool-chip-result {group.toolResult.is_error ? 'error' : ''}">{@html highlightJson(formatJson(group.toolResult.content))}</pre>
								</div>
							{/if}
						</div>
					</details>
				{/each}
			</div>
			{/if}
		{/if}

		{#if !message.isThinking}
			<div class="message-bubble-footer">
				<span class="message-time" title={formatAbsoluteTime(message.timestamp)}>{formatRelativeTime(message.timestamp)}</span>
				{#if message.role === 'assistant' && message.input_tokens != null}
					<span class="token-pill" title="Input tokens">
						<span class="material-symbols-outlined token-pill-icon">arrow_upward</span>{(message.input_tokens / 1000).toFixed(1)}k
					</span>
				{/if}
				{#if message.role === 'assistant' && message.output_tokens != null}
					<span class="token-pill" title="Output tokens">
						<span class="material-symbols-outlined token-pill-icon">arrow_downward</span>{message.output_tokens.toLocaleString()}
					</span>
				{/if}
				{#if message.role === 'assistant' && message.estimated_cost != null}
					<span class="token-pill" title="Estimated cost">
						<span class="material-symbols-outlined token-pill-icon">attach_money</span>{message.estimated_cost < 0.01 ? '<$0.01' : '$' + message.estimated_cost.toFixed(2)}
					</span>
				{/if}
			</div>
			<div class="message-action-bar">
				<button class="action-btn" onclick={copyMessage} title="Copy message">
					<span class="material-symbols-outlined">{copySuccess ? 'check' : 'content_copy'}</span>
				</button>
				{#if message.role === 'user' && onEdit}
					<button class="action-btn" onclick={handleEdit} title="Edit message">
						<span class="material-symbols-outlined">edit</span>
					</button>
				{/if}
				{#if onRetry}
					<button class="action-btn" onclick={handleRetry} title="Retry">
						<span class="material-symbols-outlined">refresh</span>
					</button>
				{/if}
			</div>
		{/if}
		</div>
	</div>
</div>

<style>
	.message {
		display: flex;
		flex-direction: column;
		gap: 4px;
		width: 100%;
	}

	.message.user {
		align-items: flex-end;
	}

	.message.ai {
		align-items: flex-start;
	}

	.message-action-bar {
		position: absolute;
		top: 4px;
		right: 4px;
		display: flex;
		gap: 2px;
		opacity: 0;
		pointer-events: none;
		transition: opacity 0.15s;
		z-index: 1;
		background: color-mix(in srgb, var(--color-black) 75%, transparent);
		border-radius: 6px;
		padding: 2px;
		backdrop-filter: blur(8px);
	}

	.message:hover .message-action-bar {
		opacity: 1;
		pointer-events: auto;
	}

	.action-btn {
		width: 24px;
		height: 24px;
		border-radius: 4px;
		background: transparent;
		border: none;
		color: var(--color-interactive);
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		transition: color 0.15s, background 0.15s;
		padding: 0;
		opacity: 0.7;
	}

	.action-btn .material-symbols-outlined {
		font-size: 14px;
	}

	.message-content {
		max-width: 90%;
		display: flex;
		flex-direction: column;
		gap: 4px;
		min-width: 0;
	}

	.message-bubble {
		position: relative;
		display: flex;
		flex-direction: column;
		gap: 8px;
		min-width: 0;
		padding: 8px 12px;
		border-radius: 10px;
	}

	.message.user .message-bubble {
		background: color-mix(in srgb, var(--color-white) 16%, transparent);
		border: 1px solid color-mix(in srgb, var(--color-white) 28%, transparent);
	}

	.message.ai .message-bubble {
		background: color-mix(in srgb, var(--color-white) 4%, transparent);
		border: 1px solid color-mix(in srgb, var(--color-white) 8%, transparent);
	}

	.message.ai .message-bubble.streaming {
		background: color-mix(in srgb, var(--color-transition) 6%, transparent);
		border-color: color-mix(in srgb, var(--color-transition) 25%, transparent);
	}

	.message-text {
		line-height: 1.4;
		font-size: 13px;
		color: var(--color-neutral-active);
		word-wrap: break-word;
		overflow-wrap: break-word;
		max-width: 100%;
		box-sizing: border-box;
		overflow-x: hidden;
	}

	.message-text :global(.table-wrapper) {
		overflow-x: auto;
		max-width: 100%;
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

	.message-bubble-footer {
		display: flex;
		justify-content: flex-end;
		align-items: center;
		gap: 8px;
		margin-top: 2px;
	}

	.message-time {
		font-size: 10px;
		color: var(--color-neutral-disabled);
		opacity: 0.7;
		line-height: 1;
	}

	.token-pill {
		display: inline-flex;
		align-items: center;
		gap: 2px;
		font-size: 9px;
		font-family: 'SF Mono', 'Monaco', 'Courier New', monospace;
		color: var(--color-neutral-disabled);
		background: color-mix(in srgb, var(--color-white) 6%, transparent);
		border: 1px solid color-mix(in srgb, var(--color-white) 10%, transparent);
		border-radius: 10px;
		padding: 1px 6px;
		line-height: 1.6;
		opacity: 0.7;
	}

	.token-pill-icon {
		font-size: 10px;
		line-height: 1;
	}

	.reasoning-block {
		margin-bottom: 8px;
		border-radius: 6px;
		background: color-mix(in srgb, var(--color-white) 4%, transparent);
		border: 1px solid color-mix(in srgb, var(--color-white) 10%, transparent);
		overflow: hidden;
	}

	.reasoning-summary {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 10px;
		cursor: pointer;
		user-select: none;
		font-size: 12px;
		color: var(--color-neutral);
		list-style: none;
	}

	.reasoning-summary::-webkit-details-marker {
		display: none;
	}

	.reasoning-icon {
		font-size: 14px;
		opacity: 0.6;
	}

	.reasoning-label {
		font-size: 12px;
		font-style: italic;
		opacity: 0.7;
	}

	.reasoning-content {
		padding: 8px 12px 10px;
		border-top: 1px solid color-mix(in srgb, var(--color-white) 8%, transparent);
		font-size: 13px;
		opacity: 0.75;
	}

	.streaming-text {
		white-space: pre-wrap;
		line-height: 1.5;
	}

	@keyframes blink { 0%, 100% { opacity: 1; } 50% { opacity: 0; } }

	.stream-cursor {
		display: inline-block;
		font-size: 0.85em;
		vertical-align: middle;
		animation: blink 0.8s step-start infinite;
		color: var(--color-transition);
		margin-left: 1px;
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

	.thinking-dots span:nth-child(1) { animation-delay: -0.32s; }
	.thinking-dots span:nth-child(2) { animation-delay: -0.16s; }

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

	@keyframes thinking-pulse { 0%, 100% { opacity: 0.6; } 50% { opacity: 1; } }

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

	.tool-chips {
		display: flex;
		flex-direction: column;
		gap: 1px;
	}

	.tool-chip {
		border-radius: 6px;
		overflow: hidden;
	}

	.tool-chip-summary {
		display: flex;
		align-items: center;
		gap: 5px;
		padding: 2px 0;
		cursor: pointer;
		user-select: none;
		list-style: none;
		width: fit-content;
	}

	.tool-chip-summary::-webkit-details-marker { display: none; }

	.tool-chip-icon {
		font-size: 12px;
		opacity: 0.7;
	}

	.tool-chip-summary.success .tool-chip-icon { color: var(--color-success); opacity: 1; }
	.tool-chip-summary.error .tool-chip-icon { color: var(--color-error); opacity: 1; }
	.tool-chip-summary.pending .tool-chip-icon { color: var(--color-neutral); }

	.tool-chip-name {
		font-size: 11px;
		color: var(--color-neutral);
		font-family: 'SF Mono', 'Monaco', 'Courier New', monospace;
	}

	.tool-chip-content {
		margin-top: 4px;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.tool-chip-section {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.tool-chip-label {
		font-size: 9px;
		font-weight: 600;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: var(--color-neutral-disabled);
		padding-left: 2px;
	}

	.tool-chip-label.error {
		color: var(--color-error);
	}

	.tool-chip-json {
		margin: 0;
		padding: 8px;
		background: color-mix(in srgb, var(--color-white) 4%, transparent);
		border: 1px solid color-mix(in srgb, var(--color-white) 8%, transparent);
		border-radius: 6px;
		font-size: 10px;
		font-family: 'SF Mono', 'Monaco', 'Courier New', monospace;
		overflow-x: auto;
		white-space: pre-wrap;
		word-break: break-all;
		max-height: 200px;
		overflow-y: auto;
		color: var(--color-neutral);
		line-height: 1.5;
	}

	.tool-chip-result.error {
		border-color: color-mix(in srgb, var(--color-error) 30%, transparent);
	}

	.tool-chip-json :global(.json-key)     { color: #9cdcfe; }
	.tool-chip-json :global(.json-string)  { color: #ce9178; }
	.tool-chip-json :global(.json-number)  { color: #b5cea8; }
	.tool-chip-json :global(.json-boolean) { color: #569cd6; }
	.tool-chip-json :global(.json-null)    { color: #569cd6; }

	.subconscious-chip {
		margin-bottom: 2px;
	}

	.subconscious-chip-summary {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 2px 8px 2px 5px;
		border-radius: 12px;
		border: 1px solid color-mix(in srgb, var(--color-interactive) 35%, transparent);
		background: color-mix(in srgb, var(--color-interactive) 8%, transparent);
		color: color-mix(in srgb, var(--color-interactive) 85%, var(--color-neutral-disabled));
		cursor: pointer;
		font-size: 10px;
		line-height: 1.4;
		list-style: none;
		user-select: none;
		transition: background 0.15s, border-color 0.15s;
	}

	.subconscious-chip-summary::-webkit-details-marker { display: none; }

	.subconscious-chip-summary:hover {
		background: color-mix(in srgb, var(--color-interactive) 15%, transparent);
		border-color: color-mix(in srgb, var(--color-interactive) 55%, transparent);
	}

	.subconscious-icon {
		font-size: 12px;
		flex-shrink: 0;
		font-variation-settings: 'FILL' 1;
	}

	.subconscious-tokens {
		opacity: 0.5;
		font-size: 9px;
		white-space: nowrap;
		margin-left: 2px;
	}

	.subconscious-summary-text {
		font-weight: 500;
	}

	.subconscious-context {
		margin: 4px 0 0;
		padding: 8px 10px;
		border-radius: 6px;
		background: color-mix(in srgb, var(--color-interactive) 5%, var(--color-surface-2, #0f172a));
		border: 1px solid color-mix(in srgb, var(--color-interactive) 20%, transparent);
		font-size: 10px;
		line-height: 1.6;
		color: var(--color-neutral);
		word-break: break-word;
		overflow-x: auto;
		max-height: 400px;
		overflow-y: auto;
	}

	.subconscious-context :global(h1),
	.subconscious-context :global(h2),
	.subconscious-context :global(h3) {
		font-size: 11px;
		font-weight: 600;
		margin: 8px 0 2px;
		color: color-mix(in srgb, var(--color-interactive) 85%, var(--color-neutral));
	}

	.subconscious-context :global(p) {
		margin: 2px 0;
	}

	.subconscious-context :global(ul),
	.subconscious-context :global(ol) {
		margin: 2px 0;
		padding-left: 16px;
	}

	.subconscious-context :global(li) {
		margin: 1px 0;
	}

	.subconscious-context :global(code) {
		font-family: monospace;
		font-size: 9px;
		background: color-mix(in srgb, var(--color-interactive) 12%, transparent);
		padding: 1px 3px;
		border-radius: 3px;
	}

	.subconscious-context :global(pre) {
		background: color-mix(in srgb, var(--color-interactive) 12%, transparent);
		padding: 6px 8px;
		border-radius: 4px;
		overflow-x: auto;
		margin: 4px 0;
	}

	.subconscious-context :global(pre code) {
		background: none;
		padding: 0;
	}

	.subconscious-context :global(strong) {
		font-weight: 600;
		color: color-mix(in srgb, var(--color-neutral) 90%, var(--color-interactive));
	}

	.subconscious-context :global(hr) {
		border: none;
		border-top: 1px solid color-mix(in srgb, var(--color-interactive) 20%, transparent);
		margin: 6px 0;
	}

</style>
