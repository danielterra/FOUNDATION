<script>
	import { invoke } from '@tauri-apps/api/core';
	import { Button } from '$lib/components/ui/button';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Input } from '$lib/components/ui/input';
	import { Checkbox } from '$lib/components/ui/checkbox';
	import * as Select from '$lib/components/ui/select';
	import OpenRouterModelSelector from '$lib/components/OpenRouterModelSelector.svelte';
	import posthog from 'posthog-js';

	let { onClose } = $props();
	const settingsTabs = [
		{ id: 'ai', panelId: 'settings-panel-ai', label: 'IA e modelos', icon: 'psychology' },
		{ id: 'email', panelId: 'settings-panel-email', label: 'E-mail', icon: 'mail' },
		{ id: 'data', panelId: 'settings-panel-data', label: 'Dados', icon: 'database' },
		{ id: 'integrations', panelId: 'settings-panel-integrations', label: 'Integrações', icon: 'extension' },
		{ id: 'privacy', panelId: 'settings-panel-privacy', label: 'Privacidade', icon: 'shield' },
		{ id: 'advanced', panelId: 'settings-panel-advanced', label: 'Avançado', icon: 'tune' },
	];
	let activeSettingsTab = $state('ai');

	// --- AI Configuration (Provedor + Chave de API + Modelo) ---
	let services = $state([]);
	let models = $state([]);
	let aiServicesLoading = $state(true);
	let aiServicesLoadError = $state(false);
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
	let openRouterModelsLoading = $state(false);
	let openRouterModelsError = $state('');
	let openRouterBenchmarks = $state(null);
	let openRouterBenchmarksLoading = $state(false);
	let openRouterBenchmarksError = $state('');
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

	function formatOpenRouterPrice(price) {
		return price == null ? 'não informado' : `$${(price * 1_000_000).toFixed(2)} por 1M`;
	}

	function openRouterProvider(model) {
		return model.publisher?.trim() || 'Publicador não informado';
	}

	function formatOpenRouterSize(model) {
		if (model.parameterCountStatus !== 'reported' || model.parameterCountTotal == null) return 'Tamanho não informado pelo provedor';
		const total = `${(model.parameterCountTotal / 1_000_000_000).toLocaleString('pt-BR', { maximumFractionDigits: 1 })}B parâmetros`;
		return model.parameterCountActive == null
			? total
			: `${total} · ${(model.parameterCountActive / 1_000_000_000).toLocaleString('pt-BR', { maximumFractionDigits: 1 })}B ativos`;
	}

	function openRouterModelLabel(model) {
		return `${model.name} · ${openRouterProvider(model)} · ${formatOpenRouterSize(model)} · entrada ${formatOpenRouterPrice(model.inputCostPerToken)} · saída ${formatOpenRouterPrice(model.outputCostPerToken)}`;
	}

	let selectedOpenRouterModel = $derived(openRouterModels.find((model) => model.id === selectedOpenRouterModelId) ?? null);

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

	// --- History Retention ---
	let historyRetentionDays = $state(0);
	let loadedHistoryRetentionDays = $state(0);
	let historyRetentionLoading = $state(false);
	let historyRetentionLoadError = $state(false);
	let historyRetentionSaving = $state(false);
	let historyRetentionMessage = $state('');
	let historyRetentionError = $state(false);

	let historyRetentionInputValue = $state('');

	let historyRetentionValid = $derived(
		historyRetentionInputValue !== ''
		&& Number.isInteger(Number(historyRetentionInputValue))
		&& Number(historyRetentionInputValue) >= 0
	);
	let historyRetentionDirty = $derived(
		historyRetentionValid && Number(historyRetentionInputValue) !== loadedHistoryRetentionDays
	);
	let historyRetentionCanSave = $derived(historyRetentionValid && historyRetentionDirty);

	// --- IMAP ---
	let imapAccounts = $state([]);
	let imapAccountsLoading = $state(true);
	let imapAccountsLoadError = $state(false);
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
		await Promise.all([loadModelData(), loadLogPath(), loadLocalModelStatus(), loadImapAccounts(), loadFoundationDir(), loadHistoryRetention()]);
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

	async function loadHistoryRetention() {
		historyRetentionLoading = true;
		historyRetentionLoadError = false;
		historyRetentionMessage = '';
		try {
			const days = await invoke('settings__get_history_retention_days');
			historyRetentionDays = days;
			loadedHistoryRetentionDays = days;
			historyRetentionInputValue = String(days);
		} catch {
			historyRetentionLoadError = true;
		} finally {
			historyRetentionLoading = false;
		}
	}

	async function saveHistoryRetention() {
		if (!historyRetentionCanSave) return;
		historyRetentionSaving = true;
		historyRetentionMessage = '';
		historyRetentionError = false;
		try {
			const days = Number(historyRetentionInputValue);
			await invoke('settings__set_history_retention_days', { days });
			loadedHistoryRetentionDays = days;
			historyRetentionDays = days;
			historyRetentionMessage = 'Configuração salva.';
		} catch (e) {
			historyRetentionMessage = String(e);
			historyRetentionError = true;
		} finally {
			historyRetentionSaving = false;
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
		aiServicesLoading = true;
		aiServicesLoadError = false;
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
			aiServicesLoadError = true;
		} finally {
			aiServicesLoading = false;
		}
	}

	async function loadModels(serviceIri) {
		try {
			const result = await invoke('setup__list_ai_models', { serviceIri });
			models = result.items;
		} catch {
			models = [];
		}
	}

	async function loadOpenRouterModels() {
		if (!apiKey.trim() || !baseUrl.trim()) return;
		openRouterModelsLoading = true;
		openRouterModelsError = '';
		try {
			openRouterModels = await invoke('ai__list_openrouter_models', { apiKey, baseUrl });
			await loadOpenRouterBenchmarks();
			if (!openRouterModels.length) openRouterModelsError = 'Nenhum modelo foi retornado pelo OpenRouter.';
		} catch (error) {
			openRouterModelsError = String(error);
		} finally {
			openRouterModelsLoading = false;
		}
	}

	async function loadOpenRouterBenchmarks() {
		openRouterBenchmarksLoading = true;
		openRouterBenchmarksError = '';
		try {
			openRouterBenchmarks = await invoke('ai__list_openrouter_benchmarks', { apiKey, baseUrl });
		} catch (error) {
			openRouterBenchmarks = null;
			openRouterBenchmarksError = String(error);
		} finally {
			openRouterBenchmarksLoading = false;
		}
	}

	async function onServiceChange(e) {
		selectedServiceIri = typeof e === 'string' ? e : e.target.value;
		selectedModelIri = '';
		selectedOpenRouterModelId = '';
		loadedOpenRouterModelId = '';
		openRouterModels = [];
		openRouterModelsError = '';
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
						supportsTools: orModel.supportsTools ?? false,
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
		imapAccountsLoading = true;
		imapAccountsLoadError = false;
		try {
			imapAccounts = await invoke('imap__get_accounts');
		} catch {
			imapAccountsLoadError = true;
		} finally {
			imapAccountsLoading = false;
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
			const syncResult = await invoke('imap__get_sync_history', { accountIri, limit: 20 });
			imapSyncHistory = syncResult.items;
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

	function handleKeydown(e) {
		if (e.key === 'Escape' && imapEditing === null) onClose?.();
	}

	function handleSettingsSectionClick(event) {
		activeSettingsTab = event.currentTarget.dataset.settingsSection;
	}

	function handleSettingsSectionKeydown(event) {
		const currentSection = event.target.closest('[data-settings-section]');
		if (!currentSection) return;

		let currentIndex = -1;
		for (let index = 0; index < settingsTabs.length; index += 1) {
			if (settingsTabs[index].id === currentSection.dataset.settingsSection) {
				currentIndex = index;
				break;
			}
		}
		if (currentIndex < 0) return;

		let nextIndex = currentIndex;
		if (event.key === 'ArrowDown') nextIndex = (currentIndex + 1) % settingsTabs.length;
		else if (event.key === 'ArrowUp') nextIndex = (currentIndex - 1 + settingsTabs.length) % settingsTabs.length;
		else if (event.key === 'Home') nextIndex = 0;
		else if (event.key === 'End') nextIndex = settingsTabs.length - 1;
		else return;

		event.preventDefault();
		activeSettingsTab = settingsTabs[nextIndex].id;
		document.getElementById(`settings-nav-${settingsTabs[nextIndex].id}`)?.focus();
	}
</script>

<svelte:window onkeydown={handleKeydown} />

<section class="settings-screen" aria-label="Configurações">
	<header class="panel-header">
		<div class="panel-heading">
			<p class="panel-eyebrow">SISTEMA</p>
			<h1 class="panel-title">Configurações</h1>
		</div>
		<Button variant="ghost" size="default" onclick={onClose} aria-label="Voltar à lousa"><span class="material-symbols-outlined" aria-hidden="true">arrow_back</span> Voltar à lousa</Button>
	</header>
	<div class="settings-layout">
		<nav class="settings-navigation" aria-label="Seções de configuração">
			{#each settingsTabs as tab}
				<button
					id="settings-nav-{tab.id}"
					class="settings-nav-item"
					class:active={activeSettingsTab === tab.id}
					data-settings-section={tab.id}
					aria-current={activeSettingsTab === tab.id ? 'page' : undefined}
					aria-controls={tab.panelId}
					onclick={handleSettingsSectionClick}
					onkeydown={handleSettingsSectionKeydown}
				>
					<span class="material-symbols-outlined" aria-hidden="true">{tab.icon}</span>
					<span>{tab.label}</span>
				</button>
			{/each}
		</nav>

		<main class="panel-body">
			<div class="settings-tabpanel settings-panel-grid" id="settings-panel-ai" aria-labelledby="settings-nav-ai" hidden={activeSettingsTab !== 'ai'}>
			<section class="settings-section">
				<h3 class="section-title">
					<span class="material-symbols-outlined">psychology</span>
					Provedor e modelo de IA
				</h3>
				{#if aiServicesLoading}
					<p class="hint" role="status" aria-live="polite">Carregando provedores e modelos…</p>
				{:else if aiServicesLoadError}
					<p class="feedback error" role="alert">Não foi possível carregar provedores e modelos.</p>
				{:else if services.length > 0}
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

					{#if selectedServiceIsOpenRouter}
						<OpenRouterModelSelector
							idPrefix="settings-openrouter"
							models={openRouterModels}
							benchmarks={openRouterBenchmarks}
							bind:selectedModelId={selectedOpenRouterModelId}
							loading={openRouterModelsLoading}
							error={openRouterModelsError}
							benchmarksLoading={openRouterBenchmarksLoading}
							benchmarksError={openRouterBenchmarksError}
							loadHint={!apiKey.trim() ? 'Informe a chave de API para carregar o catálogo.' : !baseUrl.trim() ? 'Informe a URL base do OpenRouter para carregar o catálogo.' : 'Carregue o catálogo para comparar os modelos disponíveis.'}
							onRetry={loadOpenRouterModels}
						/>
					{/if}

					{#if selectedServiceIsOpenRouter && openRouterModels.length > 0}
						<label class="field-label" for="settings-fallback-add">Modelos de Fallback (em ordem de prioridade)</label>
						<div class="field-row" style="flex-direction:column;gap:4px;align-items:flex-start;">
							{#each fallbackModelIds as fbId, i}
								<div class="field-row" style="width:100%;">
					<span class="text-input" style="flex:1;padding:4px 8px;font-size:0.85em;">{openRouterModels.find((model) => model.id === fbId) ? openRouterModelLabel(openRouterModels.find((model) => model.id === fbId)) : fbId}</span>
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
											<Select.Item value={m.id}>{openRouterModelLabel(m)}</Select.Item>
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
					<p class="hint">Nenhum provedor de IA disponível.</p>
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
			</div>

			<div class="settings-tabpanel settings-panel-grid" id="settings-panel-email" role="region" aria-labelledby="settings-nav-email" hidden={activeSettingsTab !== 'email'}>
			<section class="settings-section">
				<h3 class="section-title">
					<span class="material-symbols-outlined">mail</span>
					Contas de e-mail (IMAP)
				</h3>
				{#if imapAccountsLoading}
					<p class="hint" role="status" aria-live="polite">Carregando contas de e-mail…</p>
				{:else if imapAccountsLoadError}
					<p class="feedback error" role="alert">Não foi possível carregar as contas de e-mail.</p>
				{:else if imapAccounts.length === 0}
					<p class="hint">Nenhuma conta de e-mail configurada.</p>
				{/if}

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
			</div>

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

			<div class="settings-tabpanel settings-panel-grid" id="settings-panel-data" role="region" aria-labelledby="settings-nav-data" hidden={activeSettingsTab !== 'data'}>
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
					<span class="material-symbols-outlined">history</span>
					Histórico de Dados
				</h3>
				<p class="hint">Versões antigas dos seus dados são removidas automaticamente ao iniciar o app. Defina por quantos dias elas ficam preservadas.</p>
				{#if historyRetentionLoading}
					<p class="hint">Carregando…</p>
				{:else if historyRetentionLoadError}
					<p class="feedback error">Não foi possível carregar a configuração.</p>
				{:else}
					<label class="field-label" for="settings-history-retention">Período de retenção</label>
					<div class="field-row">
						<Input
							id="settings-history-retention"
							class="retention-input"
							type="number"
							min="0"
							step="1"
							inputmode="numeric"
							aria-describedby="settings-history-retention-helper"
							bind:value={historyRetentionInputValue}
						/>
						<span class="retention-suffix">dias</span>
					</div>
					{#if !historyRetentionValid && historyRetentionInputValue !== ''}
						<p class="feedback error" id="settings-history-retention-helper" aria-live="polite">
							Informe um número inteiro de dias (0 ou mais).
						</p>
					{:else}
						<p class="hint retention-helper" id="settings-history-retention-helper" aria-live="polite">
							{#if historyRetentionValid && Number(historyRetentionInputValue) === 0}
								0 = nunca apagar. Todo o histórico será preservado para sempre.
							{:else if historyRetentionValid}
								Versões com mais de {historyRetentionInputValue} dias serão removidas no próximo início do app.
							{/if}
						</p>
					{/if}
					<div class="field-row">
						<Button variant="default" onclick={saveHistoryRetention} disabled={historyRetentionSaving || !historyRetentionCanSave}>
							{historyRetentionSaving ? 'Salvando…' : 'Salvar'}
						</Button>
					</div>
					{#if historyRetentionMessage}
						<p class="feedback" class:error={historyRetentionError}>{historyRetentionMessage}</p>
					{/if}
				{/if}
			</section>
			</div>

			<div class="settings-tabpanel settings-panel-grid" id="settings-panel-integrations" role="region" aria-labelledby="settings-nav-integrations" hidden={activeSettingsTab !== 'integrations'}>
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
			</div>

			<div class="settings-tabpanel settings-panel-grid" id="settings-panel-privacy" role="region" aria-labelledby="settings-nav-privacy" hidden={activeSettingsTab !== 'privacy'}>
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
			</div>

			<div class="settings-tabpanel settings-panel-grid" id="settings-panel-advanced" role="region" aria-labelledby="settings-nav-advanced" hidden={activeSettingsTab !== 'advanced'}>
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
		</main>
	</div>
</section>

<style>
	.settings-screen {
		position: absolute;
		inset: 0;
		z-index: calc(var(--z-widget-max) + 2);
		display: flex;
		flex-direction: column;
		min-width: 0;
		min-height: 0;
		overflow: hidden;
		background: var(--color-surface-0);
	}

	.panel-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-4);
		padding: var(--space-4) var(--space-6);
		border-bottom: 1px solid var(--color-hud-line-subtle);
		flex-shrink: 0;
	}

	.panel-heading {
		min-width: 0;
	}

	.panel-eyebrow {
		margin: 0 0 var(--space-1);
		color: var(--text-accent);
		font-family: var(--font-mono);
		font-size: var(--type-label);
		letter-spacing: var(--tracking-hud-label);
	}

	.settings-layout {
		display: grid;
		grid-template-columns: minmax(12rem, 17rem) minmax(0, 1fr);
		flex: 1;
		min-width: 0;
		min-height: 0;
	}

	.settings-navigation {
		display: flex;
		flex-direction: column;
		gap: var(--space-1);
		padding: var(--space-4);
		border-right: 1px solid var(--color-hud-line-subtle);
		min-width: 0;
		overflow-y: auto;
	}

	.settings-nav-item {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		min-height: var(--size-touch-target);
		padding: var(--space-2) var(--space-3);
		border: 0;
		border-left: 2px solid transparent;
		border-radius: var(--radius);
		background: transparent;
		color: var(--color-neutral);
		font: inherit;
		text-align: left;
		cursor: pointer;
	}

	.settings-nav-item:hover {
		color: var(--color-neutral-active);
		background: color-mix(in srgb, var(--color-interactive) 10%, transparent);
	}

	.settings-nav-item.active {
		border-left-color: var(--color-selection);
		color: var(--color-interactive);
		background: color-mix(in srgb, var(--color-interactive) 9%, transparent);
	}

	.settings-nav-item:focus-visible {
		outline: 2px solid var(--color-selection);
		outline-offset: 2px;
	}

	.settings-nav-item .material-symbols-outlined {
		flex: 0 0 auto;
		font-size: var(--type-icon-medium);
	}

	.panel-title {
		margin: 0;
		font-size: var(--type-title);
		font-weight: 400;
		color: var(--color-neutral-active);
	}

	.panel-body {
		min-width: 0;
		min-height: 0;
		overflow: auto;
		padding: var(--space-6);
	}

	.settings-tabpanel[hidden] {
		display: none;
	}

	.settings-panel-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(min(100%, var(--size-settings-section-min-width)), 1fr));
		gap: var(--spacing-section);
		align-content: start;
	}

	@media (max-width: 760px) {
		.settings-layout {
			grid-template-columns: minmax(0, 1fr);
			grid-template-rows: auto minmax(0, 1fr);
		}

		.settings-navigation {
			flex-direction: column;
			border-right: 0;
			border-bottom: 1px solid var(--color-hud-line-subtle);
			padding: var(--space-2) var(--space-3);
			max-height: 32vh;
		}

		.settings-nav-item {
			flex: 0 0 auto;
			min-height: var(--size-touch-target);
			border-left: 0;
			border-bottom: 0;
		}

		.settings-nav-item.active {
			border-left: 2px solid var(--color-selection);
		}

		.panel-header,
		.panel-body {
			padding: var(--space-3);
		}

		.settings-panel-grid {
			grid-template-columns: minmax(0, 1fr);
		}
	}

	.settings-section {
		display: flex;
		flex-direction: column;
		gap: var(--spacing-field-block);
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

	:global([data-slot="input"].retention-input) {
		flex: none;
		width: 64px;
		display: inline-block;
		padding: 4px 6px;
		height: auto;
	}

	.retention-suffix {
		font-size: 13px;
		color: var(--color-neutral);
		white-space: nowrap;
	}

	.retention-helper {
		min-height: 1.4em;
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
