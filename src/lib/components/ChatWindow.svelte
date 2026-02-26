<script>
	import { invoke } from '@tauri-apps/api/core';
	import { onMount } from 'svelte';
	import { marked } from 'marked';
	import Card from './Card.svelte';

	// Configure marked for safe HTML
	marked.setOptions({
		breaks: true,
		gfm: true,
	});

	// Props
	let { isOpen = $bindable(false) } = $props();

	// State
	let messages = $state([]);
	let inputText = $state('');
	let isLoading = $state(false);
	let chatContainer = $state(null);
	let userLocation = $state(null);
	let apiKey = $state('');
	let isInitialized = $state(false);
	let showApiKeyInput = $state(false);

	// Load recent messages on mount and request location
	onMount(async () => {
		console.log('[ChatWindow] Component mounted - NEW VERSION WITH DOWNLOAD BUTTON');

		requestLocation();

		// Try to load API key and messages immediately (database should already be initialized)
		try {
			const storedKey = await invoke('ai__get_api_key');
			if (storedKey) {
				apiKey = storedKey;
				await initializeAI(storedKey);
			} else {
				showApiKeyInput = true;
			}
		} catch (err) {
			console.error('Failed to get API key:', err);
			showApiKeyInput = true;
		}

		// Load messages immediately
		await loadMessages();

		// Also listen for import-complete in case database is still initializing
		const { listen } = await import('@tauri-apps/api/event');
		const unlisten = await listen('import-complete', async () => {
			console.log('[ChatWindow] Database re-initialized, reloading...');
			await loadMessages();
		});

		// Cleanup listener on unmount
		return () => {
			unlisten();
		};
	});

	function renderMarkdown(text) {
		if (!text) return '';
		return marked.parse(text);
	}

	async function initializeAI(key) {
		try {
			await invoke('ai__initialize', { apiKey: key });
			isInitialized = true;
			showApiKeyInput = false;
			console.log('AI initialized successfully');
		} catch (err) {
			console.error('Failed to initialize AI:', err);
			alert('Failed to initialize AI. Please check your API key.');
			isInitialized = false;
		}
	}

	async function saveApiKey() {
		if (!apiKey.trim()) return;

		try {
			await invoke('ai__save_api_key', { apiKey: apiKey });
			await initializeAI(apiKey);
		} catch (err) {
			console.error('Failed to save API key:', err);
			alert('Failed to save API key: ' + err);
		}
	}

	function requestLocation() {
		if ('geolocation' in navigator) {
			navigator.geolocation.getCurrentPosition(
				(position) => {
					userLocation = {
						latitude: position.coords.latitude,
						longitude: position.coords.longitude
					};
					console.log('Location obtained:', userLocation);
				},
				(error) => {
					console.warn('Failed to get location:', error.message);
				}
			);
		}
	}

	async function loadMessages() {
		try {
			const msgs = await invoke('chat__get_recent_messages', {
				limit: 100
			});
			console.log('Loaded messages:', msgs);
			messages = msgs;
			scrollToBottom();
		} catch (err) {
			console.error('Failed to load messages:', err);
		}
	}

	async function sendMessage() {
		if (!inputText.trim() || isLoading || !isInitialized) return;

		const content = inputText.trim();
		inputText = '';
		isLoading = true;

		try {
			// Send user message and get AI reply with location if available
			const newMessages = await invoke('chat__send_and_reply', {
				content,
				latitude: userLocation?.latitude ?? null,
				longitude: userLocation?.longitude ?? null
			});

			// Update messages with the response (user message + AI reply)
			messages = newMessages;
			scrollToBottom();
		} catch (err) {
			console.error('Failed to send message:', err);
			alert('Failed to send message: ' + err);
		} finally {
			isLoading = false;
		}
	}

	function scrollToBottom() {
		if (chatContainer) {
			setTimeout(() => {
				chatContainer.scrollTop = chatContainer.scrollHeight;
			}, 0);
		}
	}

	function handleKeydown(e) {
		if (e.key === 'Enter' && !e.shiftKey) {
			e.preventDefault();
			sendMessage();
		}
	}

	function downloadChat() {
		try {
			console.log('Download chat clicked, messages count:', messages.length);
			console.log('Messages:', JSON.stringify(messages, null, 2));

			if (messages.length === 0) {
				alert('No messages to export');
				return;
			}

			let text = '# Chat Export\n\n';

			for (const msg of messages) {
				text += `## Message: ${msg.iri}\n`;
				text += `**From:** ${msg.senderLabel} (${msg.senderIri})\n`;
				text += `**To:** ${msg.receiverLabel} (${msg.receiverIri})\n`;
				text += `**Time:** ${msg.sentAt}\n\n`;

				if (msg.content) {
					text += `**Content:**\n${msg.content}\n\n`;
				}

				if (msg.toolUses && msg.toolUses.length > 0) {
					text += `**Tool Uses:**\n`;
					for (const toolUse of msg.toolUses) {
						text += `  - IRI: ${toolUse.iri}\n`;
						text += `    Tool: ${toolUse.toolName}\n`;
						text += `    Tool Use ID: ${toolUse.toolUseId}\n`;
						text += `    Input: ${toolUse.input}\n`;
					}
					text += '\n';
				}

				if (msg.toolResults && msg.toolResults.length > 0) {
					text += `**Tool Results:**\n`;
					for (const result of msg.toolResults) {
						text += `  - IRI: ${result.iri}\n`;
						text += `    Result of: ${result.resultOfIri}\n`;
						text += `    Success: ${result.isSuccess}\n`;
						text += `    Content: ${result.resultContent}\n`;
					}
					text += '\n';
				}

				text += '---\n\n';
			}

			console.log('Export text length:', text.length);

			const blob = new Blob([text], { type: 'text/plain' });
			const url = URL.createObjectURL(blob);
			const a = document.createElement('a');
			a.href = url;
			a.download = `chat-export-${new Date().toISOString()}.txt`;
			document.body.appendChild(a);
			a.click();
			document.body.removeChild(a);
			URL.revokeObjectURL(url);

			console.log('Download completed');
		} catch (err) {
			console.error('Download error:', err);
			alert('Failed to download: ' + err.message);
		}
	}
