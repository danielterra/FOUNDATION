<script>
	import { invoke } from '@tauri-apps/api/core';
	import { onMount } from 'svelte';
	import { createEntitySubscription } from '$lib/realtime/subscriptions';
	import { openEntity } from '$lib/blackboard';
	import { marked } from 'marked';
	import ChatAttachmentPreview from './ChatAttachmentPreview.svelte';
	import ChatInputArea from './ChatInputArea.svelte';
	import ConversationBar from './ConversationBar.svelte';
	import ChatHeader from './ChatHeader.svelte';
	import ChatErrorBanner from './ChatErrorBanner.svelte';
	import ChatMessageList from './ChatMessageList.svelte';

	const SUPPORTED_IMAGE_TYPES = ['image/jpeg', 'image/png', 'image/gif', 'image/webp'];
	const SUPPORTED_TEXT_TYPES = ['text/csv', 'text/plain'];
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
	let hasPendingQuestion = $derived(
		messages.length > 0 &&
		messages[messages.length - 1]?.type === 'question' &&
		!messages[messages.length - 1]?.answer
	);
	let chatContainer = $state(null);
	let autoScroll = true;
	let userLocation = $state(null);
	let isInitialized = $state(false);
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
	let streamingReasoning = $state('');
	let streamingSpeak = $state('');
	let activeModelInfo = $state(null);
	// Mutable ref kept in sync with the prop so streaming listener closures
	// (registered once in onMount) always read the current conversation IRI.
	let activeConvIriRef = $state(activeConversationIri);

	let streamingMessage = $derived.by(() => {
		if (!streamingReasoning && !streamingSpeak) return null;
		return {
			type: 'text',
			iri: '__streaming__',
			text: streamingSpeak,
			reasoning: streamingReasoning || null,
			tool_calls: [],
		};
	});

	let visibleMessages = $derived(streamingMessage ? [...messages, streamingMessage] : messages);

	// Non-reactive buffers: chunks accumulate here from Tauri callbacks,
	// then are flushed into $state in a single setTimeout tick to avoid
	// per-chunk re-renders while preserving Svelte 5 reactivity guarantees.
	let reasoningBuf = '';
	let speakBuf = '';
	let flushTimer = null;

	function onStreamChunk(type, text) {
		if (type === 'thinking') reasoningBuf += text;
		else speakBuf += text;
		if (!flushTimer) flushTimer = setTimeout(flushStream, 0);
	}

	function flushStream() {
		flushTimer = null;
		if (reasoningBuf) { streamingReasoning += reasoningBuf; reasoningBuf = ''; }
		if (speakBuf) { streamingSpeak += speakBuf; speakBuf = ''; }
		scrollToBottom(false);
	}

	function clearStreaming() {
		if (flushTimer) { clearTimeout(flushTimer); flushTimer = null; }
		reasoningBuf = speakBuf = '';
		streamingReasoning = streamingSpeak = '';
	}
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
		activeConvIriRef = activeConversationIri;
	});

	$effect(() => {
		if (activeConversationIri) {
			loadConversationAgent(activeConversationIri);
		} else {
			conversationAgent = null;
		}
	});

	// Per-conversation snapshot tx — the max(tx) at the time of the last full load.
	// Used as the cursor for lossless replay when the creation-query fires after a gap.
	let snapshotTx = $state(0);

	// tx cursor for the conversation list — max(tx) at the time of the last loadConversations.
	let listSnapshotTx = $state(0);

	// Watches ALL conversations for new messages so the list stays ordered by last-message
	// timestamp without depending on the active-conversation subscription.
	// One creation-query per convIri is registered; joined-set fires for any conversation.
	const listSub = createEntitySubscription(async (event) => {
		if (event.type !== 'joined-set') return;
		if (event.classIri !== 'foundation:AIConversationMessage') return;
		// Promote the conversation that received a new message to the top.
		const convIri = event.objectValue;
		if (!convIri) return;
		// Idempotency: if tx ≤ cursor the event was already accounted for at snapshot time.
		if (event.tx != null && event.tx <= listSnapshotTx) return;
		await loadConversations();
	});

	// Reactive subscription: watches the active conversation for new messages (via creation-query)
	// and for updates to messages already in the displayed set (via exact IRI subscription).
	// Falls back to full reload only for question-type messages that need the full tool_results map.
	const conversationSub = createEntitySubscription(async (event) => {
		if (!activeConversationIri) return;

		if (event.type === 'joined-set') {
			// A new message arrived for the active conversation.
			if (
				event.classIri !== 'foundation:AIConversationMessage' ||
				event.objectValue !== activeConversationIri
			) return;

			const newIri = event.entityId;
			// Skip if we already have it (idempotent).
			if (messages.some(m => m.iri === newIri)) return;

			// Capture version before any await so a concurrent conversation switch
			// can be detected; results from a stale conversation are discarded.
			const joinedSetVersion = loadMessagesVersion;
			const unit = await invoke('chat__get_message_by_iri', { messageIri: newIri }).catch(() => null);
			if (loadMessagesVersion !== joinedSetVersion) return;
			if (!unit) {
				// question or pure tool_result — reload the full set once to resolve the answer.
				const version = ++loadMessagesVersion;
				const result = await invoke('chat__get_recent_messages', {
					limit: messageLimit,
					conversationId: activeConversationIri
				}).catch(() => null);
				if (!result || version !== loadMessagesVersion) return;
				clearStreaming();
				hasMoreMessages = result.messages.length === messageLimit;
				messages = result.messages;
			} else {
				// The persisted user message replaces the optimistic placeholder added on
				// send: the placeholder carries no real IRI, so the join-set idempotency
				// check above cannot dedupe it — drop it before appending the real unit.
				let base = messages;
				if (unit.type === 'user') {
					const optIdx = base.findIndex(m => m.iri?.startsWith('optimistic_') && m.text === unit.text);
					if (optIdx !== -1) base = [...base.slice(0, optIdx), ...base.slice(optIdx + 1)];
				}
				const lastCurrent = base.at(-1);
				const hasNewAssistantText = unit.type === 'text' && !!unit.text && unit.iri !== lastCurrent?.iri;
				const hasNewToolUse = unit.type === 'tool_use' && unit.iri !== lastCurrent?.iri;
				if (!streamingSpeak || hasNewAssistantText || hasNewToolUse) {
					clearStreaming();
				}
				messages = [...base, unit];
				// Add new message IRI to subscription so future updates to it are received.
				conversationSub.setIris([
					activeConversationIri,
					...messages.map(m => m.iri).filter(Boolean)
				]);
			}
			scrollToBottom(false);
			await loadConversations();
			return;
		}

		if (event.type === 'updated') {
			// A message already in the displayed set was mutated — refetch only that unit.
			const msgIndex = messages.findIndex(m => m.iri === event.entityId);
			if (msgIndex === -1) return;

			const unit = await invoke('chat__get_message_by_iri', { messageIri: event.entityId }).catch(() => null);
			if (!unit) return;

			const updated = [...messages];
			updated[msgIndex] = unit;
			messages = updated;
			return;
		}
	});

	$effect(() => {
		if (!activeConversationIri) {
			conversationSub.setIris([]);
			conversationSub.setCreationQueries([]);
			return;
		}
		// Declare the exact set of IRIs displayed plus a creation-query for new messages.
		conversationSub.setIris([
			activeConversationIri,
			...messages.map(m => m.iri).filter(Boolean)
		]);
		conversationSub.setCreationQueries([{
			classIri: 'foundation:AIConversationMessage',
			predicate: 'foundation:partOfConversation',
			objectValue: activeConversationIri,
		}]);
		conversationSub.setSinceTx(snapshotTx);
	});

	// Keep listSub watching all conversations for new messages so the list stays sorted.
	$effect(() => {
		const queries = conversations.map(c => ({
			classIri: 'foundation:AIConversationMessage',
			predicate: 'foundation:partOfConversation',
			objectValue: c.iri,
		}));
		listSub.setCreationQueries(queries);
		listSub.setSinceTx(listSnapshotTx);
	});

	onMount(async () => {
		requestLocation();

		const onSettingsClosed = () => loadActiveModelInfo();
		window.addEventListener('foundation:settings-closed', onSettingsClosed);

		const { listen } = await import('@tauri-apps/api/event');

		const initializeApp = async () => {
			const t0 = performance.now();
			console.debug('[INIT] initializeApp start');
			try {
				await loadActiveModelInfo();
				console.debug(`[INIT] loadActiveModelInfo=${Math.round(performance.now() - t0)}ms`);
				await initializeAI();
				console.debug(`[INIT] initializeAI=${Math.round(performance.now() - t0)}ms`);
			} catch (err) {
				console.error('Failed to initialize AI:', err);
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

		const unlistenDelta = await listen('chat-ai-delta', (event) => {
			const { conversationId, type, text } = event.payload;
			if (conversationId !== activeConvIriRef) return;
			if (type === 'thinking') {
				onStreamChunk('thinking', text);
			} else if (type === 'speak' || type === 'text') {
				onStreamChunk('speak', text);
			}
		});

		const unlistenAIStatus = await listen('ai-status', (event) => {
			const iri = event.payload?.conversationId ?? activeConvIriRef;
			if (event.payload?.status) {
				startAIStatus(event.payload.status, iri, event.payload?.phase ?? 'llm');
			} else {
				stopAIStatus(iri);
				// ai-status stop fires from Rust after run_conversation_loop completes —
				// all streaming chunks are done and the final message is persisted.
				// Clear any leftover streaming buffer that entity-referenced may have missed.
				if (iri === activeConvIriRef) clearStreaming();
			}
		});

		const unlistenAIError = await listen('ai-error', (event) => {
			stopAIStatus(activeConvIriRef);
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
			unlistenDelta();
			unlistenAIStatus();
			unlistenAIError();
			document.removeEventListener('chat-inject', handleChatInject);
			listSub.destroy();
			conversationSub.destroy();
			for (const state of Object.values(convLoading)) {
				if (state.elapsedInterval) clearInterval(state.elapsedInterval);
			}
		};
	});

	function renderMarkdown(text) {
		if (!text) return '';
		return marked.parse(text, { breaks: true, gfm: true });
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

	async function loadActiveModelInfo() {
		try {
			activeModelInfo = await invoke('setup__get_active_model_info');
		} catch {
			// non-fatal
		}
	}

	async function initializeAI() {
		try {
			await invoke('ai__initialize');
			isInitialized = true;
		} catch (err) {
			console.error('Failed to initialize AI:', err);
			isInitialized = false;
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
			const config = await invoke('agent__get_ai_config', { agentIri: agent.iri });
			activeModelInfo = { modelLabel: config.modelLabel };
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
		await openEntity(entityIri, activeConversationIri).catch(() => {});
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
			// Use the active conversation's snapshot tx as the list cursor — it is the
			// most recent max(tx) we have and conservatively covers any new message
			// written after this load in any conversation.
			if (snapshotTx > listSnapshotTx) {
				listSnapshotTx = snapshotTx;
				listSub.setSinceTx(listSnapshotTx);
				listSub.replayMissed();
			}
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
		clearStreaming();
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
		if (!activeConversationIri) {
			isLoadingMessages = false;
			return;
		}
		const version = ++loadMessagesVersion;
		const convIri = activeConversationIri;
		try {
			// Serial fetch: snapshotTx must be captured AFTER get_recent_messages so it
			// represents a TX <= the max TX already present in the returned messages —
			// any message with tx == snapshotTx is already in the result set, and any
			// tx > snapshotTx will be caught by replayMissed().
			const result = await invoke('chat__get_recent_messages', {
				limit: messageLimit,
				conversationId: convIri,
			});
			const snapTx = await invoke('chat__get_conversation_snapshot_tx', {
				conversationId: convIri,
			}).catch(() => 0);
			if (version !== loadMessagesVersion) return;
			hasMoreMessages = result.messages.length === messageLimit;
			messages = result.messages;
			snapshotTx = /** @type {number} */ (snapTx);
			// Update sinceTx so the subscription replays any events missed during load.
			conversationSub.setSinceTx(snapshotTx);
			conversationSub.replayMissed();
			isLoadingMessages = false;
			syncLoadingFromDb(result.is_processing, activeConversationIri);
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

		try {
			messageLimit += MESSAGE_PAGE_SIZE;

			const result = await invoke('chat__get_recent_messages', {
				limit: messageLimit,
				conversationId: activeConversationIri
			});
			hasMoreMessages = result.messages.length === messageLimit;
			messages = result.messages;
			// column-reverse preserves the viewport position when content is added
			// at the old-messages end (visually at the top), so no scrollTop adjustment needed.
		} catch (err) {
			console.error('Failed to load more messages:', err);
		} finally {
			isLoadingMore = false;
		}
	}

	function handleScroll() {
		if (!chatContainer) return;
		// In column-reverse layout the bottom anchor is scrollTop ≈ 0 (newest messages).
		// scrollTop may be negative in some Chromium/WebView2 builds, so use Math.abs.
		const absScrollTop = Math.abs(chatContainer.scrollTop);
		autoScroll = absScrollTop < 50;
		if (isLoadingMore) return;
		// The "load more" (older messages) trigger is at the visual top, which in
		// column-reverse corresponds to the scrollable extent far from scrollTop=0.
		const distFromOldEnd = chatContainer.scrollHeight - absScrollTop - chatContainer.clientHeight;
		if (distFromOldEnd < SCROLL_LOAD_THRESHOLD) {
			loadMoreMessages();
		}
	}

	function startAIStatus(status, iri = activeConversationIri, phase = 'llm') {
		const existing = convLoading[iri];
		if (existing?.elapsedInterval) clearInterval(existing.elapsedInterval);
		const phaseStart = Date.now();
		const elapsedInterval = setInterval(() => {
			const s = convLoading[iri];
			if (s?.aiStatus) {
				convLoading[iri] = { ...s, elapsedSeconds: Math.floor((Date.now() - s.aiStatus.phaseStart) / 1000) };
			}
		}, 1000);
		convLoading[iri] = { isLoading: true, aiStatus: { status, phase, phaseStart }, elapsedSeconds: 0, elapsedInterval };
	}

	function stopAIStatus(iri = activeConversationIri) {
		const existing = convLoading[iri];
		if (!existing) return;
		if (existing.elapsedInterval) clearInterval(existing.elapsedInterval);
		convLoading[iri] = { isLoading: false, aiStatus: null, elapsedSeconds: 0, elapsedInterval: null };
	}

	function syncLoadingFromDb(is_processing, iri = activeConversationIri) {
		if (!is_processing) {
			stopAIStatus(iri);
		} else if (!convLoading[iri]?.isLoading) {
			startAIStatus('Processando...', iri, 'prep');
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
		const convIri = activeConversationIri;
		if ((convLoading[convIri]?.isLoading ?? false) || !isInitialized) return;
		if (iri.startsWith('optimistic_')) return;

		invoke('chat__retry_from_message', { messageIri: iri, conversationId: convIri }).then(() => {
			stopAIStatus(convIri);
		}).catch(err => {
			console.error('Failed to retry message:', err);
			showError(err);
			stopAIStatus(convIri);
		});
	}

	async function sendMessage() {
		let convIri = activeConversationIri;
		if (!convIri) {
			await createConversation();
			convIri = activeConversationIri;
			if (!convIri) return;
		}
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
		const optimisticAttachments = pendingAttachments.map(a => ({
			fileName: a.fileName,
			filePath: a.localPath,
			fileSize: a.fileSize,
			mimeType: a.mimeType,
		}));
		const cameraImages = cameraEnabled ? selectEvenly(capturedFrames, MAX_CAMERA_FRAMES_PER_MESSAGE) : [];
		stopCapturing(false);

		inputText = '';
		pendingAttachments = [];

		if (textareaElement) {
			textareaElement.style.height = 'auto';
		}

		const optimisticIri = `optimistic_${Date.now()}`;
		if (content) {
			messages = [...messages, {
				type: 'user',
				iri: optimisticIri,
				text: content,
				attachments: optimisticAttachments,
				subconscious_entities: null,
				timestamp: Date.now(),
			}];
			scrollToBottom(true);
		}

		startAIStatus('Enviando...', convIri, 'prep');

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
			messages = messages.filter(m => m.iri !== optimisticIri);
			inputText = content;
			showError(err);
			stopAIStatus(convIri);
		});
	}

	function scrollToBottom(force = false) {
		if (!force && !autoScroll) return;
		autoScroll = true;
		// In column-reverse layout, scrollTop=0 is the bottom anchor (newest messages).
		if (chatContainer) chatContainer.scrollTop = 0;
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
			const isText = SUPPORTED_TEXT_TYPES.includes(file.type);

			if (!isImage && !isPDF && !isText) {
				alert(
					`File ${file.name} is not supported. ` +
					'Supported: images (PNG, JPEG, WebP, GIF), PDFs, and text files (CSV, TXT).'
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
		{cameraEnabled}
		onToggleCamera={toggleCamera}
		{thinkingEnabled}
		onToggleThinking={toggleThinking}
		{activeModelInfo}
	/>
	<ConversationBar bind:conversations bind:activeConversationIri onSwitch={switchConversation} onDelete={handleDeleteConversation} />
	<div class="chat-content">
		<ChatErrorBanner {errorMessage} onDismiss={dismissError} />

		<ChatMessageList
			messages={visibleMessages}
			conversationId={activeConversationIri}
			{isLoadingMessages}
			{isLoadingMore}
			bind:chatContainer
			onScroll={handleScroll}
			onEdit={editMessage}
			onRetry={retryMessage}
			onEntityClick={openEntityInspector}
		/>

		<ChatAttachmentPreview
			pendingAttachments={pendingAttachments}
			onRemove={removeAttachment}
		/>

		{#if !hasPendingQuestion}
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
		background: var(--card);
		border-radius: var(--radius);
		overflow: hidden;
	}

	.chat-content {
		flex: 1;
		display: flex;
		flex-direction: column;
		overflow: hidden;
		padding: 16px 24px;
	}

</style>
