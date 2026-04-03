<script>
	import { invoke } from '@tauri-apps/api/core';
	import { onMount } from 'svelte';
	import Card from './Card.svelte';
	import ChatAttachmentPreview from './ChatAttachmentPreview.svelte';
	import ChatInputArea from './ChatInputArea.svelte';
	import ConversationBar from './ConversationBar.svelte';
	import ChatHeader from './ChatHeader.svelte';
	import ChatApiKeySetup from './ChatApiKeySetup.svelte';
	import ChatErrorBanner from './ChatErrorBanner.svelte';
	import ChatMessageList from './ChatMessageList.svelte';

	const SUPPORTED_IMAGE_TYPES = ['image/jpeg', 'image/png', 'image/gif', 'image/webp'];
	const MESSAGE_PAGE_SIZE = 50;
	const TEXTAREA_MAX_HEIGHT = 300;
	const SCROLL_LOAD_THRESHOLD = 100;
	const MAX_CAMERA_FRAMES_PER_MESSAGE = 4;
	const MAX_FILE_SIZE_BYTES = 30 * 1024 * 1024;
	const CAMERA_FRAME_WIDTH = 320;
	const CAMERA_FRAME_HEIGHT = 240;
	const JPEG_QUALITY = 0.6;
	const MAX_CAPTURED_FRAMES = 60;
	const CAMERA_INACTIVITY_TIMEOUT_MS = 3000;

	// Props
	let { isOpen = $bindable(false), activeConversationIri = $bindable(null) } = $props();

	// State
	let messages = $state([]);
	let inputText = $state('');
	let convLoading = $state({}); // {[iri]: {isLoading, aiStatus, elapsedSeconds, elapsedInterval}}
	let isLoading = $derived(convLoading[activeConversationIri]?.isLoading ?? false);
	let aiStatus = $derived(convLoading[activeConversationIri]?.aiStatus ?? null);
	let chatContainer = $state(null);
	let userLocation = $state(null);
	let apiKey = $state('');
	let isInitialized = $state(false);
	let showApiKeyInput = $state(false);
	let messageLimit = $state(MESSAGE_PAGE_SIZE);
	let isLoadingMore = $state(false);
	let isLoadingMessages = $state(true);
	let hasMoreMessages = $state(true);
	let textareaElement = $state(null);
	let pendingAttachments = $state([]);  // {iri, fileName, mimeType, fileSize, localPath}
	let fileInputElement = $state(null);
	let elapsedSeconds = $derived(convLoading[activeConversationIri]?.elapsedSeconds ?? 0);
	let errorMessage = $state(null);
	let editingMessageIri = $state(null);
	let editingMessageText = $state('');
	let conversations = $state([]);
	let conversationAgent = $state(null);
	let agents = $state([]);
	let loadMessagesVersion = 0;
	let cameraEnabled = $state(localStorage.getItem('camera_vision_enabled') !== 'false');
	let thinkingEnabled = $state(localStorage.getItem('thinking_enabled') !== 'false');
	let cameraStream = null;
	let cameraVideoEl = null;
	let capturedFrames = [];
	let captureTimer = null;
	let captureStarted = false;
	let inactivityTimer = null;

	$effect(() => {
		localStorage.setItem('camera_vision_enabled', String(cameraEnabled));
		if (!cameraEnabled) stopCapturing(false);
	});

	$effect(() => {
		localStorage.setItem('thinking_enabled', String(thinkingEnabled));
	});

	function toggleCamera() {
		cameraEnabled = !cameraEnabled;
	}

	function toggleThinking() {
		thinkingEnabled = !thinkingEnabled;
	}

	$effect(() => {
		if (activeConversationIri) {
			loadConversationAgent(activeConversationIri);
		} else {
			conversationAgent = null;
		}
	});

	onMount(async () => {
		requestLocation();

		const { listen } = await import('@tauri-apps/api/event');

		const initializeApp = async () => {
			const t0 = performance.now();
			console.debug('[INIT] initializeApp start');
			try {
				const storedKey = await invoke('ai__get_api_key');
				console.debug(`[INIT] ai__get_api_key=${Math.round(performance.now() - t0)}ms`);
				if (storedKey) {
					apiKey = storedKey;
					await initializeAI(storedKey);
					console.debug(`[INIT] initializeAI=${Math.round(performance.now() - t0)}ms`);
				} else {
					showApiKeyInput = true;
				}
			} catch (err) {
				console.error('Failed to get API key:', err);
				showApiKeyInput = true;
			}
			await loadConversations();
			console.debug(`[INIT] loadConversations=${Math.round(performance.now() - t0)}ms`);
			if (conversations.length > 0) {
				const saved = localStorage.getItem('activeConversationIri');
				const exists = saved && conversations.some(c => c.iri === saved);
				activeConversationIri = exists ? saved : conversations[0].iri;
				inputText = localStorage.getItem(`draft_${activeConversationIri}`) ?? '';
			}
			await loadMessages();
			console.debug(`[INIT] loadMessages=${Math.round(performance.now() - t0)}ms — done`);
			loadAgents();
		};

		const unlistenImport = await listen('import-complete', async () => {
			await initializeApp();
		});

		await initializeApp();

		const unlistenMessages = await listen('chat-message-added', async () => {
			await loadMessages();
			await loadConversations();
		});

		const unlistenAIProcessing = await listen('ai-processing-started', (event) => {
			const iri = event.payload?.conversationId ?? activeConversationIri;
			startAIStatus('Claude is thinking', iri);
		});

		const unlistenAIStatus = await listen('ai-status', (event) => {
			const iri = event.payload?.conversationId ?? activeConversationIri;
			if (event.payload?.status) {
				startAIStatus(event.payload.status, iri);
			} else {
				stopAIStatus(iri);
			}
		});

		const unlistenAIError = await listen('ai-error', (event) => {
			stopAIStatus(activeConversationIri);
			if (event.payload?.message) {
				showError(event.payload.message);
			}
		});

		function handleChatInject(e) {
			inputText += (inputText ? ' ' : '') + e.detail.text;
			textareaElement?.focus();
		}
		document.addEventListener('chat-inject', handleChatInject);

		return () => {
			unlistenImport();
			unlistenMessages();
			unlistenAIProcessing();
			unlistenAIStatus();
			unlistenAIError();
			document.removeEventListener('chat-inject', handleChatInject);
			for (const state of Object.values(convLoading)) {
				if (state.elapsedInterval) clearInterval(state.elapsedInterval);
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
		textareaElement.style.height = 'auto';
		const newHeight = Math.min(textareaElement.scrollHeight, TEXTAREA_MAX_HEIGHT);
		textareaElement.style.height = newHeight + 'px';
	}

	$effect(() => {
		inputText;
		autoResizeTextarea();
		if (activeConversationIri) {
			localStorage.setItem(`draft_${activeConversationIri}`, inputText);
		}
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
			const agent = await invoke('chat__get_conversation_agent', { conversationId: conversationIri });
			conversationAgent = { iri: agent.iri, label: agent.label, icon: agent.icon };
		} catch {
			conversationAgent = null;
		}
	}

	async function loadAgents() {
		try {
			agents = await invoke('chat__list_agents');
		} catch {
			agents = [];
		}
	}

	async function switchAgent(agentIri) {
		if (!activeConversationIri) return;
		try {
			await invoke('chat__set_conversation_agent', {
				conversationId: activeConversationIri,
				agentIri,
			});
			await loadConversationAgent(activeConversationIri);
		} catch (err) {
			showError(err?.toString() ?? 'Failed to switch assistant');
		}
	}

	async function openEntityInspector(entityIri) {
		try {
			await invoke('widget_blackboard__add_widget', {
				widgetType: 'inspector',
				entityId: entityIri,
				position: null,
				size: null,
				conversationId: activeConversationIri
			});
		} catch (_err) {
		}
	}

	async function openAgentInspector() {
		if (!conversationAgent) return;
		try {
			await invoke('widget_blackboard__add_widget', {
				widgetType: 'inspector',
				entityId: conversationAgent.iri,
				position: null,
				size: null,
				conversationId: activeConversationIri
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
			localStorage.setItem('activeConversationIri', conv.iri);
			messages = [];
			messageLimit = MESSAGE_PAGE_SIZE;
			hasMoreMessages = true;
			await loadConversations();
			await loadMessages();
		} catch (err) {
			console.error('Failed to create conversation:', err);
		}
	}

	async function switchConversation(iri) {
		activeConversationIri = iri;
		loadMessagesVersion++;
		localStorage.setItem('activeConversationIri', iri);
		inputText = localStorage.getItem(`draft_${iri}`) ?? '';
		messages = [];
		messageLimit = MESSAGE_PAGE_SIZE;
		hasMoreMessages = true;
		await loadMessages();
	}

	async function handleDeleteConversation(iri) {
		if (activeConversationIri !== iri) return;
		if (conversations.length > 0) {
			await switchConversation(conversations[0].iri);
		} else {
			await createConversation();
		}
	}

	async function loadMessages() {
		if (!activeConversationIri) return;
		const version = ++loadMessagesVersion;
		try {
			const msgs = await invoke('chat__get_recent_messages', {
				limit: messageLimit,
				conversationId: activeConversationIri
			});
			if (version !== loadMessagesVersion) return;
			hasMoreMessages = msgs.length === messageLimit;
			messages = msgs;
			scrollToBottom();
		} catch (err) {
			if (version !== loadMessagesVersion) return;
			console.error('Failed to load messages:', err);
		} finally {
			if (version === loadMessagesVersion) isLoadingMessages = false;
		}
	}

	async function loadMoreMessages() {
		if (isLoadingMore || !hasMoreMessages) return;

		isLoadingMore = true;
		const previousScrollHeight = chatContainer?.scrollHeight || 0;

		try {
			messageLimit += MESSAGE_PAGE_SIZE;

			const msgs = await invoke('chat__get_recent_messages', {
				limit: messageLimit,
				conversationId: activeConversationIri
			});
			hasMoreMessages = msgs.length === messageLimit;
			messages = msgs;

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
		if (chatContainer.scrollTop < SCROLL_LOAD_THRESHOLD) {
			loadMoreMessages();
		}
	}

	function startAIStatus(status, iri = activeConversationIri) {
		const existing = convLoading[iri];
		if (existing?.elapsedInterval) clearInterval(existing.elapsedInterval);
		const startTime = Date.now();
		const elapsedInterval = setInterval(() => {
			const s = convLoading[iri];
			if (s?.aiStatus) {
				convLoading[iri] = { ...s, elapsedSeconds: Math.floor((Date.now() - s.aiStatus.startTime) / 1000) };
			}
		}, 1000);
		convLoading[iri] = { isLoading: true, aiStatus: { status, startTime }, elapsedSeconds: 0, elapsedInterval };
	}

	function stopAIStatus(iri = activeConversationIri) {
		const existing = convLoading[iri];
		if (!existing) return;
		if (existing.elapsedInterval) clearInterval(existing.elapsedInterval);
		convLoading[iri] = { isLoading: false, aiStatus: null, elapsedSeconds: 0, elapsedInterval: null };
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
		const convIri = activeConversationIri;
		if ((convLoading[convIri]?.isLoading ?? false) || !isInitialized) return;

		startAIStatus('Claude is thinking', convIri);

		invoke('chat__retry_from_message', { messageIri: iri, conversationId: convIri }).then(() => {
			stopAIStatus(convIri);
		}).catch(err => {
			console.error('Failed to retry message:', err);
			showError(err);
			stopAIStatus(convIri);
		});
	}

	async function sendMessage() {
		const convIri = activeConversationIri;
		if ((!inputText.trim() && pendingAttachments.length === 0) || (convLoading[convIri]?.isLoading ?? false) || !isInitialized) return;

		const content = inputText.trim();

		if (editingMessageIri) {
			const iri = editingMessageIri;
			editingMessageIri = null;
			editingMessageText = '';
			inputText = '';
			if (textareaElement) {
				textareaElement.style.height = 'auto';
			}

			startAIStatus('Claude is thinking', convIri);

			invoke('chat__edit_and_retry', { messageIri: iri, newContent: content, conversationId: convIri }).then(() => {
				stopAIStatus(convIri);
			}).catch(err => {
				console.error('Failed to edit message:', err);
				showError(err);
				stopAIStatus(convIri);
			});
			return;
		}

		const attachmentIris = pendingAttachments.map(a => a.iri);
		const cameraImages = cameraEnabled ? selectEvenly(capturedFrames, MAX_CAMERA_FRAMES_PER_MESSAGE) : [];
		stopCapturing(false);

		inputText = '';
		pendingAttachments = [];

		if (textareaElement) {
			textareaElement.style.height = 'auto';
		}

		if (content) {
			messages = [...messages, {
				iri: `optimistic_${Date.now()}`,
				role: 'user',
				content: [{ type: 'text', text: content }],
				timestamp: Date.now(),
			}];
			scrollToBottom();
		}

		startAIStatus('Claude is thinking', convIri);

		invoke('chat__send_and_reply', {
			content,
			latitude: userLocation?.latitude ?? null,
			longitude: userLocation?.longitude ?? null,
			attachmentIris: attachmentIris.length > 0 ? attachmentIris : null,
			conversationId: convIri,
			cameraImages: cameraImages.length > 0 ? cameraImages : null,
			thinkingEnabled,
		}).then(() => {
			stopAIStatus(convIri);
		}).catch(err => {
			console.error('Failed to send message:', err);
			showError(err);
			stopAIStatus(convIri);
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
		const convIri = activeConversationIri;
		stopAIStatus(convIri);
		invoke('chat__cancel', { conversationId: convIri }).catch(err => console.error('Failed to cancel AI:', err));
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

		fileInputElement.value = '';
	}

	async function handlePaste(e) {
		const items = e.clipboardData?.items;
		if (!items) return;

		const imageItem = Array.from(items).find(item => SUPPORTED_IMAGE_TYPES.includes(item.type));
		if (!imageItem) return;

		e.preventDefault();
		const file = imageItem.getAsFile();
		if (!file) return;

		const ext = imageItem.type.split('/')[1] ?? 'png';
		const namedFile = new File([file], `pasted-image-${Date.now()}.${ext}`, { type: imageItem.type });
		await attachFile(namedFile);
	}

	async function attachFile(file) {
		try {
			if (file.size > MAX_FILE_SIZE_BYTES) {
				alert(`File ${file.name} is too large. Maximum size is 30 MB.`);
				return;
			}

			const isImage = SUPPORTED_IMAGE_TYPES.includes(file.type);
			const isPDF = file.type === 'application/pdf';

			if (!isImage && !isPDF) {
				alert(
					`File ${file.name} is not supported. ` +
					'Only images (PNG, JPEG, WebP, GIF) and PDFs are supported.'
				);
				return;
			}

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

			const tempPath = await tempDir();
			const timestamp = Date.now();
			const filePath = await join(tempPath, `${timestamp}_${file.name}`);

			const arrayBuffer = await file.arrayBuffer();
			await writeFile(filePath, new Uint8Array(arrayBuffer));

			const attachmentIri = await invoke('chat__attach_file', {
				filePath,
				fileName: file.name,
				mimeType: file.type
			});

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

	async function initCamera() {
		if (cameraStream) return;
		try {
			cameraStream = await navigator.mediaDevices.getUserMedia({ video: true, audio: false });
			cameraVideoEl = document.createElement('video');
			cameraVideoEl.srcObject = cameraStream;
			cameraVideoEl.muted = true;
			cameraVideoEl.autoplay = true;
			await cameraVideoEl.play();
		} catch {
			cameraStream = null;
			cameraVideoEl = null;
		}
	}

	function captureFrame() {
		if (!cameraVideoEl || cameraVideoEl.readyState < 2) return null;
		try {
			const canvas = document.createElement('canvas');
			canvas.width = CAMERA_FRAME_WIDTH;
			canvas.height = CAMERA_FRAME_HEIGHT;
			canvas.getContext('2d').drawImage(cameraVideoEl, 0, 0, CAMERA_FRAME_WIDTH, CAMERA_FRAME_HEIGHT);
			return canvas.toDataURL('image/jpeg', JPEG_QUALITY).split(',')[1];
		} catch {
			return null;
		}
	}

	function startCapturing() {
		captureStarted = true;
		initCamera().then(() => {
			if (!captureStarted) return;
			captureTimer = setInterval(() => {
				if (capturedFrames.length >= MAX_CAPTURED_FRAMES) return;
				const frame = captureFrame();
				if (frame) capturedFrames.push(frame);
			}, 1000);
		});
	}

	function stopCapturing(keepFrames) {
		captureStarted = false;
		clearInterval(captureTimer);
		captureTimer = null;
		clearTimeout(inactivityTimer);
		inactivityTimer = null;
		if (!keepFrames) capturedFrames = [];
		if (cameraStream) {
			cameraStream.getTracks().forEach(t => t.stop());
			cameraStream = null;
			cameraVideoEl = null;
		}
	}

	function resetCaptureInactivityTimer() {
		clearTimeout(inactivityTimer);
		inactivityTimer = setTimeout(() => stopCapturing(true), CAMERA_INACTIVITY_TIMEOUT_MS);
	}

	function selectEvenly(arr, n) {
		if (arr.length === 0) return [];
		if (arr.length <= n) return arr;
		const result = [];
		for (let i = 0; i < n; i++) {
			result.push(arr[Math.round(i * (arr.length - 1) / (n - 1))]);
		}
		return result;
	}

	$effect(() => {
		if (inputText.length > 0 && cameraEnabled) {
			if (!captureStarted) startCapturing();
			resetCaptureInactivityTimer();
		} else {
			stopCapturing(false);
		}
	});

</script>

<div class="chat-panel">
	<ChatHeader
		{conversationAgent}
		{agents}
		onOpenAgentInspector={openAgentInspector}
		onSwitchAgent={switchAgent}
		onNewConversation={createConversation}
		onDownloadChat={downloadChat}
	/>
	<ConversationBar bind:conversations bind:activeConversationIri onSwitch={switchConversation} onDelete={handleDeleteConversation} />
	<div class="chat-content">
		{#if showApiKeyInput}
			<ChatApiKeySetup bind:apiKey onSave={saveApiKey} />
		{:else}
			<ChatErrorBanner {errorMessage} onDismiss={dismissError} />

			<ChatMessageList
				{messages}
				{isLoadingMessages}
				{isLoadingMore}
				bind:chatContainer
				onScroll={handleScroll}
				{shouldDisplayMessage}
				onEdit={editMessage}
				onRetry={retryMessage}
				onEntityClick={openEntityInspector}
			/>

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
				onPaste={handlePaste}
				bind:textareaElement
				bind:fileInputElement
				{editingMessageIri}
				onCancelEdit={cancelEdit}
				onCancelAI={cancelAI}
				{cameraEnabled}
				onToggleCamera={toggleCamera}
				{thinkingEnabled}
				onToggleThinking={toggleThinking}
			/>
		{/if}
	</div>
</div>

<style>
	.chat-panel {
		width: 100%;
		height: 100%;
		display: flex;
		flex-direction: column;
		background: color-mix(in srgb, var(--color-black) 50%, transparent);
		backdrop-filter: blur(5px);
	}

	.chat-content {
		flex: 1;
		display: flex;
		flex-direction: column;
		overflow: hidden;
		padding: 16px 24px;
	}
</style>
