<script>
	import { invoke } from '@tauri-apps/api/core';
	import { Button } from '$lib/components/ui/button';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Input } from '$lib/components/ui/input';
	import { Checkbox } from '$lib/components/ui/checkbox';
	import * as Select from '$lib/components/ui/select';
	import posthog from 'posthog-js';

	let { onClose } = $props();

	// --- AI Configuration (Provedor + Chave de API + Modelo) ---
	let services = $state([]);
	let models = $state([]);
	let selectedServiceIri = $state('');
	let selectedModelIri = $state('');
	let apiKey = $state('');
	let apiKeyLoading = $state(false);
	let apiKeyDeleting = $state(false);

	let loadedServiceIri = $state('');
	let loadedModelIri = $state('');
	let loadedApiKey = $state('');
	let baseUrl = $state('');
	let loadedBaseUrl = $state('');
	let openRouterModels = $state([]);
	let selectedOpenRouterModelId = $state('');
	let loadedOpenRouterModelId = $state('');
	let fallbackModelIds = $state([]);
	let loadedFallbackModelIds = $state([]);

	let aiSaving = $state(false);
	let aiMessage = $state('');
	let aiError = $state(false);

	let selectedServiceIsLocal = $derived(
		services.find(s => s.iri === selectedServiceIri)?.isLocal ?? false
	);
	let selectedServiceIsOpenRouter = $derived(
		baseUrl.includes('openrouter.ai')
	);
	let currentModelLabel = $state('');

	let aiDirty = $derived(
		selectedServiceIri !== loadedServiceIri
		|| selectedModelIri !== loadedModelIri
		|| (!selectedServiceIsLocal && apiKey !== loadedApiKey)
		|| (selectedServiceIsOpenRouter && baseUrl !== loadedBaseUrl)
		|| (selectedServiceIsOpenRouter && selectedOpenRouterModelId !== loadedOpenRouterModelId)
		|| (selectedServiceIsOpenRouter && JSON.stringify(fallbackModelIds) !== JSON.stringify(loadedFallbackModelIds))
	);
	let aiCanSave = $derived(
		!!selectedServiceIri
		&& (selectedServiceIsOpenRouter ? !!selectedOpenRouterModelId : !!selectedModelIri)
		&& (selectedServiceIsLocal || apiKey.trim().length > 0)
		&& aiDirty
	);

	// --- Local Model ---
	let localModelExists = $state(false);
	let localModelSizeHuman = $state('~4.9 GB');
	let localModelIsDownloading = $state(false);
	let localModelProgress = $state(0);
	let localModelDownloadedBytes = $state(0);
	let localModelTotalBytes = $state(0);
	let localModelMessage = $state('');
	let localModelError = $state(false);
	let localModelUnlisten = $state(null);

	// --- Logs ---
	let logPath = $state('');
	let logClearing = $state(false);
	let logMessage = $state('');
	let logError = $state(false);

	// --- Claude Desktop ---
	let claudeConnecting = $state(false);
	let claudeMessage = $state('');
	let claudeError = $state(false);

	// --- Analytics ---
	let analyticsEnabled = $state(false);

	function loadAnalyticsConsent() {
		try {
			analyticsEnabled = posthog.has_opted_in_capturing();
		} catch {
			analyticsEnabled = false;
		}
	}

	function toggleAnalytics(enabled) {
		try {
			if (enabled) {
				posthog.opt_in_capturing();
			} else {
				posthog.opt_out_capturing();
			}
			analyticsEnabled = enabled;
		} catch {
			// posthog não inicializado (chave ausente)
		}
	}

	// --- Foundation Dir ---
	let foundationDir = $state('');
	let foundationDirSaving = $state(false);
	let foundationDirMessage = $state('');
	let foundationDirError = $state(false);

	// --- IMAP ---
	let imapAccounts = $state([]);
	let imapEditing = $state(null); // null = hidden, 'new' = new form, iri = editing existing
	let imapForm = $state({ label: '', host: '', port: 993, use_tls: true, username: '', password: '', sync_interval_minutes: 15 });
	let imapTesting = $state(false);
	let imapSaving = $state(false);
	let imapDeleting = $state(false);
	let imapMessage = $state('');
	let imapError = $state(false);
	let imapAvailableFolders = $state([]);
	let imapSelectedFolders = $state([]);
	let imapLoadingFolders = $state(false);
	let imapSyncHistory = $state([]);
	let imapHistoryAccountIri = $state(null);
	let imapHasExistingPassword = $state(false);

	$effect(() => {
		loadAll();
		return () => { localModelUnlisten?.(); };
	});

	async function loadAll() {
		await Promise.all([loadModelData(), loadLogPath(), loadLocalModelStatus(), loadImapAccounts(), loadFoundationDir()]);
		await loadApiKeyForService(selectedServiceIri);
		if (selectedServiceIsOpenRouter && apiKey.trim()) {
			await loadOpenRouterModels();
		}
		loadAnalyticsConsent();
	}

	async function loadFoundationDir() {
		try {
			foundationDir = await invoke('settings__get_foundation_dir');
		} catch {
			// ignore
		}
	}

	async function browseFoundationDir() {
		const { open } = await import('@tauri-apps/plugin-dialog');
		const selected = await open({ directory: true, title: 'Selecionar pasta do Foundation' });
		if (selected) foundationDir = selected;
	}

	async function saveFoundationDir() {
		foundationDirSaving = true;
		foundationDirMessage = '';
		foundationDirError = false;
		try {
			await invoke('settings__set_foundation_dir', { path: foundationDir });
		} catch (e) {
			foundationDirMessage = String(e);
			foundationDirError = true;
			foundationDirSaving = false;
		}
	}

	async function loadLocalModelStatus() {
		try {
			const status = await invoke('setup__get_local_model_status');
			localModelExists = status.exists;
			localModelSizeHuman = status.sizeHuman;
			localModelIsDownloading = status.isDownloading;
		} catch {
			// ignore
		}
	}

	async function startModelDownload() {
		localModelError = false;
		localModelMessage = '';
		localModelIsDownloading = true;
		localModelProgress = 0;

		const { listen } = await import('@tauri-apps/api/event');

		const unlistenProgress = await listen('local-model-download-progress', (event) => {
			localModelProgress = event.payload.percentage;
			localModelDownloadedBytes = event.payload.downloadedBytes;
			localModelTotalBytes = event.payload.totalBytes;
		});

		const unlistenComplete = await listen('local-model-download-complete', () => {
			localModelIsDownloading = false;
			localModelExists = true;
			localModelMessage = 'Modelo instalado com sucesso.';
			localModelProgress = 100;
			unlistenProgress();
			unlistenComplete();
			unlistenError();
			loadLocalModelStatus();
		});

		const unlistenError = await listen('local-model-download-error', (event) => {
			localModelIsDownloading = false;
			localModelError = !event.payload.cancelled;
			localModelMessage = event.payload.error;
			unlistenProgress();
			unlistenComplete();
			unlistenError();
		});

		localModelUnlisten = () => {
			unlistenProgress();
			unlistenComplete();
			unlistenError();
		};

		try {
			await invoke('setup__download_local_model');
		} catch (e) {
			localModelIsDownloading = false;
			localModelError = true;
			localModelMessage = String(e);
		}
	}

	async function cancelModelDownload() {
		try {
			await invoke('setup__cancel_local_model_download');
		} catch {
			// ignore
		}
	}

	function formatBytes(bytes) {
		if (bytes === 0) return '0 B';
		const gb = bytes / 1_073_741_824;
		if (gb >= 1) return `${gb.toFixed(1)} GB`;
		const mb = bytes / 1_048_576;
		return `${mb.toFixed(0)} MB`;
	}

	async function loadApiKeyForService(serviceIri) {
		if (!serviceIri) {
			apiKey = '';
			loadedApiKey = '';
			baseUrl = '';
			loadedBaseUrl = '';
			return;
		}
		apiKeyLoading = true;
		try {
			const [key, url, fbModels] = await Promise.all([
				invoke('ai__get_api_key', { serviceIri }),
				invoke('ai__get_service_base_url', { serviceIri }),
				invoke('ai__get_fallback_models', { serviceIri }),
			]);
			apiKey = key ?? '';
			loadedApiKey = apiKey;
			baseUrl = url ?? '';
			loadedBaseUrl = baseUrl;
			fallbackModelIds = fbModels ?? [];
			loadedFallbackModelIds = [...fallbackModelIds];
		} catch {
			apiKey = '';
			loadedApiKey = '';
			baseUrl = '';
			loadedBaseUrl = '';
			fallbackModelIds = [];
			loadedFallbackModelIds = [];
		} finally {
			apiKeyLoading = false;
		}
	}

	async function deleteApiKey() {
		if (!selectedServiceIri || selectedServiceIsLocal) return;
		apiKeyDeleting = true;
		aiMessage = '';
		aiError = false;
		try {
			await invoke('ai__delete_api_key', { serviceIri: selectedServiceIri });
			apiKey = '';
			loadedApiKey = '';
			aiMessage = 'Chave de API removida.';
		} catch (e) {
			aiMessage = String(e);
			aiError = true;
		} finally {
			apiKeyDeleting = false;
		}
	}

	async function loadModelData() {
		try {
			const [svcs, current, currentSvc] = await Promise.all([
				invoke('setup__list_ai_services'),
				invoke('setup__get_current_ai_model'),
				invoke('setup__get_current_ai_service'),
			]);
			services = svcs;
			if (current) {
				currentModelLabel = current.label;
				selectedModelIri = current.iri;
				loadedModelIri = current.iri;
				if (current.modelIdentifier) {
					selectedOpenRouterModelId = current.modelIdentifier;
					loadedOpenRouterModelId = current.modelIdentifier;
				}
			}
			if (currentSvc) {
				selectedServiceIri = currentSvc.iri;
				loadedServiceIri = currentSvc.iri;
			} else if (services.length > 0) {
				selectedServiceIri = services[0].iri;
				loadedServiceIri = '';
			}
			if (selectedServiceIri) {
				await loadModels(selectedServiceIri);
			}
		} catch {
			// ignore
		}
	}

	async function loadModels(serviceIri) {
		try {
			models = await invoke('setup__list_ai_models', { serviceIri });
		} catch {
			models = [];
		}
	}

	async function loadOpenRouterModels() {
		if (!apiKey.trim() || !baseUrl) return;
		try {
			openRouterModels = await invoke('ai__list_openrouter_models', { apiKey, baseUrl });
		} catch {
			openRouterModels = [];
		}
	}

	async function onServiceChange(e) {
		selectedServiceIri = typeof e === 'string' ? e : e.target.value;
		selectedModelIri = '';
		selectedOpenRouterModelId = '';
		loadedOpenRouterModelId = '';
		openRouterModels = [];
		fallbackModelIds = [];
		loadedFallbackModelIds = [];
		aiMessage = '';
		aiError = false;
		await Promise.all([loadModels(selectedServiceIri), loadApiKeyForService(selectedServiceIri)]);
		if (selectedServiceIsOpenRouter && apiKey.trim()) {
			await loadOpenRouterModels();
		}
	}

	async function saveAiConfig() {
		if (!aiCanSave) return;
		aiSaving = true;
		aiMessage = '';
		aiError = false;
		try {
			if (!selectedServiceIsLocal && apiKey !== loadedApiKey) {
				if (selectedServiceIsOpenRouter) {
					await invoke('ai__validate_provider_key', { apiKey, baseUrl });
				}
				await invoke('ai__save_api_key', { apiKey, serviceIri: selectedServiceIri });
				loadedApiKey = apiKey;
			}
			if (selectedServiceIsOpenRouter && baseUrl !== loadedBaseUrl) {
				await invoke('ai__save_service_base_url', { serviceIri: selectedServiceIri, baseUrl });
				loadedBaseUrl = baseUrl;
			}
			if (selectedServiceIsOpenRouter && JSON.stringify(fallbackModelIds) !== JSON.stringify(loadedFallbackModelIds)) {
				await invoke('ai__save_fallback_models', { serviceIri: selectedServiceIri, modelIds: fallbackModelIds });
				loadedFallbackModelIds = [...fallbackModelIds];
			}
			if (selectedServiceIsOpenRouter && selectedOpenRouterModelId) {
				const orModel = openRouterModels.find(m => m.id === selectedOpenRouterModelId);
				if (orModel) {
					selectedModelIri = await invoke('ai__ensure_openrouter_model', {
						serviceIri: selectedServiceIri,
						modelId: orModel.id,
						modelName: orModel.name,
						contextLength: orModel.contextLength,
						inputCostPerToken: orModel.inputCostPerToken,
						outputCostPerToken: orModel.outputCostPerToken,
						supportsTools: true,
					});
					loadedOpenRouterModelId = selectedOpenRouterModelId;
				}
			}
			if (selectedServiceIri !== loadedServiceIri) {
				await invoke('setup__save_ai_service', { serviceIri: selectedServiceIri });
				loadedServiceIri = selectedServiceIri;
			}
			if (selectedModelIri !== loadedModelIri) {
				await invoke('setup__save_ai_model', { modelIri: selectedModelIri });
				loadedModelIri = selectedModelIri;
			}
			const selected = models.find(m => m.iri === selectedModelIri);
			currentModelLabel = selected?.label ?? '';
			aiMessage = 'Configuração salva.';
		} catch (e) {
			aiMessage = String(e);
			aiError = true;
		} finally {
			aiSaving = false;
		}
	}

	async function loadLogPath() {
		try {
			logPath = await invoke('get_log_file_path_command');
		} catch {
			logPath = '';
		}
	}

	async function clearLogs() {
		logClearing = true;
		logMessage = '';
		logError = false;
		try {
			await invoke('clear_logs');
			logMessage = 'Logs cleared.';
		} catch (e) {
			logMessage = String(e);
			logError = true;
		} finally {
			logClearing = false;
		}
	}

	async function loadImapAccounts() {
		try {
			imapAccounts = await invoke('imap__get_accounts');
		} catch {
			imapAccounts = [];
		}
	}

	function startAddImapAccount() {
		imapEditing = 'new';
		imapHasExistingPassword = false;
		imapForm = { label: '', host: '', port: 993, use_tls: true, username: '', password: '', sync_interval_minutes: 15 };
		imapAvailableFolders = [];
		imapSelectedFolders = [];
		imapMessage = '';
		imapError = false;
	}

	function startEditImapAccount(account) {
		imapEditing = account.iri;
		imapHasExistingPassword = true;
		imapForm = {
			label: account.label,
			host: account.host,
			port: account.port,
			use_tls: account.use_tls,
			username: account.username,
			password: '',
			sync_interval_minutes: account.sync_interval_minutes,
		};
		imapAvailableFolders = account.monitored_folders ?? [];
		imapSelectedFolders = account.monitored_folders ?? [];
		imapMessage = '';
		imapError = false;
	}

	function cancelImapEdit() {
		imapEditing = null;
		imapHasExistingPassword = false;
		imapAvailableFolders = [];
		imapSelectedFolders = [];
		imapMessage = '';
		imapError = false;
	}

	function toggleImapFolder(folder) {
		if (imapSelectedFolders.includes(folder)) {
			imapSelectedFolders = imapSelectedFolders.filter(f => f !== folder);
		} else {
			imapSelectedFolders = [...imapSelectedFolders, folder];
		}
	}

	async function testImapConnection() {
		imapTesting = true;
		imapLoadingFolders = false;
		imapMessage = '';
		imapError = false;
		try {
			const msg = await invoke('imap__test_connection', {
				host: imapForm.host,
				port: imapForm.port,
				useTls: imapForm.use_tls,
				username: imapForm.username,
				password: imapForm.password,
			});
			imapMessage = msg;
			imapLoadingFolders = true;
			try {
				imapAvailableFolders = await invoke('imap__list_folders', {
					host: imapForm.host,
					port: imapForm.port,
					useTls: imapForm.use_tls,
					username: imapForm.username,
					password: imapForm.password,
				});
				if (imapSelectedFolders.length === 0) {
					imapSelectedFolders = imapAvailableFolders.filter(f => f === 'INBOX');
				}
			} catch {
				// ignore folder listing error
			} finally {
				imapLoadingFolders = false;
			}
		} catch (e) {
			imapMessage = String(e);
			imapError = true;
		} finally {
			imapTesting = false;
		}
	}

	async function saveImapAccount() {
		imapSaving = true;
		imapMessage = '';
		imapError = false;
		try {
			const savedIri = await invoke('imap__save_account', {
				accountIri: imapEditing === 'new' ? null : imapEditing,
				input: {
					label: imapForm.label,
					host: imapForm.host,
					port: imapForm.port,
					use_tls: imapForm.use_tls,
					username: imapForm.username,
					password: imapForm.password,
					sync_interval_minutes: imapForm.sync_interval_minutes,
				},
			});
			if (imapSelectedFolders.length > 0) {
				await invoke('imap__save_monitored_folders', {
					accountIri: savedIri,
					folders: imapSelectedFolders,
				});
			}
			imapMessage = 'Conta salva.';
			imapEditing = null;
			imapAvailableFolders = [];
			imapSelectedFolders = [];
			await loadImapAccounts();
			invoke('imap__start_account_sync', { accountIri: savedIri });
		} catch (e) {
			imapMessage = String(e);
			imapError = true;
		} finally {
			imapSaving = false;
		}
	}

	async function toggleImapHistory(accountIri) {
		if (imapHistoryAccountIri === accountIri) {
			imapHistoryAccountIri = null;
			imapSyncHistory = [];
			return;
		}
		imapHistoryAccountIri = accountIri;
		try {
			imapSyncHistory = await invoke('imap__get_sync_history', { accountIri, limit: 20 });
		} catch {
			imapSyncHistory = [];
		}
	}

	async function deleteImapAccount(iri) {
		imapDeleting = true;
		imapMessage = '';
		imapError = false;
		try {
			await invoke('imap__delete_account', { accountIri: iri });
			if (imapEditing === iri) imapEditing = null;
			await loadImapAccounts();
		} catch (e) {
			imapMessage = String(e);
			imapError = true;
		} finally {
			imapDeleting = false;
		}
	}

	let claudeFilePath = $state('');

	async function connectClaudeDesktop() {
		claudeConnecting = true;
		claudeMessage = '';
		claudeFilePath = '';
		claudeError = false;
		try {
			claudeFilePath = await invoke('settings__connect_claude_desktop');
		} catch (e) {
			claudeMessage = String(e);
			claudeError = true;
		} finally {
			claudeConnecting = false;
		}
	}

	function handleBackdropClick(e) {
		if (e.target === e.currentTarget) onClose?.();
	}

	function handleKeydown(e) {
		if (e.key === 'Escape') onClose?.();
	}
