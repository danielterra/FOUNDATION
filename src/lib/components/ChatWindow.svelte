<script>
	import { invoke } from '@tauri-apps/api/core';
	import { onMount } from 'svelte';
	import Card from './Card.svelte';

	// Props
	let { isOpen = $bindable(false) } = $props();

	// State
	let messages = $state([]);
	let inputText = $state('');
	let isLoading = $state(false);
	let showHistory = $state(false);
	let chatContainer = $state(null);
	let userLocation = $state(null);
	let apiKey = $state('');
	let isInitialized = $state(false);
	let showApiKeyInput = $state(false);

	// Load recent messages on mount and request location
	onMount(async () => {
		// Check if API key is already stored in ontology
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
		requestLocation();
	});

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
				limit: showHistory ? 100 : 2
			});
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

	function toggleHistory() {
		showHistory = !showHistory;
		loadMessages();
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
</script>

<!-- Floating chat button -->
{#if !isOpen}
	<button class="chat-fab" onclick={() => (isOpen = true)} aria-label="Open chat">
		<span class="material-symbols-outlined">chat</span>
	</button>
{/if}

<!-- Chat window -->
{#if isOpen}
	<div class="chat-window">
		<Card>
			{#snippet children()}
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
									<div class="message-text">{message.content}</div>
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
						onclick={toggleHistory}
						aria-label={showHistory ? 'Hide history' : 'Show full history'}
					>
						<span class="material-symbols-outlined">
							{showHistory ? 'compress' : 'expand'}
						</span>
					</button>
					<button class="action-btn" onclick={() => (isOpen = false)} aria-label="Close chat">
						<span class="material-symbols-outlined">close</span>
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
			{/snippet}
		</Card>
	</div>
{/if}

<style>
	/* Floating Action Button */
	.chat-fab {
		position: fixed;
		bottom: 24px;
		right: 24px;
		width: 56px;
		height: 56px;
		border-radius: 50%;
		background: var(--color-interactive);
		color: var(--color-neutral-on-interactive);
		border: none;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.3);
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		transition: all 0.2s;
		z-index: 1000;
	}

	.chat-fab:hover {
		background: var(--color-interactive-hover);
		box-shadow: 0 6px 16px rgba(0, 0, 0, 0.4);
		transform: scale(1.05);
	}

	.chat-fab:active {
		background: var(--color-interactive-active);
	}

	.chat-fab .material-symbols-outlined {
		font-size: 28px;
	}

	/* Chat Window */
	.chat-window {
		position: fixed;
		bottom: 90px;
		right: 24px;
		width: 400px;
		height: auto;
		max-height: calc(100vh - 140px);
		z-index: 1000;
		display: flex;
		flex-direction: column;
	}

	.chat-window :global(.card) {
		display: flex;
		flex-direction: column;
		padding: 16px;
		overflow: hidden;
		max-height: calc(100vh - 140px);
		height: 100%;
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
		padding: 10px 14px;
		border-radius: 12px;
		line-height: 1.5;
		color: var(--color-neutral-active);
		border: 1px solid color-mix(in srgb, var(--color-white) 15%, transparent);
		word-wrap: break-word;
		overflow-wrap: break-word;
		white-space: pre-wrap;
		max-width: 100%;
		box-sizing: border-box;
	}

	.message.ai .message-text {
		background: color-mix(in srgb, var(--color-white) 10%, transparent);
		border-color: color-mix(in srgb, var(--color-white) 20%, transparent);
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
</style>