</script>

<!-- Chat panel (always visible when isOpen is true) -->
{#if isOpen}
	<div class="chat-panel">
		<div class="chat-header">
			<h2>FOUNDATION</h2>
		</div>
		<div class="chat-content">
				<!-- API Key Input -->
				{#if showApiKeyInput}
					<div class="api-key-setup">
						<h3>Setup Claude API</h3>
						<p>Enter your Anthropic API key to enable AI chat:</p>
						<input
							type="password"
							bind:value={apiKey}
							placeholder="sk-ant-..."
							onkeydown={(e) => e.key === 'Enter' && saveApiKey()}
						/>
						<button onclick={saveApiKey} disabled={!apiKey.trim()}>
							Save API Key
						</button>
						<small>Your API key is securely stored in your local ontology database</small>
					</div>
				{:else}
					<!-- Messages -->
				<div class="chat-messages" bind:this={chatContainer}>
					{#if messages.length === 0}
						<div class="empty-state">
							<span class="material-symbols-outlined">chat_bubble</span>
							<p>Start a conversation with the AI assistant</p>
						</div>
					{:else}
						{#each messages as message}
							<div class="message {message.senderIri === 'foundation:ThisUser' ? 'user' : 'ai'}">
								<div class="message-content">
									{#if message.content}
										<div class="message-text markdown-content">
											{@html renderMarkdown(message.content)}
										</div>
									{/if}

									{#if message.toolUses && message.toolUses.length > 0}
										<div class="tool-uses">
											<div class="tool-header">
												<span class="material-symbols-outlined">construction</span>
												<span>Tools Called ({message.toolUses.length})</span>
											</div>
											{#each message.toolUses as toolUse}
												<details class="tool-item" open>
													<summary class="tool-name">
														<span class="material-symbols-outlined">build</span>
														{toolUse.toolName}
													</summary>
													<div class="tool-details">
														<div class="tool-meta">
															<strong>Tool Use ID:</strong> <code>{toolUse.toolUseId}</code>
														</div>
														<div class="tool-meta">
															<strong>IRI:</strong> <code>{toolUse.iri}</code>
														</div>
														{#if toolUse.input}
															<div class="tool-input">
																<strong>Input Parameters:</strong>
																<pre class="tool-input-json">{JSON.stringify(JSON.parse(toolUse.input), null, 2)}</pre>
															</div>
														{/if}
													</div>
												</details>
											{/each}
										</div>
									{/if}

									{#if message.toolResults && message.toolResults.length > 0}
										<div class="tool-results">
											<div class="tool-header">
												<span class="material-symbols-outlined">{message.toolResults.every(r => r.isSuccess) ? 'check_circle' : 'error'}</span>
												<span>Tool Results ({message.toolResults.length})</span>
											</div>
											{#each message.toolResults as result}
												<details class="tool-result-item" open>
													<summary class="tool-result-summary {result.isSuccess ? 'success' : 'error'}">
														{result.isSuccess ? '✓' : '✗'} Result for {result.resultOfIri}
													</summary>
													<div class="tool-result-details">
														<div class="tool-meta">
															<strong>IRI:</strong> <code>{result.iri}</code>
														</div>
														<div class="tool-meta">
															<strong>Status:</strong> <span class="{result.isSuccess ? 'status-success' : 'status-error'}">{result.isSuccess ? 'Success' : 'Failed'}</span>
														</div>
														<div class="tool-result-content-wrapper">
															<strong>Result:</strong>
															<pre class="tool-result-content">{(() => {
																try {
																	return JSON.stringify(JSON.parse(result.resultContent), null, 2);
																} catch {
																	return result.resultContent;
																}
															})()}</pre>
														</div>
													</div>
												</details>
											{/each}
										</div>
									{/if}

									<div class="message-time">
										{message.sentAt && !isNaN(new Date(message.sentAt).getTime())
											? new Date(message.sentAt).toLocaleTimeString()
											: ''}
									</div>
								</div>
							</div>
						{/each}
					{/if}
				</div>

				<!-- Actions bar -->
				<div class="chat-actions">
					<button
						class="action-btn"
						onclick={downloadChat}
						aria-label="Download chat"
					>
						<span class="material-symbols-outlined">download</span>
					</button>
				</div>

				<!-- Input -->
				<div class="chat-input">
					<textarea
						bind:value={inputText}
						onkeydown={handleKeydown}
						placeholder="Ask me anything..."
						rows="1"
						disabled={isLoading}
					></textarea>
					<button onclick={sendMessage} disabled={!inputText.trim() || isLoading} aria-label="Send" class:loading={isLoading}>
						<span class="material-symbols-outlined">
							{isLoading ? 'hourglass_empty' : 'send'}
						</span>
					</button>
				</div>
				{/if}
		</div>
	</div>
{/if}

<style>
	/* Chat Panel - Fixed Right Side */
	.chat-panel {
		width: 100%;
		height: 100%;
		display: flex;
		flex-direction: column;
		background: color-mix(in srgb, var(--color-black) 40%, transparent);
		backdrop-filter: blur(10px);
	}

	.chat-header {
		padding: 20px 24px;
		border-bottom: 1px solid color-mix(in srgb, var(--color-white) 15%, transparent);
		flex-shrink: 0;
	}

	.chat-header h2 {
		margin: 0;
		font-size: 20px;
		font-weight: 600;
		color: var(--color-neutral-active);
	}

	.chat-content {
		flex: 1;
		display: flex;
		flex-direction: column;
		overflow: hidden;
		padding: 16px 24px;
	}

	/* Messages */
	.chat-messages {
		flex: 1;
		overflow-y: auto;
		overflow-x: hidden;
		display: flex;
		flex-direction: column;
		gap: 16px;
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

	/* Actions bar */
	.chat-actions {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		margin-bottom: 12px;
		padding-bottom: 12px;
		border-bottom: 1px solid color-mix(in srgb, var(--color-white) 20%, transparent);
		flex-shrink: 0;
	}

	.action-btn {
		width: 32px;
		height: 32px;
		border-radius: 8px;
		background: transparent;
		border: 1px solid color-mix(in srgb, var(--color-white) 20%, transparent);
		color: var(--color-neutral);
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		transition: all 0.2s;
	}

	.action-btn:hover {
		background: color-mix(in srgb, var(--color-white) 10%, transparent);
		border-color: var(--color-interactive);
		color: var(--color-interactive);
	}

	.action-btn .material-symbols-outlined {
		font-size: 18px;
	}

	/* Input */
	.chat-input {
		display: flex;
		gap: 8px;
		flex-shrink: 0;
	}

	.chat-input textarea {
		flex: 1;
		border: 1px solid color-mix(in srgb, var(--color-white) 20%, transparent);
		border-radius: 20px;
		padding: 10px 16px;
		font-family: inherit;
		font-size: 14px;
		resize: none;
		min-height: 40px;
		max-height: 80px;
		transition: border-color 0.2s;
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

	/* Scrollbar */
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

	/* API Key Setup */
	.api-key-setup {
		display: flex;
		flex-direction: column;
		gap: 16px;
		padding: 20px;
	}

	.api-key-setup h3 {
		margin: 0;
		color: var(--color-neutral-active);
		font-size: 18px;
	}

	.api-key-setup p {
		margin: 0;
		color: var(--color-neutral);
		font-size: 14px;
	}

	.api-key-setup input {
		padding: 12px 16px;
		border: 1px solid color-mix(in srgb, var(--color-white) 20%, transparent);
		border-radius: 8px;
		background: color-mix(in srgb, var(--color-white) 5%, transparent);
		color: var(--color-neutral-active);
		font-family: inherit;
		font-size: 14px;
	}

	.api-key-setup input:focus {
		outline: none;
		border-color: var(--color-interactive);
	}

	.api-key-setup button {
		padding: 12px 24px;
		border: none;
		border-radius: 8px;
		background: var(--color-interactive);
		color: var(--color-neutral-on-interactive);
		font-size: 14px;
		font-weight: 500;
		cursor: pointer;
		transition: all 0.2s;
	}

	.api-key-setup button:hover:not(:disabled) {
		background: var(--color-interactive-hover);
	}

	.api-key-setup button:disabled {
		background: var(--color-neutral-disabled);
		cursor: not-allowed;
		opacity: 0.5;
	}

	.api-key-setup small {
		color: var(--color-neutral-disabled);
		font-size: 12px;
	}

	/* Tool Uses/Results */
	.tool-uses,
	.tool-results {
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

	.tool-item,
	.tool-result-item {
		margin-bottom: 8px;
		border: 1px solid color-mix(in srgb, var(--color-white) 10%, transparent);
		border-radius: 6px;
		background: color-mix(in srgb, var(--color-white) 3%, transparent);
	}

	.tool-item summary,
	.tool-result-item summary {
		padding: 10px 12px;
		cursor: pointer;
		user-select: none;
		border-radius: 6px;
		transition: background 0.2s;
	}

	.tool-item summary:hover,
	.tool-result-item summary:hover {
		background: color-mix(in srgb, var(--color-white) 8%, transparent);
	}

	.tool-name {
		font-weight: 600;
		color: var(--color-interactive);
		display: flex;
		align-items: center;
		gap: 6px;
	}

	.tool-name .material-symbols-outlined {
		font-size: 16px;
	}

	.tool-details,
	.tool-result-details {
		padding: 12px;
		border-top: 1px solid color-mix(in srgb, var(--color-white) 10%, transparent);
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
		color: #e0e0e0; /* Light gray text for good contrast */
		line-height: 1.6;
	}

	.status-success {
		color: #4caf50;
		font-weight: 600;
	}

	.status-error {
		color: #f44336;
		font-weight: 600;
	}

	.tool-result-item {
		margin-bottom: 4px;
	}

	.tool-result-summary {
		padding: 6px 8px;
		background: color-mix(in srgb, var(--color-white) 3%, transparent);
		border-radius: 4px;
		cursor: pointer;
		font-size: 11px;
	}

	.tool-result-summary:hover {
		background: color-mix(in srgb, var(--color-white) 8%, transparent);
	}

	.tool-result-summary.success {
		color: #4CAF50;
	}

	.tool-result-summary.error {
		color: #f44336;
	}

	.tool-result-content {
		margin: 4px 0 0 0;
		padding: 8px;
		background: color-mix(in srgb, var(--color-black) 30%, transparent);
		border-radius: 4px;
		font-size: 10px;
		font-family: monospace;
		overflow-x: auto;
		white-space: pre-wrap;
		word-break: break-all;
		max-height: 200px;
		overflow-y: auto;
	}
</style>
