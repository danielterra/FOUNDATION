<script>
	import { invoke } from '@tauri-apps/api/core';
	import { convertFileSrc } from '@tauri-apps/api/core';
	import { openPath } from '@tauri-apps/plugin-opener';
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
	let messageLimit = $state(50);
	let isLoadingMore = $state(false);
	let hasMoreMessages = $state(true);
	let textareaElement = $state(null);
	let pendingAttachments = $state([]);  // {iri, fileName, mimeType, fileSize, localPath}
	let fileInputElement = $state(null);

	// Load recent messages on mount and request location
	onMount(async () => {
		requestLocation();

		// Listen for database events and message updates
		const { listen } = await import('@tauri-apps/api/event');

		// Listen for import-complete in case database is still initializing
		const unlistenImport = await listen('import-complete', async () => {
			await initializeApp();
		});

		// Function to initialize app (API key + messages)
		const initializeApp = async () => {
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
			await loadMessages();
		};

		// Try to initialize immediately (database should already be initialized)
		await initializeApp();

		// Listen for new messages
		const unlistenMessages = await listen('chat-message-added', async () => {
			await loadMessages();
		});

		// Listen for AI processing started (from recovery)
		const unlistenAIProcessing = await listen('ai-processing-started', () => {
			isLoading = true;
		});

		// Cleanup listeners on unmount
		return () => {
			unlistenImport();
			unlistenMessages();
			unlistenAIProcessing();
		};
	});

	function renderMarkdown(text) {
		if (!text) return '';
		return marked.parse(text);
	}

	async function openFile(filePath) {
		if (!filePath) return;
		try {
			await openPath(filePath);
		} catch (err) {
			console.error('Failed to open file:', err);
		}
	}

	// Auto-resize textarea
	function autoResizeTextarea() {
		if (!textareaElement) return;

		// Reset height to auto to get the correct scrollHeight
		textareaElement.style.height = 'auto';

		// Set new height based on scrollHeight, with max of 15 lines (~300px)
		const newHeight = Math.min(textareaElement.scrollHeight, 300);
		textareaElement.style.height = newHeight + 'px';
	}

	// Watch for input text changes to auto-resize
	$effect(() => {
		inputText; // Track inputText changes
		autoResizeTextarea();
	});

	// Group ToolUse with their corresponding ToolResults across all messages
	function groupToolsWithResults(message, allMessages) {
		const grouped = [];

		// Safety check
		if (!allMessages || !Array.isArray(allMessages)) {
			console.warn('[ChatWindow] groupToolsWithResults: allMessages is not an array', allMessages);
			return grouped;
		}

		// First, create a map of all tool results from ALL messages
		const resultsMap = new Map();
		for (const msg of allMessages) {
			if (msg && msg.toolResults) {
				for (const result of msg.toolResults) {
					resultsMap.set(result.resultOfIri, result);
				}
			}
		}

		// Now match each toolUse with its result
		if (message && message.toolUses) {
			for (const toolUse of message.toolUses) {
				grouped.push({
					toolUse,
					toolResult: resultsMap.get(toolUse.iri) || null
				});
			}
		}

		return grouped;
	}

	async function initializeAI(key) {
		try {
			await invoke('ai__initialize', { apiKey: key });
			isInitialized = true;
			showApiKeyInput = false;
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
				limit: messageLimit
			});

			// Check if we got fewer messages than requested (means we've loaded all)
			hasMoreMessages = msgs.length === messageLimit;

			messages = msgs;
			scrollToBottom();
		} catch (err) {
			console.error('Failed to load messages:', err);
		}
	}

	async function loadMoreMessages() {
		if (isLoadingMore || !hasMoreMessages) return;

		isLoadingMore = true;
		const previousScrollHeight = chatContainer?.scrollHeight || 0;

		try {
			// Increase limit to load 50 more messages
			messageLimit += 50;

			const msgs = await invoke('chat__get_recent_messages', {
				limit: messageLimit
			});
			console.log('Loaded more messages:', msgs.length, 'of', messageLimit);

			// Check if we got fewer messages than requested (means we've loaded all)
			hasMoreMessages = msgs.length === messageLimit;

			messages = msgs;

			// Maintain scroll position
			setTimeout(() => {
				if (chatContainer) {
					const newScrollHeight = chatContainer.scrollHeight;
					chatContainer.scrollTop = newScrollHeight - previousScrollHeight;
				}
			}, 0);
		} catch (err) {
			console.error('Failed to load more messages:', err);
		} finally {
			isLoadingMore = false;
		}
	}

	function handleScroll() {
		if (!chatContainer || isLoadingMore) return;

		// If scrolled to top (with small threshold), load more
		if (chatContainer.scrollTop < 100) {
			loadMoreMessages();
		}
	}

	async function sendMessage() {
		if ((!inputText.trim() && pendingAttachments.length === 0) || isLoading || !isInitialized) return;

		const content = inputText.trim();
		const attachmentIris = pendingAttachments.map(a => a.iri);

		inputText = '';
		pendingAttachments = [];
		isLoading = true;

		// Reset textarea height after clearing input
		if (textareaElement) {
			textareaElement.style.height = 'auto';
		}

		// Send user message and get AI reply with location and attachments
		// Don't await - let it run in background so UI updates immediately via events
		invoke('chat__send_and_reply', {
			content,
			latitude: userLocation?.latitude ?? null,
			longitude: userLocation?.longitude ?? null,
			attachmentIris: attachmentIris.length > 0 ? attachmentIris : null
		}).then(() => {
			isLoading = false;
		}).catch(err => {
			console.error('Failed to send message:', err);
			alert('Failed to send message: ' + err);
			isLoading = false;
		});
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

	async function handleFileSelect() {
		console.log('[ChatWindow] handleFileSelect called');
		if (!fileInputElement) {
			console.log('[ChatWindow] fileInputElement is null');
			return;
		}

		const files = fileInputElement.files;
		console.log('[ChatWindow] Selected files:', files?.length || 0);
		if (!files || files.length === 0) return;

		for (const file of files) {
			console.log('[ChatWindow] Processing file:', file.name);
			await attachFile(file);
		}

		// Clear the input
		fileInputElement.value = '';
	}

	async function attachFile(file) {
		try {
			console.log('[ChatWindow] Attaching file:', file.name, file.type, file.size);

			// Check file size (30 MB limit)
			if (file.size > 30 * 1024 * 1024) {
				alert(`File ${file.name} is too large. Maximum size is 30 MB.`);
				return;
			}

			// Only allow images and PDFs
			const isImage = file.type.startsWith('image/');
			const isPDF = file.type === 'application/pdf';

			if (!isImage && !isPDF) {
				alert(`File ${file.name} is not supported. Only images (PNG, JPEG, WebP, GIF) and PDFs are supported.`);
				return;
			}

			// Import Tauri modules
			let tempDir, join, writeFile;
			try {
				const pathModule = await import('@tauri-apps/api/path');
				const fsModule = await import('@tauri-apps/plugin-fs');
				tempDir = pathModule.tempDir;
				join = pathModule.join;
				writeFile = fsModule.writeFile;
			} catch (importErr) {
				console.error('[ChatWindow] Failed to import Tauri modules:', importErr);
				alert('Failed to load required modules. Make sure you are running in Tauri.');
				return;
			}

			// Save file to temporary directory
			const tempPath = await tempDir();
			const timestamp = Date.now();
			const filePath = await join(tempPath, `${timestamp}_${file.name}`);

			// Read file as ArrayBuffer and write using Tauri FS
			const arrayBuffer = await file.arrayBuffer();
			await writeFile(filePath, new Uint8Array(arrayBuffer));

			console.log('[ChatWindow] File saved to temp:', filePath);

			// Call backend to save and register attachment
			const attachmentIri = await invoke('chat__attach_file', {
				filePath,
				fileName: file.name,
				mimeType: file.type
			});

			// Add to pending attachments
			pendingAttachments = [...pendingAttachments, {
				iri: attachmentIri,
				fileName: file.name,
				mimeType: file.type,
				fileSize: file.size,
				localPath: filePath
			}];

			console.log('[ChatWindow] Attachment added:', attachmentIri);
		} catch (err) {
			console.error('[ChatWindow] Failed to attach file:', err);
			alert('Failed to attach file: ' + err);
		}
	}

	function removeAttachment(iri) {
		pendingAttachments = pendingAttachments.filter(a => a.iri !== iri);
	}

	function openFilePicker() {
		console.log('[ChatWindow] openFilePicker called, fileInputElement:', fileInputElement);
		fileInputElement?.click();
	}

	function formatFileSize(bytes) {
		if (bytes === 0) return '0 Bytes';
		const k = 1024;
		const sizes = ['Bytes', 'KB', 'MB', 'GB'];
		const i = Math.floor(Math.log(bytes) / Math.log(k));
		return Math.round(bytes / Math.pow(k, i) * 100) / 100 + ' ' + sizes[i];
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
				<div class="chat-messages" bind:this={chatContainer} onscroll={handleScroll}>
					{#if isLoadingMore}
						<div class="loading-more">
							<span class="material-symbols-outlined spinning">refresh</span>
							<span>Loading more messages...</span>
						</div>
					{/if}
					{#if messages.length === 0}
						<div class="empty-state">
							<span class="material-symbols-outlined">chat_bubble</span>
							<p>Start a conversation with the AI assistant</p>
						</div>
					{:else}
						{#each messages as message}
							<div class="message {message.senderIri === 'foundation:ThisUser' ? 'user' : 'ai'} {message.isThinking ? 'thinking' : ''}">
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
									{:else if message.content}
										<div class="message-text markdown-content">
											{@html renderMarkdown(message.content)}
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

									{#if message.toolUses && message.toolUses.length > 0}
										{@const toolGroups = groupToolsWithResults(message, messages)}
										{#if toolGroups.length > 0}
										<div class="tool-execution-groups">
											<div class="tool-header">
												<span class="material-symbols-outlined">construction</span>
												<span>Tool Executions ({toolGroups.length})</span>
											</div>
											{#each toolGroups as group}
												<details class="tool-group">
													<summary class="tool-group-summary {group.toolResult ? (group.toolResult.isSuccess ? 'success' : 'error') : 'pending'}">
														<span class="material-symbols-outlined">
															{group.toolResult
																? (group.toolResult.isSuccess ? 'check_circle' : 'error')
																: 'pending'}
														</span>
														<span class="tool-group-title">
															{group.toolUse ? group.toolUse.toolName : 'Unknown Tool'}
														</span>
														{#if group.toolResult}
															<span class="tool-status-badge {group.toolResult.isSuccess ? 'success' : 'error'}">
																{group.toolResult.isSuccess ? '✓ Success' : '✗ Failed'}
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
																	<strong>Tool Use ID:</strong> <code>{group.toolUse.toolUseId}</code>
																</div>
																<div class="tool-meta">
																	<strong>IRI:</strong> <code>{group.toolUse.iri}</code>
																</div>
																{#if group.toolUse.input}
																	<div class="tool-input">
																		<strong>Input Parameters:</strong>
																		<pre class="tool-input-json">{JSON.stringify(JSON.parse(group.toolUse.input), null, 2)}</pre>
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
																	<strong>Result IRI:</strong> <code>{group.toolResult.iri}</code>
																</div>
																<div class="tool-result-content-wrapper">
																	<strong>Result:</strong>
																	<pre class="tool-result-content">{(() => {
																		try {
																			return JSON.stringify(JSON.parse(group.toolResult.resultContent), null, 2);
																		} catch {
																			return group.toolResult.resultContent;
																		}
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

				<!-- Hidden file input -->
				<input
					type="file"
					bind:this={fileInputElement}
					onchange={handleFileSelect}
					accept="image/png,image/jpeg,image/webp,image/gif,application/pdf"
					multiple
					style="display: none;"
				/>

				<!-- Pending attachments preview -->
				{#if pendingAttachments.length > 0}
					<div class="attachments-preview">
						{#each pendingAttachments as attachment}
							<div class="attachment-item">
								<span class="material-symbols-outlined">
									{attachment.mimeType.startsWith('image/') ? 'image' : 'picture_as_pdf'}
								</span>
								<span class="attachment-name">{attachment.fileName}</span>
								<span class="attachment-size">{formatFileSize(attachment.fileSize)}</span>
								<button
									class="remove-attachment"
									onclick={() => removeAttachment(attachment.iri)}
									aria-label="Remove attachment"
								>
									<span class="material-symbols-outlined">close</span>
								</button>
							</div>
						{/each}
					</div>
				{/if}

				<!-- Input -->
				<div class="chat-input">
					<button
						class="attach-btn"
						onclick={(e) => {
							console.log('[ChatWindow] Attach button clicked!', e);
							openFilePicker();
						}}
						disabled={isLoading}
						aria-label="Attach file"
					>
						<span class="material-symbols-outlined">attach_file</span>
					</button>
					<textarea
						bind:this={textareaElement}
						bind:value={inputText}
						onkeydown={handleKeydown}
						placeholder="Ask me anything..."
						rows="1"
						disabled={isLoading}
					></textarea>
					<button onclick={sendMessage} disabled={(!inputText.trim() && pendingAttachments.length === 0) || isLoading} aria-label="Send" class:loading={isLoading}>
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

	/* Attachments preview */
	.attachments-preview {
		display: flex;
		flex-direction: column;
		gap: 8px;
		margin-bottom: 12px;
		padding: 12px;
		background: color-mix(in srgb, var(--color-white) 5%, transparent);
		border: 1px solid color-mix(in srgb, var(--color-white) 15%, transparent);
		border-radius: 8px;
	}

	.attachment-item {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px;
		background: color-mix(in srgb, var(--color-white) 8%, transparent);
		border-radius: 6px;
		font-size: 13px;
	}

	.attachment-item .material-symbols-outlined {
		font-size: 20px;
		color: var(--color-interactive);
	}

	.attachment-name {
		flex: 1;
		color: var(--color-neutral-active);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.attachment-size {
		color: var(--color-neutral);
		font-size: 11px;
	}

	.remove-attachment {
		width: 24px;
		height: 24px;
		border-radius: 4px;
		background: transparent;
		border: none;
		color: var(--color-neutral);
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		transition: all 0.2s;
	}

	.remove-attachment:hover {
		background: color-mix(in srgb, var(--color-white) 10%, transparent);
		color: #f44336;
	}

	.remove-attachment .material-symbols-outlined {
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
		color: #e0e0e0; /* Light gray text for good contrast */
		line-height: 1.6;
	}


	/* Loading more indicator */
	.loading-more {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 8px;
		padding: 12px;
		color: var(--color-neutral);
		font-size: 13px;
		background: color-mix(in srgb, var(--color-white) 5%, transparent);
		border-radius: 8px;
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
</style>