</script>

<svelte:window onkeydown={handleKeydown} />

<div class="backdrop" role="presentation" onclick={handleBackdropClick}>
	<div class="panel" role="dialog" aria-modal="true" aria-label="Settings">
		<div class="panel-header">
			<span class="panel-title">Settings</span>
			<Button variant="ghost" size="icon" title="Close" onclick={onClose}><span class="material-symbols-outlined">close</span></Button>
		</div>

		<div class="panel-body">
			<section class="settings-section">
				<h3 class="section-title">
					<span class="material-symbols-outlined">psychology</span>
					AI Model
				</h3>
				{#if services.length > 0}
					<label class="field-label" for="settings-provider-select">Provedor</label>
					<div class="field-row">
						<Select.Root
							type="single"
							value={selectedServiceIri}
							onValueChange={async (v) => { if (v) await onServiceChange(v); }}
						>
							<Select.Trigger id="settings-provider-select" class="flex-1">
								<Select.Value placeholder="Selecionar provedor" />
							</Select.Trigger>
							<Select.Content>
								{#each services as svc}
									<Select.Item value={svc.iri}>{svc.label}</Select.Item>
								{/each}
							</Select.Content>
						</Select.Root>
					</div>

					{#if !selectedServiceIsLocal}
						<label class="field-label" for="settings-api-key-input">Chave de API</label>
						{#if apiKeyLoading}
							<p class="hint">Loading…</p>
						{:else}
							<div class="field-row">
								<Input
									id="settings-api-key-input"
									type="password"
									placeholder={selectedServiceIsOpenRouter ? 'sk-or-…' : 'sk-ant-…'}
									bind:value={apiKey}
								/>
								<Button variant="destructive" size="icon"
									title="Remover chave de API"
									onclick={deleteApiKey}
									disabled={apiKeyDeleting || !loadedApiKey}
								><span class="material-symbols-outlined">delete</span></Button>
							</div>
						{/if}
					{/if}

					{#if selectedServiceIsOpenRouter}
						<label class="field-label" for="settings-base-url-input">URL Base</label>
						<div class="field-row">
							<Input
								id="settings-base-url-input"
								type="url"
								placeholder="https://openrouter.ai/api/v1"
								bind:value={baseUrl}
							/>
						</div>
					{/if}

					{#if selectedServiceIsOpenRouter && openRouterModels.length > 0}
						<label class="field-label" for="settings-or-model-select">Modelo (OpenRouter)</label>
						<div class="field-row">
							<Select.Root
								type="single"
								value={selectedOpenRouterModelId}
								onValueChange={(v) => { if (v !== undefined) selectedOpenRouterModelId = v; }}
							>
								<Select.Trigger id="settings-or-model-select" class="flex-1">
									<Select.Value placeholder="Selecione um modelo…" />
								</Select.Trigger>
								<Select.Content>
									<Select.Item value="">Selecione um modelo…</Select.Item>
									{#each openRouterModels as m}
										<Select.Item value={m.id}>{m.name} — {(m.contextLength / 1000).toFixed(0)}k ctx · in ${(m.inputCostPerToken * 1_000_000).toFixed(2)} / out ${(m.outputCostPerToken * 1_000_000).toFixed(2)} por 1M</Select.Item>
									{/each}
								</Select.Content>
							</Select.Root>
						</div>
					{:else if selectedServiceIsOpenRouter && apiKey.trim() && openRouterModels.length === 0}
						<div class="field-row">
							<Button variant="default" onclick={loadOpenRouterModels}>Carregar modelos</Button>
						</div>
					{/if}

					{#if selectedServiceIsOpenRouter && openRouterModels.length > 0}
						<label class="field-label" for="settings-fallback-add">Modelos de Fallback (em ordem de prioridade)</label>
						<div class="field-row" style="flex-direction:column;gap:4px;align-items:flex-start;">
							{#each fallbackModelIds as fbId, i}
								<div class="field-row" style="width:100%;">
									<span class="text-input" style="flex:1;padding:4px 8px;font-size:0.85em;">{fbId}</span>
									<Button variant="ghost" size="icon" title="Remover" onclick={() => { fallbackModelIds = fallbackModelIds.filter((_, idx) => idx !== i); }}><span class="material-symbols-outlined">remove</span></Button>
								</div>
							{/each}
							<div class="field-row" style="width:100%;">
								<Select.Root
									type="single"
									value=""
									onValueChange={(v) => {
										if (v && !fallbackModelIds.includes(v)) {
											fallbackModelIds = [...fallbackModelIds, v];
										}
									}}
								>
									<Select.Trigger id="settings-fallback-add" class="flex-1">
										<Select.Value placeholder="Adicionar modelo de fallback…" />
									</Select.Trigger>
									<Select.Content>
										{#each openRouterModels.filter(m => !fallbackModelIds.includes(m.id)) as m}
											<Select.Item value={m.id}>{m.name}</Select.Item>
										{/each}
									</Select.Content>
								</Select.Root>
							</div>
						</div>
					{/if}

					{#if !selectedServiceIsOpenRouter && models.length > 0}
						<label class="field-label" for="settings-model-select">Modelo padrão</label>
						<div class="field-row">
							<Select.Root
								type="single"
								value={selectedModelIri}
								onValueChange={(v) => { if (v !== undefined) selectedModelIri = v; }}
							>
								<Select.Trigger id="settings-model-select" class="flex-1">
									<Select.Value placeholder="Selecione um modelo…" />
								</Select.Trigger>
								<Select.Content>
									{#each models as model}
										<Select.Item value={model.iri}>{model.label}</Select.Item>
									{/each}
								</Select.Content>
							</Select.Root>
						</div>
						{#if !selectedServiceIsLocal && !apiKey.trim()}
							<p class="feedback error">Informe a chave de API antes de ativar este provedor.</p>
						{/if}
					{/if}

					<div class="field-row">
						<Button variant="default" onclick={saveAiConfig} disabled={aiSaving || !aiCanSave}>
							{aiSaving ? 'Salvando…' : 'Salvar'}
						</Button>
					</div>

					{#if aiMessage}
						<p class="feedback" class:error={aiError}>{aiMessage}</p>
					{/if}
				{:else}
					<p class="hint">No AI services found.</p>
				{/if}
			</section>

			{#if selectedServiceIsLocal}
			<section class="settings-section">
				<h3 class="section-title">
					<span class="material-symbols-outlined">save</span>
					Modelo Local
				</h3>
				{#if localModelExists}
					<p class="hint">
						<span class="material-symbols-outlined model-status-icon installed">check_circle</span>
						Instalado · {localModelSizeHuman}
					</p>
				{:else if localModelIsDownloading}
					<p class="hint">Baixando… {localModelProgress.toFixed(1)}%</p>
					<div class="progress-bar-track">
						<div class="progress-bar-fill" style="width: {localModelProgress}%"></div>
					</div>
					<p class="progress-detail">{formatBytes(localModelDownloadedBytes)} / {formatBytes(localModelTotalBytes)}</p>
					<div class="field-row">
						<Button variant="destructive" onclick={cancelModelDownload}>Cancelar</Button>
					</div>
				{:else}
					<p class="hint">Modelo não instalado · {localModelSizeHuman}</p>
					<div class="field-row">
						<Button variant="default" onclick={startModelDownload}><span class="material-symbols-outlined">download</span>Baixar modelo</Button>
					</div>
				{/if}
				{#if localModelMessage}
					<p class="feedback" class:error={localModelError}>{localModelMessage}</p>
				{/if}
			</section>
			{/if}

			<section class="settings-section">
				<h3 class="section-title">
					<span class="material-symbols-outlined">mail</span>
					Contas de Email (IMAP)
				</h3>

				{#each imapAccounts as account}
					<div class="imap-account-row">
						<span class="material-symbols-outlined imap-status-icon" class:connected={account.is_connected}>
							{account.is_connected ? 'check_circle' : 'radio_button_unchecked'}
						</span>
						<div class="imap-account-info">
							<span class="imap-account-label">{account.label}</span>
							<span class="imap-account-host">{account.username}@{account.host}</span>
						</div>
						<Button variant="ghost" size="icon-sm" title="Histórico de sincronização" onclick={() => toggleImapHistory(account.iri)}><span class="material-symbols-outlined">history</span></Button>
						<Button variant="ghost" size="icon-sm" title="Editar" onclick={() => startEditImapAccount(account)}><span class="material-symbols-outlined">edit</span></Button>
						<Button variant="destructive" size="icon-sm" title="Remover" disabled={imapDeleting} onclick={() => deleteImapAccount(account.iri)}><span class="material-symbols-outlined">delete</span></Button>
					</div>
					{#if imapHistoryAccountIri === account.iri}
						<div class="sync-history">
							{#if imapSyncHistory.length === 0}
								<p class="hint">Nenhuma sincronização registrada.</p>
							{:else}
								<table class="sync-table">
									<thead>
										<tr>
											<th>Data</th>
											<th>Importados</th>
											<th>Status</th>
										</tr>
									</thead>
									<tbody>
										{#each imapSyncHistory as entry}
											<tr>
												<td>{entry.started_at.replace('T', ' ').replace('Z', '')}</td>
												<td>{entry.emails_imported}</td>
												<td class:sync-error={!!entry.error}>{entry.error ?? 'OK'}</td>
											</tr>
										{/each}
									</tbody>
								</table>
							{/if}
						</div>
					{/if}
				{/each}

				{#if imapMessage && imapEditing === null}
					<p class="feedback" class:error={imapError}>{imapMessage}</p>
				{/if}

				<div class="field-row">
					<Button variant="default" onclick={startAddImapAccount}><span class="material-symbols-outlined">add</span>Adicionar conta</Button>
				</div>
			</section>

			<Dialog.Root open={imapEditing !== null} onOpenChange={(o) => { if (!o) cancelImapEdit(); }}>
				<Dialog.Content class="sm:max-w-[480px]">
					<Dialog.Header>
						<Dialog.Title>
							<span class="material-symbols-outlined" style="vertical-align: middle; font-size: 18px; margin-right: 6px;">mail</span>
							{imapEditing === 'new' ? 'Nova conta de email' : 'Editar conta de email'}
						</Dialog.Title>
					</Dialog.Header>

					<div class="imap-form-body">
						<div class="form-field">
							<label class="field-label" for="imap-label">Rótulo</label>
							<Input id="imap-label" type="text" placeholder="Gmail Pessoal" bind:value={imapForm.label} />
						</div>

						<div class="form-grid">
							<div class="form-field" style="flex:3; min-width:0">
								<label class="field-label" for="imap-host">Servidor IMAP</label>
								<Input id="imap-host" type="text" placeholder="imap.gmail.com" bind:value={imapForm.host} />
							</div>
							<div class="form-field" style="flex:1; min-width:80px">
								<label class="field-label" for="imap-port">Porta</label>
								<Input id="imap-port" type="number" placeholder="993" bind:value={imapForm.port} min="1" max="65535" />
							</div>
						</div>

						<div class="form-field">
							<label class="field-label" for="imap-username">Usuário</label>
							<Input id="imap-username" type="text" placeholder="seu@email.com" bind:value={imapForm.username} autocomplete="off" />
						</div>

						<div class="form-field">
							<label class="field-label" for="imap-password">Senha</label>
							<Input
								id="imap-password"
								type="password"
								placeholder={imapHasExistingPassword ? '••••••••' : 'Senha de aplicativo'}
								bind:value={imapForm.password}
								autocomplete="new-password"
							/>
							{#if imapHasExistingPassword}
								<p class="hint">Deixe em branco para manter a senha salva.</p>
							{/if}
						</div>

						<div class="field-row imap-options-row">
							<label class="checkbox-label">
								<Checkbox bind:checked={imapForm.use_tls} />
								Usar TLS
							</label>
							<label class="checkbox-label" style="margin-left:auto">
								Sincronizar a cada
								<Input class="interval-input" type="number" bind:value={imapForm.sync_interval_minutes} min="1" max="1440" />
								min
							</label>
						</div>

						{#if imapAvailableFolders.length > 0}
							<div class="form-field">
								<span class="field-label">Pastas monitoradas</span>
								<div class="imap-folders">
									{#each imapAvailableFolders as folder}
										<label class="checkbox-label folder-label">
											<Checkbox checked={imapSelectedFolders.includes(folder)} onCheckedChange={() => toggleImapFolder(folder)} />
											{folder}
										</label>
									{/each}
								</div>
							</div>
						{:else if imapLoadingFolders}
							<p class="hint">Carregando pastas…</p>
						{/if}

						{#if imapMessage}
							<p class="feedback" class:error={imapError}>{imapMessage}</p>
						{/if}
					</div>

					<Dialog.Footer>
						<Button variant="ghost" onclick={cancelImapEdit}>Cancelar</Button>
						<Button variant="default" onclick={testImapConnection} disabled={imapTesting || !imapForm.host || !imapForm.username || !imapForm.password}>
							{imapTesting ? 'Testando…' : 'Testar conexão'}
						</Button>
						<Button variant="default" onclick={saveImapAccount} disabled={imapSaving || !imapForm.label || !imapForm.host || !imapForm.username || (!imapHasExistingPassword && !imapForm.password)}>
							{imapSaving ? 'Salvando…' : 'Salvar'}
						</Button>
					</Dialog.Footer>
				</Dialog.Content>
			</Dialog.Root>

			<section class="settings-section">
				<h3 class="section-title">
					<span class="material-symbols-outlined">folder_open</span>
					Pasta de Dados
				</h3>
				<p class="hint">Pasta onde o banco de dados do Foundation é armazenado. O app será reiniciado ao salvar.</p>
				<div class="field-row">
					<Input
						type="text"
						placeholder="/caminho/para/pasta"
						bind:value={foundationDir}
					/>
					<Button variant="ghost" size="icon" title="Selecionar pasta" onclick={browseFoundationDir}><span class="material-symbols-outlined">folder_open</span></Button>
				</div>
				<div class="field-row">
					<Button variant="default"
						onclick={saveFoundationDir}
						disabled={foundationDirSaving || !foundationDir.trim()}
					><span class="material-symbols-outlined">restart_alt</span>{foundationDirSaving ? 'Salvando…' : 'Salvar e reiniciar'}</Button>
				</div>
				{#if foundationDirMessage}
					<p class="feedback" class:error={foundationDirError}>{foundationDirMessage}</p>
				{/if}
			</section>

			<section class="settings-section">
				<h3 class="section-title">
					<span class="material-symbols-outlined">extension</span>
					Claude Desktop
				</h3>
				<p class="hint">Instala uma extensão no Claude Desktop que expõe as ferramentas do Foundation. O app Foundation precisa estar aberto para a extensão funcionar.</p>
				<div class="field-row">
					<Button variant="default" onclick={connectClaudeDesktop} disabled={claudeConnecting}><span class="material-symbols-outlined">download</span>{claudeConnecting ? 'Preparando…' : 'Preparar instalador'}</Button>
				</div>
				{#if claudeFilePath}
					<div class="claude-instructions">
						<p class="hint"><strong>Arquivo pronto em</strong></p>
						<p class="log-path">{claudeFilePath}</p>
						<p class="hint" style="margin-top:8px"><strong>Para instalar:</strong></p>
						<ol class="claude-steps">
							<li>Abra o Claude Desktop.</li>
							<li>Vá em <em>Configurações → Extensões</em>.</li>
							<li>Arraste o arquivo <code>foundation.mcpb</code> da janela do Explorer para dentro da janela de Extensões.</li>
							<li>Siga as instruções do diálogo de instalação. As ferramentas do Foundation aparecem em seguida.</li>
						</ol>
					</div>
				{/if}
				{#if claudeMessage}
					<p class="feedback" class:error={claudeError}>{claudeMessage}</p>
				{/if}
			</section>

			<section class="settings-section">
				<h3 class="section-title">
					<span class="material-symbols-outlined">shield</span>
					Privacidade
				</h3>
				<label class="analytics-toggle">
					<Checkbox
						checked={analyticsEnabled}
						onCheckedChange={(v) => toggleAnalytics(!!v)}
					/>
					<span class="analytics-toggle-text">
						<strong>Compartilhar dados de uso anônimos</strong>
						<span>Ajuda a melhorar o FOUNDATION. Nenhuma informação pessoal ou conteúdo de e-mail é enviado.</span>
					</span>
				</label>
			</section>

			<section class="settings-section">
				<h3 class="section-title">
					<span class="material-symbols-outlined">developer_mode</span>
					Desenvolvimento
				</h3>
				<p class="hint">Ferramentas para teste e depuração.</p>
				<div class="field-row">
					<Button variant="default"
						onclick={async () => { await invoke('setup__reset'); }}
					><span class="material-symbols-outlined">refresh</span>Refazer setup inicial</Button>
				</div>
			</section>

			<section class="settings-section">
				<h3 class="section-title">
					<span class="material-symbols-outlined">description</span>
					Logs
				</h3>
				{#if logPath}
					<p class="log-path">{logPath}</p>
				{/if}
				<div class="field-row">
					<Button variant="destructive" onclick={clearLogs} disabled={logClearing}>
						{logClearing ? 'Clearing…' : 'Clear logs'}
					</Button>
				</div>
				{#if logMessage}
					<p class="feedback" class:error={logError}>{logMessage}</p>
				{/if}
			</section>
		</div>
	</div>
</div>

<style>
	.backdrop {
		position: fixed;
		inset: 0;
		z-index: 200;
		background: rgba(0, 0, 0, 0.45);
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.panel {
		background: var(--color-surface-1);
		border-radius: var(--radius);
		width: 920px;
		max-width: 95vw;
		max-height: 90vh;
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.panel-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 16px 18px 12px;
		flex-shrink: 0;
	}

	.panel-title {
		font-size: 15px;
		font-weight: 600;
		color: var(--color-neutral-active);
	}

	.panel-body {
		overflow-x: hidden;
		overflow-y: auto;
		padding: 18px;
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 24px 28px;
		align-content: start;
	}

	@media (max-width: 760px) {
		.panel-body {
			grid-template-columns: 1fr;
		}
	}

	.settings-section {
		display: flex;
		flex-direction: column;
		gap: 10px;
		min-width: 0;
	}

	/* .section-title and .field-label come from global base.css */

	.hint {
		font-size: 14px;
		color: var(--color-neutral);
		margin: 0;
	}

	.hint strong {
		color: var(--color-neutral-active);
	}

	.log-path {
		font-size: 11px;
		color: var(--color-neutral);
		word-break: break-all;
		margin: 0;
		font-family: monospace;
	}

	.field-row {
		display: flex;
		gap: 8px;
		align-items: center;
	}

	.text-input {
		flex: 1;
		min-width: 0;
		background: var(--color-surface-3);
		border: none;
		border-radius: var(--radius);
		color: var(--color-neutral-active);
		font-family: var(--font-body);
		font-size: 14px;
		padding: 7px 10px;
		line-height: 1.4;
		outline: none;
		box-sizing: border-box;
	}

	/* save-btn / danger-btn replaced by <Button> component */

	.feedback {
		font-size: 12px;
		color: var(--color-interactive);
		margin: 0;
	}

	.feedback.error {
		color: var(--color-danger);
	}

	.model-status-icon {
		font-size: 14px;
		vertical-align: middle;
		margin-right: 4px;
	}

	.model-status-icon.installed {
		color: var(--color-success);
	}

	.progress-bar-track {
		height: 4px;
		background: color-mix(in srgb, var(--color-white) 10%, transparent);
		width: 100%;
	}

	.progress-bar-fill {
		height: 100%;
		background: var(--color-interactive);
		transition: width 0.25s linear;
	}

	.progress-detail {
		font-size: 11px;
		color: var(--color-neutral);
		margin: 0;
	}

	.imap-account-row {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 12px;
		background: var(--color-surface-2);
		border-radius: var(--radius);
		margin-bottom: 6px;
	}

	.imap-status-icon {
		font-size: 16px;
		color: var(--color-neutral);
		flex-shrink: 0;
	}

	.imap-status-icon.connected {
		color: var(--color-success);
	}

	.sync-history {
		padding: 8px 0 4px 28px;
	}

	.sync-table {
		width: 100%;
		border-collapse: collapse;
		font-size: 12px;
	}

	.sync-table th, .sync-table td {
		text-align: left;
		padding: 3px 8px;
		color: var(--color-neutral);
	}

	.sync-table th {
		font-weight: 600;
		color: var(--color-neutral);
	}

	.sync-table .sync-error {
		color: var(--color-error);
		max-width: 300px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.imap-account-info {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}

	.imap-account-label {
		font-size: 13px;
		color: var(--color-neutral-active);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.imap-account-host {
		font-size: 11px;
		color: var(--color-neutral);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.form-field {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.form-grid {
		display: flex;
		gap: 10px;
	}

	.imap-options-row {
		flex-wrap: wrap;
		gap: 12px;
	}

	.checkbox-label {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 13px;
		color: var(--color-neutral);
		cursor: pointer;
	}

	:global([data-slot="input"].interval-input) {
		flex: none;
		width: 52px;
		display: inline-block;
		padding: 4px 6px;
		height: auto;
	}

	.imap-folders {
		display: flex;
		flex-direction: column;
		gap: 4px;
		max-height: 160px;
		overflow-y: auto;
		padding: 6px 0;
	}

	.folder-label {
		font-size: 12px;
	}

	.claude-instructions {
		display: flex;
		flex-direction: column;
		gap: 4px;
		padding: 10px 12px;
		background: var(--color-surface-2);
		border-radius: var(--radius);
	}

	.claude-steps {
		margin: 4px 0 0;
		padding-left: 20px;
		font-size: 12px;
		color: var(--color-neutral);
		line-height: 1.6;
	}

	.claude-steps em {
		font-style: normal;
		color: var(--color-neutral-active);
	}

	.claude-steps code {
		font-family: monospace;
		background: var(--color-surface-3);
		padding: 0 4px;
		border-radius: var(--radius);
	}

	.analytics-toggle {
		display: flex;
		align-items: flex-start;
		gap: 10px;
		padding: 10px 12px;
		background: var(--color-surface-2);
		border-radius: var(--radius);
		cursor: pointer;
	}

	.analytics-toggle :global([data-slot="checkbox"]) {
		margin-top: 2px;
		flex-shrink: 0;
	}

	.analytics-toggle-text {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.analytics-toggle-text strong {
		font-size: 13px;
		color: var(--color-neutral-active);
		font-weight: 500;
	}

	.analytics-toggle-text span {
		font-size: 12px;
		color: var(--color-neutral);
		line-height: 1.5;
	}

	.imap-form-body {
		display: flex;
		flex-direction: column;
		gap: 12px;
		overflow-y: auto;
		max-height: 60vh;
	}
</style>
