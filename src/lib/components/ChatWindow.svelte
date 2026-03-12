<script>
	import { invoke } from '@tauri-apps/api/core';
	import { callMcpTool } from '$lib/utils/mcp';
	import { onMount } from 'svelte';
	import { fly } from 'svelte/transition';
	import { cubicOut } from 'svelte/easing';
	import Card from './Card.svelte';
	import ChatMessageBubble from './ChatMessageBubble.svelte';
	import ChatAttachmentPreview from './ChatAttachmentPreview.svelte';
	import ChatInputArea from './ChatInputArea.svelte';
	import ConversationBar from './ConversationBar.svelte';

	// Props
	let { isOpen = $bindable(false) } = $props();

	// State
	let messages = $state([]);
	let inputText = $state('');
	let isLoading = $state(false);
	let aiStatus = $state(null);  // { status: string, startTime: number }
	let chatContainer = $state(null);
	let userLocation = $state(null);
	let apiKey = $state('');
	let isInitialized = $state(false);
	let showApiKeyInput = $state(false);
	let messageLimit = $state(50);
	let isLoadingMore = $state(false);
	let isLoadingMessages = $state(true);
	let hasMoreMessages = $state(true);
	let textareaElement = $state(null);
	let pendingAttachments = $state([]);  // {iri, fileName, mimeType, fileSize, localPath}
	let fileInputElement = $state(null);
	let elapsedSeconds = $state(0);
	let elapsedInterval = $state(null);
	let errorMessage = $state(null);
	let editingMessageIri = $state(null);
	let editingMessageText = $state('');
	let activeConversationIri = $state(null);
	let conversations = $state([]);
	let conversationAgent = $state(null);

	$effect(() => {
		if (activeConversationIri) {
			loadConversationAgent(activeConversationIri);
		} else {
			conversationAgent = null;
		}
	});

	// Load recent messages on mount and request location
	onMount(async () => {
		requestLocation();

		// Listen for database events and message updates
		const { listen } = await import('@tauri-apps/api/event');

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
			await loadConversations();
			if (conversations.length > 0) {
				activeConversationIri = conversations[0].iri;
			}
			await loadMessages();
		};

		// Listen for import-complete in case database is still initializing
		const unlistenImport = await listen('import-complete', async () => {
			await initializeApp();
		});

		// Try to initialize immediately (database should already be initialized)
		await initializeApp();

		// Listen for new messages
		const unlistenMessages = await listen('chat-message-added', async () => {
			await loadMessages();
		});

		// Listen for AI processing started (from recovery)
		const unlistenAIProcessing = await listen('ai-processing-started', () => {
			startAIStatus('Claude is thinking');
		});

		// Listen for AI status updates
		const unlistenAIStatus = await listen('ai-status', (event) => {
			if (event.payload && event.payload.status) {
				startAIStatus(event.payload.status);
			}
		});

		// Listen for AI errors (e.g. recovery failures)
		const unlistenAIError = await listen('ai-error', (event) => {
			stopAIStatus();
			if (event.payload && event.payload.message) {
				showError(event.payload.message);
			}
		});

		// Listen for diagram node clicks from MermaidWidget
		function handleChatInject(e) {
			inputText += (inputText ? ' ' : '') + e.detail.text;
			textareaElement?.focus();
		}
		document.addEventListener('chat-inject', handleChatInject);

		// Cleanup listeners on unmount
		return () => {
			unlistenImport();
			unlistenMessages();
			unlistenAIProcessing();
			unlistenAIStatus();
			unlistenAIError();
			document.removeEventListener('chat-inject', handleChatInject);
			if (elapsedInterval) {
				clearInterval(elapsedInterval);
			}
		};
	});

	function shouldDisplayMessage(message) {
		if (!Array.isArray(message.content)) return true;
		if (message.content.length === 0) return false;
		const hasOnlyToolResults = message.content.every(block => block.type === 'tool_result');
		return !hasOnlyToolResults;
	}

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
				},
				(error) => {
					console.warn('Failed to get location:', error.message);
				}
			);
		}
	}

	async function loadConversationAgent(conversationIri) {
		try {
			const convResult = await callMcpTool('get_things', { iris: [conversationIri] });
			const conv = convResult.result?.things?.[0];
			const agentIri = conv?.properties?.find(p => p.property === 'foundation:handledBy')?.value;
			if (!agentIri) { conversationAgent = null; return; }

			const agentResult = await callMcpTool('get_things', { iris: [agentIri] });
			const agent = agentResult.result?.things?.[0];
			conversationAgent = { iri: agentIri, label: agent?.label, icon: agent?.icon };
		} catch {
			conversationAgent = null;
		}
	}

	async function openAgentInspector() {
		if (!conversationAgent) return;
		try {
			await invoke('widget_blackboard__add_widget', {
				widgetType: 'inspector',
				entityId: conversationAgent.iri,
				position: null,
				size: null
			});
		} catch (err) {
			console.error('Failed to open agent inspector:', err);
		}
	}

	async function loadConversations() {
		try {
			conversations = await invoke('chat__list_conversations');
		} catch (err) {
			console.error('Failed to load conversations:', err);
		}
	}

	async function createConversation() {
		try {
			const conv = await invoke('chat__create_conversation', { label: null });
			activeConversationIri = conv.iri;
			messages = [];
			messageLimit = 50;
			hasMoreMessages = true;
			await loadConversations();
			await loadMessages();
		} catch (err) {
			console.error('Failed to create conversation:', err);
		}
	}

	async function switchConversation(iri) {
		activeConversationIri = iri;
		messages = [];
		messageLimit = 50;
		hasMoreMessages = true;
		await loadMessages();
	}

	async function loadMessages() {
		if (!activeConversationIri) return;
		try {
			const msgs = await invoke('chat__get_recent_messages', {
				limit: messageLimit,
				conversationId: activeConversationIri
			});

			// Check if we got fewer messages than requested (means we've loaded all)
			hasMoreMessages = msgs.length === messageLimit;

			messages = msgs;
			scrollToBottom();
		} catch (err) {
			console.error('Failed to load messages:', err);
		} finally {
			isLoadingMessages = false;
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
				limit: messageLimit,
				conversationId: activeConversationIri
			});
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

	function startAIStatus(status) {
		isLoading = true;
		aiStatus = {
			status,
			startTime: Date.now()
		};
		elapsedSeconds = 0;

		// Clear any existing interval
		if (elapsedInterval) {
			clearInterval(elapsedInterval);
		}

		// Start counting
		elapsedInterval = setInterval(() => {
			if (aiStatus) {
				elapsedSeconds = Math.floor((Date.now() - aiStatus.startTime) / 1000);
			}
		}, 1000);
	}

	function stopAIStatus() {
		isLoading = false;
		aiStatus = null;
		elapsedSeconds = 0;
		if (elapsedInterval) {
			clearInterval(elapsedInterval);
			elapsedInterval = null;
		}
	}

	function showError(msg) {
		errorMessage = msg;
	}

	function dismissError() {
		errorMessage = null;
	}

	function editMessage(iri, text) {
		editingMessageIri = iri;
		editingMessageText = text;
		inputText = text;
		if (textareaElement) {
			textareaElement.focus();
		}
	}

	function cancelEdit() {
		editingMessageIri = null;
		editingMessageText = '';
		inputText = '';
		if (textareaElement) {
			textareaElement.style.height = 'auto';
		}
	}

	async function retryMessage(iri) {
		if (isLoading || !isInitialized) return;

		startAIStatus('Claude is thinking');

		invoke('chat__retry_from_message', { messageIri: iri, conversationId: activeConversationIri }).then(() => {
			stopAIStatus();
		}).catch(err => {
			console.error('Failed to retry message:', err);
			showError(err);
			stopAIStatus();
		});
	}

	async function sendMessage() {
		if ((!inputText.trim() && pendingAttachments.length === 0) || isLoading || !isInitialized) return;

		const content = inputText.trim();

		if (editingMessageIri) {
			const iri = editingMessageIri;
			editingMessageIri = null;
			editingMessageText = '';
			inputText = '';
			if (textareaElement) {
				textareaElement.style.height = 'auto';
			}

			startAIStatus('Claude is thinking');

			invoke('chat__edit_and_retry', { messageIri: iri, newContent: content, conversationId: activeConversationIri }).then(() => {
				stopAIStatus();
			}).catch(err => {
				console.error('Failed to edit message:', err);
				showError(err);
				stopAIStatus();
			});
			return;
		}

		const attachmentIris = pendingAttachments.map(a => a.iri);

		inputText = '';
		pendingAttachments = [];

		// Reset textarea height after clearing input
		if (textareaElement) {
			textareaElement.style.height = 'auto';
		}

		// Start AI status
		startAIStatus('Claude is thinking');

		// Send user message and get AI reply with location and attachments
		// Don't await - let it run in background so UI updates immediately via events
		invoke('chat__send_and_reply', {
			content,
			latitude: userLocation?.latitude ?? null,
			longitude: userLocation?.longitude ?? null,
			attachmentIris: attachmentIris.length > 0 ? attachmentIris : null,
			conversationId: activeConversationIri
		}).then(() => {
			stopAIStatus();
		}).catch(err => {
			console.error('Failed to send message:', err);
			showError(err);
			stopAIStatus();
		});
	}

	function scrollToBottom() {
		if (chatContainer) {
			setTimeout(() => {
				chatContainer.scrollTop = chatContainer.scrollHeight;
			}, 0);
		}
	}

	function cancelAI() {
		if (!isLoading) return;
		stopAIStatus();
		invoke('chat__cancel').catch(err => console.error('Failed to cancel AI:', err));
	}

	function handleKeydown(e) {
		if (e.key === 'Escape' && isLoading) {
			e.preventDefault();
			cancelAI();
			return;
		}
		if (e.key === 'Enter' && !e.shiftKey) {
			e.preventDefault();
			sendMessage();
		}
	}

	function downloadChat() {
		try {
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

			const blob = new Blob([text], { type: 'text/plain' });
			const url = URL.createObjectURL(blob);
			const a = document.createElement('a');
			a.href = url;
			a.download = `chat-export-${new Date().toISOString()}.txt`;
			document.body.appendChild(a);
			a.click();
			document.body.removeChild(a);
			URL.revokeObjectURL(url);
		} catch (err) {
			console.error('Download error:', err);
			alert('Failed to download: ' + err.message);
		}
	}

	async function handleFileSelect() {
		if (!fileInputElement) {
			return;
		}

		const files = fileInputElement.files;
		if (!files || files.length === 0) return;

		for (const file of files) {
			await attachFile(file);
		}

		// Clear the input
		fileInputElement.value = '';
	}

	async function attachFile(file) {
		try {
			// Check file size (30 MB limit)
			if (file.size > 30 * 1024 * 1024) {
				alert(`File ${file.name} is too large. Maximum size is 30 MB.`);
				return;
			}

			// Only allow images and PDFs
			const isImage = file.type.startsWith('image/');
			const isPDF = file.type === 'application/pdf';

			if (!isImage && !isPDF) {
				alert(
					`File ${file.name} is not supported. ` +
					'Only images (PNG, JPEG, WebP, GIF) and PDFs are supported.'
				);
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

		} catch (err) {
			console.error('[ChatWindow] Failed to attach file:', err);
			alert('Failed to attach file: ' + err);
		}
	}

	function removeAttachment(iri) {
		pendingAttachments = pendingAttachments.filter(a => a.iri !== iri);
	}

</script>

<!-- Chat panel (always visible when isOpen is true) -->
{#if isOpen}
	<div class="chat-panel">
		<div class="chat-header">
			<div class="chat-header-left">
				<button class="agent-avatar" onclick={openAgentInspector} title={conversationAgent ? `Open ${conversationAgent.label} in Inspector` : 'AI Assistant'}>
					<span class="material-symbols-outlined">{conversationAgent?.icon || 'smart_toy'}</span>
				</button>
				<span class="agent-name">{conversationAgent?.label || 'AI Assistant'}</span>
			</div>
			<div class="chat-header-right">
				<button class="header-action-btn" onclick={createConversation} title="New conversation">
					<span class="material-symbols-outlined">add</span>
				</button>
				<button class="header-action-btn" onclick={downloadChat} title="Download chat">
					<span class="material-symbols-outlined">download</span>
				</button>
			</div>
		</div>
		<ConversationBar bind:conversations bind:activeConversationIri onSwitch={switchConversation} />
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
					<!-- Error Banner -->
					{#if errorMessage}
						<div class="error-banner">
							<span class="material-symbols-outlined">error</span>
							<span class="error-text">{errorMessage}</span>
							<button class="error-dismiss" onclick={dismissError} aria-label="Dismiss error">
								<span class="material-symbols-outlined">close</span>
							</button>
						</div>
					{/if}

					<!-- Messages -->
				<div class="chat-messages" bind:this={chatContainer} onscroll={handleScroll}>
					{#if isLoadingMore}
						<div class="loading-more">
							<span class="material-symbols-outlined spinning">refresh</span>
							<span>Loading more messages...</span>
						</div>
					{/if}
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
						{#each messages as message (message.iri)}
							{#if shouldDisplayMessage(message)}
								<div in:fly={{ y: 80, duration: 380, easing: cubicOut }}>
									<ChatMessageBubble
										{message}
										{messages}
										onEdit={editMessage}
										onRetry={retryMessage}
									/>
								</div>
							{/if}
						{/each}
					{/if}
				</div>

			<ChatAttachmentPreview
					pendingAttachments={pendingAttachments}
					onRemove={removeAttachment}
				/>

				<ChatInputArea
					bind:inputText
					{isLoading}
					hasPendingAttachments={pendingAttachments.length > 0}
					{aiStatus}
					{elapsedSeconds}
					onSend={sendMessage}
					onKeydown={handleKeydown}
					onFileSelect={handleFileSelect}
					bind:textareaElement
					bind:fileInputElement
					{editingMessageIri}
					onCancelEdit={cancelEdit}
					onCancelAI={cancelAI}
				/>
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
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 10px 14px;
		border-bottom: 1px solid color-mix(in srgb, var(--color-white) 10%, transparent);
		flex-shrink: 0;
		gap: 8px;
	}

	.chat-header-left {
		display: flex;
		align-items: center;
		gap: 10px;
		min-width: 0;
		flex: 1;
	}

	.agent-avatar {
		width: 36px;
		height: 36px;
		border-radius: 50%;
		background: color-mix(in srgb, var(--color-interactive) 18%, transparent);
		border: 1px solid color-mix(in srgb, var(--color-interactive) 35%, transparent);
		color: var(--color-interactive);
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		transition: all 0.2s;
	}

	.agent-avatar:hover {
		background: color-mix(in srgb, var(--color-interactive) 28%, transparent);
		border-color: var(--color-interactive);
	}

	.agent-avatar .material-symbols-outlined {
		font-size: 18px;
	}

	.agent-name {
		font-size: 14px;
		font-weight: 600;
		color: var(--color-neutral-active);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.chat-header-right {
		display: flex;
		align-items: center;
		gap: 4px;
		flex-shrink: 0;
	}

	.header-action-btn {
		width: 32px;
		height: 32px;
		border-radius: 6px;
		background: transparent;
		border: none;
		color: var(--color-neutral);
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		transition: all 0.15s;
	}

	.header-action-btn:hover {
		background: color-mix(in srgb, var(--color-white) 10%, transparent);
		color: var(--color-neutral-active);
	}

	.header-action-btn .material-symbols-outlined {
		font-size: 18px;
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
		gap: 2px;
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

	/* Error Banner */
	.error-banner {
		display: flex;
		align-items: flex-start;
		gap: 10px;
		padding: 12px 14px;
		background: color-mix(in srgb, var(--color-danger) 15%, transparent);
		border: 1px solid color-mix(in srgb, var(--color-danger) 40%, transparent);
		border-radius: 8px;
		margin-bottom: 12px;
		flex-shrink: 0;
	}

	.error-banner .material-symbols-outlined {
		font-size: 18px;
		color: var(--color-danger-hover);
		flex-shrink: 0;
		margin-top: 1px;
	}

	.error-text {
		flex: 1;
		font-size: 13px;
		color: var(--color-danger-active);
		line-height: 1.4;
		word-break: break-word;
	}

	.error-dismiss {
		background: none;
		border: none;
		cursor: pointer;
		color: var(--color-danger-hover);
		padding: 0;
		display: flex;
		align-items: center;
		flex-shrink: 0;
		opacity: 0.7;
		transition: opacity 0.15s;
	}

	.error-dismiss:hover {
		opacity: 1;
	}

	.error-dismiss .material-symbols-outlined {
		font-size: 16px;
		margin-top: 0;
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
</style>
