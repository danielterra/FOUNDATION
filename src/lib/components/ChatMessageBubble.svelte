<script>
	import { convertFileSrc } from '@tauri-apps/api/core';
	import ChatQuestionBlock from './ChatQuestionBlock.svelte';
	import { openPath } from '@tauri-apps/plugin-opener';
	import { marked } from 'marked';
	import { onMount, onDestroy } from 'svelte';
	import moment from 'moment';
	import { modal } from '$lib/stores/modal';
	import MarkdownValue from './widgets/inspector/MarkdownValue.svelte';

	marked.setOptions({ breaks: true, gfm: true });

	let { unit, isLast = false, conversationId = '', isStreaming = false, onEdit = null, onRetry = null, onEntityClick = null } = $props();

	let copySuccess = $state(false);
	let copyTimeout = null;
	let now = $state(Date.now());
	let ticker;
	let streamingTextEl = $state(null);

	$effect(() => {
		if (streamingTextEl && unit.text) {
			streamingTextEl.scrollTop = streamingTextEl.scrollHeight;
		}
	});

	onMount(() => {
		ticker = setInterval(() => { now = Date.now(); }, 30_000);
	});

	onDestroy(() => {
		clearInterval(ticker);
	});

	function renderMarkdown(text) {
		if (!text) return '';
		return marked.parse(text)
			.replace(/<table>/g, '<div class="table-wrapper"><table>')
			.replace(/<\/table>/g, '</table></div>');
	}

	function formatJson(content) {
		if (typeof content === 'object') return JSON.stringify(content, null, 2);
		if (typeof content === 'string') {
			try { return JSON.stringify(JSON.parse(content), null, 2); } catch { return content; }
		}
		return String(content);
	}

	function openToolModal(tc) {
		const sections = [];
		if (tc.input) {
			sections.push({ label: 'Request', content: JSON.stringify(tc.input, null, 2) });
		}
		if (tc.result !== null && tc.result !== undefined) {
			sections.push({
				label: 'Response',
				content: formatJson(tc.result),
				isError: !!tc.is_error,
			});
		}
		modal.set({ title: tc.name, sections });
	}

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

	function openSubconsciousModal(entities) {
		const ctx = formatSubconsciousContext(entities);
		modal.set({ title: 'Contexto de Memória', html: marked.parse(ctx) });
	}

	function openReasoningModal() {
		if (!unit.reasoning) return;
		modal.set({ title: 'Pensamentos', html: renderMarkdown(unit.reasoning) });
	}

	async function copyMessage() {
		let text = '';
		if (unit.type === 'user') {
			text = unit.text ?? '';
		} else if (unit.type === 'text') {
			const parts = [unit.reasoning, unit.text].filter(Boolean);
			text = parts.join('\n\n');
		} else if (unit.type === 'question') {
			text = unit.question ?? '';
		}
		try {
			await navigator.clipboard.writeText(text);
			copySuccess = true;
			if (copyTimeout) clearTimeout(copyTimeout);
			copyTimeout = setTimeout(() => { copySuccess = false; }, 2000);
		} catch (err) {
			console.error('Failed to copy:', err);
		}
	}

	function handleEdit() {
		onEdit?.(unit.iri, unit.text ?? '');
	}

	function handleRetry() {
		onRetry?.(unit.iri);
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

	function formatRelativeTime(ts) {
		if (!ts || isNaN(new Date(ts).getTime())) return '';
		now;
		const m = moment(ts);
		return m.isSame(moment(), 'day') ? m.fromNow() : m.calendar();
	}

	function formatAbsoluteTime(ts) {
		if (!ts || isNaN(new Date(ts).getTime())) return '';
		return new Date(ts).toLocaleString(undefined, {
			day: 'numeric', month: 'short', year: 'numeric',
			hour: '2-digit', minute: '2-digit'
		});
	}
</script>

<div class="message {unit.type === 'user' ? 'user' : 'ai'}">
	<div class="message-content">
		{#if unit.type === 'user' && unit.subconscious_entities?.length > 0}
			{@const ctx = formatSubconsciousContext(unit.subconscious_entities)}
			<button class="subconscious-chip-summary" onclick={() => openSubconsciousModal(unit.subconscious_entities)}>
				<span class="material-symbols-outlined subconscious-icon">neurology</span>
				<span class="subconscious-summary-text">{subconsciousSummary(unit.subconscious_entities)}</span>
				<span class="subconscious-tokens">~{estimateTokens(ctx).toLocaleString()} tokens</span>
			</button>
		{/if}
		<div class="message-bubble" class:streaming={isStreaming}>
			{#if unit.type === 'user'}
				{#if unit.text}
					<div class="message-text"><MarkdownValue value={unit.text} openEntityInspector={onEntityClick} /></div>
				{/if}
				{#if unit.attachments?.length > 0}
					<div class="message-attachments">
						{#each unit.attachments as attachment}
							{#if attachment.mimeType.startsWith('image/')}
								<button
									class="attachment-thumbnail attachment-image"
									onclick={() => openFile(attachment.filePath)}
									title="Click to open in default app"
								>
									{#if attachment.filePath}
										<img src={convertFileSrc(attachment.filePath)} alt={attachment.fileName} />
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
										<iframe src={convertFileSrc(attachment.filePath)} scrolling="no" title={attachment.fileName}></iframe>
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
			{:else if unit.type === 'text'}
				{#if unit.reasoning && !isStreaming}
					<button class="tool-chip-summary success" onclick={openReasoningModal}>
						<span class="material-symbols-outlined tool-chip-icon">psychology</span>
						<span class="tool-chip-name">Pensamentos</span>
					</button>
				{/if}
				{#if unit.text || isStreaming}
					{#if isStreaming}
						<div class="streaming-header">
							<span class="material-symbols-outlined streaming-header-icon">psychology</span>
							<span>Pensando...</span>
						</div>
						<div bind:this={streamingTextEl} class="message-text streaming-text">{unit.text}<span class="stream-cursor">▋</span></div>
					{:else}
						<div class="message-text"><MarkdownValue value={unit.text} openEntityInspector={onEntityClick} /></div>
					{/if}
				{/if}
			{:else if unit.type === 'tool_use'}
				{#if unit.reasoning}
					<button class="tool-chip-summary success" onclick={openReasoningModal}>
						<span class="material-symbols-outlined tool-chip-icon">psychology</span>
						<span class="tool-chip-name">Pensamentos</span>
					</button>
				{/if}
				{#if unit.tool_calls?.length > 0}
					<div class="tool-chips">
						{#each unit.tool_calls as tc}
							{@const hasResult = tc.result !== null && tc.result !== undefined}
							<button
								class="tool-chip-summary {hasResult ? (tc.is_error ? 'error' : 'success') : 'pending'}"
								onclick={() => openToolModal(tc)}
							>
								<span class="material-symbols-outlined tool-chip-icon">
									{hasResult ? (tc.is_error ? 'error' : 'check_circle') : 'pending'}
								</span>
								<span class="tool-chip-name">{tc.name}</span>
							</button>
						{/each}
					</div>
				{/if}
			{:else if unit.type === 'question'}
				<ChatQuestionBlock
					q={{ id: unit.id, question: unit.question, question_type: unit.question_type, options: unit.options }}
					answer={unit.answer}
					{isLast}
					{conversationId}
				/>
			{/if}

			<div class="message-bubble-footer">
				<span class="message-time" title={formatAbsoluteTime(unit.timestamp)}>{formatRelativeTime(unit.timestamp)}</span>
				{#if unit.type === 'text' && unit.input_tokens != null}
					<span class="token-pill" title="Input tokens">
						<span class="material-symbols-outlined token-pill-icon">arrow_upward</span>{(unit.input_tokens / 1000).toFixed(1)}k
					</span>
				{/if}
				{#if unit.type === 'text' && unit.output_tokens != null}
					<span class="token-pill" title="Output tokens">
						<span class="material-symbols-outlined token-pill-icon">arrow_downward</span>{unit.output_tokens.toLocaleString()}
					</span>
				{/if}
				{#if unit.type === 'text' && unit.estimated_cost != null}
					<span class="token-pill" title="Estimated cost">
						<span class="material-symbols-outlined token-pill-icon">attach_money</span>{unit.estimated_cost < 0.01 ? '<$0.01' : '$' + unit.estimated_cost.toFixed(2)}
					</span>
				{/if}
			</div>
			<div class="message-action-bar">
				<button class="action-btn" onclick={copyMessage} title="Copy message">
					<span class="material-symbols-outlined">{copySuccess ? 'check' : 'content_copy'}</span>
				</button>
				{#if unit.type === 'user' && onEdit}
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
		background: var(--color-surface-2);
		border-radius: var(--radius-sm);
		padding: 2px;
	}

	.message:hover .message-action-bar {
		opacity: 1;
		pointer-events: auto;
	}

	.action-btn {
		width: 24px;
		height: 24px;
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
	}

	.message.user .message-bubble {
		background: color-mix(in srgb, var(--color-white) 16%, transparent);
	}

	.message.ai .message-bubble {
		background: color-mix(in srgb, var(--color-white) 4%, transparent);
	}

	.message.ai .message-bubble.streaming {
		background: color-mix(in srgb, var(--color-transition) 14%, transparent);
	}

	.message-text {
		line-height: 1.4;
		font-size: 14px;
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
	}

	.pdf-preview iframe {
		width: 100%;
		height: 100%;
		border: none;
		pointer-events: none;
	}

	.file-preview .material-symbols-outlined {
		font-size: 48px;
		opacity: 0.4;
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
		color: #989898;
		background: color-mix(in srgb, var(--color-white) 6%, transparent);
		padding: 1px 6px;
		line-height: 1.6;
		opacity: 0.7;
	}

	.token-pill-icon {
		font-size: 10px;
		line-height: 1;
	}

	.streaming-header {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 0.75em;
		color: var(--color-transition);
		opacity: 0.8;
		margin-bottom: 6px;
		animation: thinking-pulse 1.5s infinite ease-in-out;
	}

	.streaming-header-icon {
		font-size: 14px;
	}

	.streaming-text {
		white-space: pre-wrap;
		line-height: 1.5;
		font-size: 0.78em;
		max-height: 200px;
		overflow-y: auto;
		color: color-mix(in srgb, var(--color-neutral) 70%, transparent);
		font-family: monospace;
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

	.tool-chips {
		display: flex;
		flex-direction: column;
		gap: 1px;
	}

	.tool-chip-summary {
		display: flex;
		align-items: center;
		gap: 5px;
		padding: 2px 0;
		cursor: pointer;
		user-select: none;
		background: none;
		border: none;
		color: inherit;
		width: fit-content;
	}

	.tool-chip-icon {
		font-size: 12px;
		opacity: 0.7;
	}

	.tool-chip-summary.success .tool-chip-icon { color: var(--color-success); opacity: 1; }
	.tool-chip-summary.error .tool-chip-icon { color: var(--color-error); opacity: 1; }
	.tool-chip-summary.pending .tool-chip-icon { color: var(--color-transition); opacity: 1; animation: thinking-pulse 1.5s infinite ease-in-out; }
	.tool-chip-summary.pending .tool-chip-name { color: var(--color-transition); animation: thinking-pulse 1.5s infinite ease-in-out; }

	.tool-chip-name {
		font-size: 11px;
		color: var(--color-neutral);
		font-family: 'SF Mono', 'Monaco', 'Courier New', monospace;
	}

	@keyframes thinking-pulse { 0%, 100% { opacity: 0.6; } 50% { opacity: 1; } }

	.subconscious-chip-summary {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 2px 8px 2px 5px;
		background: color-mix(in srgb, var(--color-interactive) 8%, transparent);
		color: color-mix(in srgb, var(--color-interactive) 85%, var(--color-neutral-disabled));
		cursor: pointer;
		font-size: 10px;
		line-height: 1.4;
		user-select: none;
		border: none;
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
</style>
