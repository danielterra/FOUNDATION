<script>
	import { invoke } from '@tauri-apps/api/core';
	import { Button } from '$lib/components/ui/button';
	import * as Dialog from '$lib/components/ui/dialog';
	import { Input } from '$lib/components/ui/input';
	import { Checkbox } from '$lib/components/ui/checkbox';
	import * as Select from '$lib/components/ui/select';
	import OpenRouterModelSelector from '$lib/components/OpenRouterModelSelector.svelte';
	import { aiServiceDisplayName } from '$lib/utils/aiServiceDisplayName';
	import openRouterLogo from '$lib/assets/providers/openrouter.svg';
	import anthropicLogo from '$lib/assets/providers/anthropic.svg';
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
	let modelPickerOpen = $state(false);
	let modelPickerDraftId = $state('');
	let modelPickerTrigger = $state(null);
	let fallbackPickerOpen = $state(false);
	let fallbackPickerDraftIds = $state([]);
	let fallbackPickerTrigger = $state(null);
	let fallbackPickerSaving = $state(false);
	let fallbackPickerError = $state('');
	let fallbackModelIds = $state([]);
	let loadedFallbackModelIds = $state([]);
	let serviceChangeGeneration = 0;
	let serviceConfigLoadGeneration = 0;
	let readinessRefreshGeneration = 0;

	let aiSaving = $state(false);
	let aiSavingStage = $state('');
	let aiMessage = $state('');
	let aiError = $state(false);
	let aiFieldStatus = $state({ provider: 'idle', apiKey: 'idle', baseUrl: 'idle', model: 'idle', fallback: 'idle' });
	let aiFieldError = $state({ provider: '', apiKey: '', baseUrl: '', model: '', fallback: '' });

	let selectedServiceIsLocal = $derived(
		services.find(s => s.iri === selectedServiceIri)?.isLocal ?? false
	);
	let selectedService = $derived(services.find((service) => service.iri === selectedServiceIri) ?? null);
	let selectedServiceLabel = $derived(
		selectedService ? aiServiceDisplayName(selectedService) : ''
	);
	let selectedServiceIsOpenRouter = $derived(selectedServiceLabel.toLowerCase().includes('openrouter'));
	let loadedServiceLabel = $derived(
		loadedServiceIri
			? aiServiceDisplayName(services.find((service) => service.iri === loadedServiceIri) ?? {})
			: 'Nenhum provedor configurado'
	);
	let loadedOpenRouterModel = $derived(
		openRouterModels.find((model) => model.id === loadedOpenRouterModelId) ?? null
	);
	let configuredModelLabel = $derived(
		currentModelLabel || loadedOpenRouterModel?.name || (loadedOpenRouterModelId ? loadedOpenRouterModelId : 'Nenhum modelo selecionado')
	);
	let modelPickerCanConfirm = $derived(
		!!modelPickerDraftId
			&& (
				(modelPickerDraftId === loadedOpenRouterModelId && !aiDirty)
				|| (!!selectedServiceIri && (selectedServiceIsLocal || apiKey.trim().length > 0) && (modelPickerDraftId !== loadedOpenRouterModelId || aiDirty))
			)
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

	let aiDirty = $derived(
		selectedServiceIri !== loadedServiceIri
		|| selectedModelIri !== loadedModelIri
		|| (selectedServiceIsOpenRouter && selectedOpenRouterModelId !== loadedOpenRouterModelId)
	);

	function aiProviderLogo(service) {
		const name = aiServiceDisplayName(service).toLowerCase();
		if (name.includes('openrouter')) return openRouterLogo;
		if (name.includes('claude') || name.includes('anthropic')) return anthropicLogo;
		return null;
	}

	function aiProviderIcon(service) {
		return service.isLocal ? 'memory' : 'psychology';
	}

	function runtimeReadinessLabel(service) {
		const readiness = service.runtimeReadiness;
		if (!readiness || readiness.isReady) return 'Pronto';
		const reasons = {
			needs_credentials: 'Adicione uma chave de API nas configurações abaixo.',
			needs_endpoint: 'Configure o endereço do serviço nas configurações abaixo.',
			needs_local_model: 'Instale um modelo local para usar este serviço.',
			unsupported_route: 'Esta rota de execução não é suportada atualmente.',
		};
		return `Configuração incompleta · ${reasons[readiness.status] ?? 'Revise as configurações deste serviço.'}`;
	}

	function setAiFieldStatus(field, status, error = '') {
		aiFieldStatus = { ...aiFieldStatus, [field]: status };
		aiFieldError = { ...aiFieldError, [field]: error };
	}

	function aiFieldStatusLabel(field) {
		if (aiFieldStatus[field] === 'saving') return 'Salvando…';
		if (aiFieldStatus[field] === 'saved') return 'Salvo';
		if (aiFieldStatus[field] === 'error') return aiFieldError[field] || 'Não foi possível salvar';
		return '';
	}

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

	async function refreshAIServiceReadiness() {
		const generation = ++readinessRefreshGeneration;
		try {
			const refreshedServices = await invoke('setup__list_ai_services');
			if (generation === readinessRefreshGeneration) services = refreshedServices;
		} catch {
			// Keep the last known service state if the refresh fails.
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
			refreshAIServiceReadiness();
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
		if (serviceIri !== selectedServiceIri) return;
		const generation = ++serviceConfigLoadGeneration;
		apiKey = '';
		loadedApiKey = '';
		baseUrl = '';
		loadedBaseUrl = '';
		fallbackModelIds = [];
		loadedFallbackModelIds = [];
		if (!serviceIri) {
			apiKeyLoading = false;
			return;
		}
		apiKeyLoading = true;
		try {
			const [key, url, fbModels] = await Promise.all([
				invoke('ai__get_api_key', { serviceIri }),
				invoke('ai__get_service_base_url', { serviceIri }),
				invoke('ai__get_fallback_models', { serviceIri }),
			]);
			if (generation !== serviceConfigLoadGeneration || serviceIri !== selectedServiceIri) return;
			apiKey = key ?? '';
			loadedApiKey = apiKey;
			baseUrl = url ?? '';
			loadedBaseUrl = baseUrl;
			fallbackModelIds = fbModels ?? [];
			loadedFallbackModelIds = [...fallbackModelIds];
		} catch {
			if (generation !== serviceConfigLoadGeneration || serviceIri !== selectedServiceIri) return;
			apiKey = '';
			loadedApiKey = '';
			baseUrl = '';
			loadedBaseUrl = '';
			fallbackModelIds = [];
			loadedFallbackModelIds = [];
		} finally {
			if (generation === serviceConfigLoadGeneration && serviceIri === selectedServiceIri) {
				apiKeyLoading = false;
			}
		}
	}

	async function deleteApiKey() {
		if (!selectedServiceIri || selectedServiceIsLocal) return;
		const serviceIri = selectedServiceIri;
		apiKeyDeleting = true;
		setAiFieldStatus('apiKey', 'saving');
		try {
			await invoke('ai__delete_api_key', { serviceIri });
			if (serviceIri !== selectedServiceIri) {
				await refreshAIServiceReadiness();
				return;
			}
			apiKey = '';
			loadedApiKey = '';
			setAiFieldStatus('apiKey', 'saved');
			await refreshAIServiceReadiness();
		} catch (e) {
			if (serviceIri === selectedServiceIri) setAiFieldStatus('apiKey', 'error', String(e));
		} finally {
			apiKeyDeleting = false;
		}
	}

	async function loadModelData() {
		aiServicesLoading = true;
		aiServicesLoadError = false;
		try {
			const [svcs, currentSvc] = await Promise.all([
				invoke('setup__list_ai_services'),
				invoke('setup__get_current_ai_service'),
			]);
			services = svcs;
			if (currentSvc) {
				selectedServiceIri = currentSvc.iri;
				loadedServiceIri = currentSvc.iri;
			} else if (services.length > 0) {
				selectedServiceIri = services[0].iri;
				loadedServiceIri = '';
			}
			if (selectedServiceIri) {
				const [current, ,] = await Promise.all([
					invoke('setup__get_current_ai_model'),
					loadModels(selectedServiceIri),
					loadApiKeyForService(selectedServiceIri),
				]);
				applyConfiguredModel(current, selectedServiceIri);
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

	async function onServiceChange(serviceIri) {
		if (!serviceIri || serviceIri === selectedServiceIri || aiSaving) return;
		const previousServiceIri = selectedServiceIri;
		const generation = ++serviceChangeGeneration;
		for (const field of ['apiKey', 'baseUrl', 'model', 'fallback']) setAiFieldStatus(field, 'idle');
		selectedServiceIri = serviceIri;
		setAiFieldStatus('provider', 'saving');
		try {
			await invoke('setup__save_ai_service', { serviceIri });
			if (generation !== serviceChangeGeneration) return;
			const currentModel = await invoke('setup__get_current_ai_model');
			if (generation !== serviceChangeGeneration) return;
			loadedServiceIri = serviceIri;
			applyConfiguredModel(currentModel, serviceIri);
			openRouterModels = [];
			openRouterModelsError = '';
			setAiFieldStatus('provider', 'saved');
			await Promise.all([loadModels(serviceIri), loadApiKeyForService(serviceIri)]);
			if (generation !== serviceChangeGeneration) return;
			if (aiServiceDisplayName(services.find((service) => service.iri === serviceIri) ?? {}).toLowerCase().includes('openrouter') && apiKey.trim() && baseUrl.trim()) {
				await loadOpenRouterModels();
			}
		} catch (error) {
			if (generation === serviceChangeGeneration) {
				selectedServiceIri = previousServiceIri;
				setAiFieldStatus('provider', 'error', String(error));
			}
		}
	}

	function applyConfiguredModel(model, serviceIri) {
		const service = services.find((item) => item.iri === serviceIri);
		const isOpenRouter = aiServiceDisplayName(service ?? {}).toLowerCase().includes('openrouter');
		selectedModelIri = model?.iri ?? '';
		loadedModelIri = model?.iri ?? '';
		currentModelLabel = model?.label ?? '';
		selectedOpenRouterModelId = isOpenRouter ? (model?.modelIdentifier ?? '') : '';
		loadedOpenRouterModelId = selectedOpenRouterModelId;
	}

	async function saveApiKey() {
		if (!selectedServiceIri || selectedServiceIsLocal || apiKey === loadedApiKey) return;
		if (!apiKey.trim()) {
			setAiFieldStatus('apiKey', 'idle');
			return;
		}
		const serviceIri = selectedServiceIri;
		const apiKeyToSave = apiKey;
		const baseUrlToUse = baseUrl;
		const isOpenRouter = aiServiceDisplayName(
			services.find((service) => service.iri === serviceIri) ?? {},
		).toLowerCase().includes('openrouter');
		setAiFieldStatus('apiKey', 'saving');
		try {
			if (isOpenRouter) {
				if (!baseUrlToUse.trim()) throw new Error('Informe a URL base antes de salvar a chave.');
				await invoke('ai__validate_provider_key', { apiKey: apiKeyToSave, baseUrl: baseUrlToUse });
			}
			await invoke('ai__save_api_key', { apiKey: apiKeyToSave, serviceIri });
			if (serviceIri !== selectedServiceIri) {
				await refreshAIServiceReadiness();
				return;
			}
			loadedApiKey = apiKeyToSave;
			setAiFieldStatus('apiKey', 'saved');
			await refreshAIServiceReadiness();
			if (isOpenRouter && baseUrlToUse.trim()) await loadOpenRouterModels();
		} catch (error) {
			if (serviceIri === selectedServiceIri) setAiFieldStatus('apiKey', 'error', String(error));
		}
	}

	async function saveBaseUrl() {
		if (!selectedServiceIri || !selectedServiceIsOpenRouter || baseUrl === loadedBaseUrl) return;
		const serviceIri = selectedServiceIri;
		const baseUrlToSave = baseUrl;
		setAiFieldStatus('baseUrl', 'saving');
		try {
			await invoke('ai__save_service_base_url', { serviceIri, baseUrl: baseUrlToSave });
			if (serviceIri !== selectedServiceIri) {
				await refreshAIServiceReadiness();
				return;
			}
			loadedBaseUrl = baseUrlToSave;
			setAiFieldStatus('baseUrl', 'saved');
			await refreshAIServiceReadiness();
			if (apiKey.trim() && baseUrlToSave.trim()) await loadOpenRouterModels();
		} catch (error) {
			if (serviceIri === selectedServiceIri) setAiFieldStatus('baseUrl', 'error', String(error));
		}
	}

	async function saveSelectedModel(modelIri) {
		if (!selectedServiceIri || !modelIri || modelIri === loadedModelIri || aiSaving) return false;
		const serviceIri = selectedServiceIri;
		const previousModelIri = loadedModelIri;
		setAiFieldStatus('model', 'saving');
		aiSaving = true;
		try {
			if (!selectedServiceIsLocal && !apiKey.trim()) throw new Error('Informe a chave de API antes de ativar o modelo.');
			if (serviceIri !== loadedServiceIri) {
				await invoke('setup__save_ai_service', { serviceIri });
			}
			await invoke('setup__save_ai_model', { modelIri });
			loadedServiceIri = serviceIri;
			selectedModelIri = modelIri;
			loadedModelIri = modelIri;
			currentModelLabel = models.find((model) => model.iri === modelIri)?.label ?? '';
			setAiFieldStatus('model', 'saved');
			return true;
		} catch (error) {
			selectedModelIri = previousModelIri;
			setAiFieldStatus('model', 'error', String(error));
			return false;
		} finally {
			aiSaving = false;
		}
	}

	async function saveFallbackModels(nextFallbackModelIds) {
		if (!selectedServiceIri || !selectedServiceIsOpenRouter || fallbackPickerSaving) return false;
		fallbackPickerSaving = true;
		setAiFieldStatus('fallback', 'saving');
		try {
			await invoke('ai__save_fallback_models', { serviceIri: selectedServiceIri, modelIds: nextFallbackModelIds });
			fallbackModelIds = [...nextFallbackModelIds];
			loadedFallbackModelIds = [...nextFallbackModelIds];
			setAiFieldStatus('fallback', 'saved');
			return true;
		} catch (error) {
			setAiFieldStatus('fallback', 'error', String(error));
			return false;
		} finally {
			fallbackPickerSaving = false;
		}
	}

	async function openFallbackPicker() {
		fallbackPickerDraftIds = [...fallbackModelIds];
		fallbackPickerError = '';
		fallbackPickerOpen = true;
		if (openRouterModels.length === 0 && !openRouterModelsLoading && apiKey.trim() && baseUrl.trim()) {
			await loadOpenRouterModels();
		}
	}

	function closeFallbackPicker() {
		if (fallbackPickerSaving) return;
		fallbackPickerOpen = false;
		fallbackPickerDraftIds = [...fallbackModelIds];
		fallbackPickerError = '';
		requestAnimationFrame(() => fallbackPickerTrigger?.focus());
	}

	function moveFallbackDraft(index, offset) {
		const nextIndex = index + offset;
		if (nextIndex < 0 || nextIndex >= fallbackPickerDraftIds.length) return;
		const next = [...fallbackPickerDraftIds];
		[next[index], next[nextIndex]] = [next[nextIndex], next[index]];
		fallbackPickerDraftIds = next;
	}

	async function saveFallbackSelection() {
		fallbackPickerError = '';
		const saved = await saveFallbackModels([...fallbackPickerDraftIds]);
		if (saved) closeFallbackPicker();
		else fallbackPickerError = aiFieldError.fallback || 'Não foi possível salvar a lista de fallbacks.';
	}

	function fallbackModelLabel(modelId) {
		const model = openRouterModels.find((item) => item.id === modelId);
		return model ? openRouterModelLabel(model) : modelId;
	}

	async function saveAiConfig(openRouterModelIdOverride = null) {
		const modelIdToSave = openRouterModelIdOverride ?? selectedOpenRouterModelId;
		const orModel = openRouterModels.find((model) => model.id === modelIdToSave);
		if (!orModel || !selectedServiceIri || !selectedServiceIsOpenRouter || aiSaving) return false;
		aiSaving = true;
		aiSavingStage = 'Validando o modelo escolhido…';
		setAiFieldStatus('model', 'saving');
		aiMessage = '';
		aiError = false;
		try {
			if (!apiKey.trim() || !baseUrl.trim()) throw new Error('Informe e salve a chave e a URL base do provedor antes de escolher um modelo.');
			await invoke('ai__validate_provider_key', { apiKey, baseUrl });
			aiSavingStage = 'Registrando o modelo escolhido…';
			const modelIri = await invoke('ai__ensure_openrouter_model', {
				serviceIri: selectedServiceIri,
				modelId: orModel.id,
				modelName: orModel.name,
				contextLength: orModel.contextLength,
				inputCostPerToken: orModel.inputCostPerToken,
				outputCostPerToken: orModel.outputCostPerToken,
				supportsTools: orModel.supportsTools ?? false,
			});
			aiSavingStage = 'Ativando o modelo…';
			await invoke('setup__save_ai_model', { modelIri });
			selectedModelIri = modelIri;
			loadedModelIri = modelIri;
			selectedOpenRouterModelId = modelIdToSave;
			loadedOpenRouterModelId = modelIdToSave;
			currentModelLabel = orModel.name;
			setAiFieldStatus('model', 'saved');
			return true;
		} catch (error) {
			aiMessage = String(error);
			aiError = true;
			setAiFieldStatus('model', 'error', String(error));
			return false;
		} finally {
			aiSaving = false;
			aiSavingStage = '';
		}
	}

	async function openModelPicker() {
		modelPickerDraftId = selectedOpenRouterModelId || loadedOpenRouterModelId;
		modelPickerOpen = true;
		if (apiKey.trim() && baseUrl.trim() && openRouterModels.length === 0 && !openRouterModelsLoading) {
			await loadOpenRouterModels();
		}
	}

	function closeModelPicker() {
		if (aiSaving) return;
		modelPickerOpen = false;
		modelPickerDraftId = loadedOpenRouterModelId;
		requestAnimationFrame(() => modelPickerTrigger?.focus());
	}

	async function saveAndUseModel() {
		if (!modelPickerDraftId || aiSaving) return;
		if (modelPickerDraftId === loadedOpenRouterModelId && !aiDirty) {
			closeModelPicker();
			return;
		}
		const saved = await saveAiConfig(modelPickerDraftId);
		if (saved) {
			modelPickerOpen = false;
			modelPickerDraftId = loadedOpenRouterModelId;
			aiMessage = '';
			requestAnimationFrame(() => modelPickerTrigger?.focus());
			return;
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
		if (e.key === 'Escape' && !e.defaultPrevented && imapEditing === null && !modelPickerOpen) onClose?.();
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
					Modelo de IA
				</h3>
				{#if aiServicesLoading}
					<p class="hint" role="status" aria-live="polite">Carregando provedores e modelos…</p>
				{:else if aiServicesLoadError}
					<p class="feedback error" role="alert">Não foi possível carregar provedores e modelos.</p>
				{:else if services.length > 0}
					<fieldset class="provider-selector">
						<legend>Escolha o provedor</legend>
						<div class="provider-card-grid">
							{#each services as svc}
								{@const providerName = aiServiceDisplayName(svc)}
								<div class="provider-option" class:selected={selectedServiceIri === svc.iri}>
									<label class="provider-choice">
										<input
											type="radio"
											name="settings-ai-provider"
											value={svc.iri}
											checked={selectedServiceIri === svc.iri}
											disabled={aiSaving}
											onchange={() => onServiceChange(svc.iri)}
										/>
										<span class="provider-brand-icon" title="Identidade visual de {providerName}">
											{#if aiProviderLogo(svc)}
												<img src={aiProviderLogo(svc)} alt="Logo oficial de {providerName}" />
											{:else}
												<span class="material-symbols-outlined">{aiProviderIcon(svc)}</span>
											{/if}
										</span>
										<span class="provider-choice-copy">
											<span class="provider-name">{providerName}</span>
											<span class="provider-description">{svc.description || `Serviço de IA ${providerName}.`}</span>
											<span class="provider-readiness" class:ready={svc.runtimeReadiness?.isReady} class:incomplete={!svc.runtimeReadiness?.isReady}>
												{runtimeReadinessLabel(svc)}
											</span>
										</span>
									</label>
									<div class="provider-links" aria-label="Links oficiais de {providerName}">
										{#if svc.seeAlsoUrl}
											<a href={svc.seeAlsoUrl} target="_blank" rel="noopener noreferrer">Site oficial</a>
										{/if}
										{#if svc.apiDocumentation}
											<a href={svc.apiDocumentation} target="_blank" rel="noopener noreferrer">Ajuda de configuração</a>
										{/if}
									</div>
								</div>
							{/each}
						</div>
						<p class="field-save-status" class:saving={aiFieldStatus.provider === 'saving'} class:error={aiFieldStatus.provider === 'error'} role="status" aria-live="polite">
							{aiFieldStatusLabel('provider')}
						</p>
					</fieldset>

					{#if selectedService}
					<section class="provider-settings" aria-labelledby="settings-provider-heading">
						<header class="provider-settings-heading">
							<div class="provider-settings-title">
								<span class="provider-brand-icon" title="Identidade visual de {selectedServiceLabel}">
									{#if aiProviderLogo(selectedService)}
										<img src={aiProviderLogo(selectedService)} alt="Logo oficial de {selectedServiceLabel}" />
									{:else}
										<span class="material-symbols-outlined">{aiProviderIcon(selectedService)}</span>
									{/if}
								</span>
								<div>
									<h4 id="settings-provider-heading">Configuração · {selectedServiceLabel}</h4>
									<p>As opções abaixo pertencem a este provedor.</p>
								</div>
							</div>
							{#if selectedServiceIsOpenRouter}
						<div class="configured-model" aria-label="Modelo de IA configurado">
							<div class="configured-model-copy">
								<span class="configured-model-label">MODELO ATUAL</span>
								<strong>{configuredModelLabel}</strong>
								<span class="configured-model-provider">{loadedServiceLabel}</span>
							</div>
							<Button bind:ref={modelPickerTrigger} variant="outline" onclick={openModelPicker} disabled={aiSaving}>
								<span class="material-symbols-outlined" aria-hidden="true">search</span>
								Alterar modelo
							</Button>
						</div>
							{/if}
						</header>

						<div class="provider-field-grid">
							{#if !selectedServiceIsLocal}
								<div class="settings-field">
						<label class="field-label" for="settings-api-key-input">Chave de API</label>
						{#if apiKeyLoading}
							<p class="hint">Carregando…</p>
						{:else}
							<div class="field-row">
								<Input
									id="settings-api-key-input"
									type="password"
									placeholder={selectedServiceIsOpenRouter ? 'sk-or-…' : 'sk-ant-…'}
									bind:value={apiKey}
									oninput={() => setAiFieldStatus('apiKey', 'idle')}
									onblur={saveApiKey}
								/>
								<Button variant="destructive" size="icon"
									title="Remover chave de API"
									aria-label="Remover chave de API"
									onclick={deleteApiKey}
									disabled={apiKeyDeleting || !loadedApiKey}
								><span class="material-symbols-outlined">delete</span></Button>
							</div>
							<p class="field-save-status" class:saving={aiFieldStatus.apiKey === 'saving'} class:error={aiFieldStatus.apiKey === 'error'} role="status" aria-live="polite">{aiFieldStatusLabel('apiKey')}</p>
						{/if}
							</div>
					{/if}

							{#if selectedServiceIsOpenRouter}
								<div class="settings-field">
						<label class="field-label" for="settings-base-url-input">URL Base</label>
						<div class="field-row">
							<Input
								id="settings-base-url-input"
								type="url"
								placeholder="https://openrouter.ai/api/v1"
								bind:value={baseUrl}
								oninput={() => setAiFieldStatus('baseUrl', 'idle')}
								onblur={saveBaseUrl}
							/>
						</div>
						<p class="field-save-status" class:saving={aiFieldStatus.baseUrl === 'saving'} class:error={aiFieldStatus.baseUrl === 'error'} role="status" aria-live="polite">{aiFieldStatusLabel('baseUrl')}</p>
								</div>
					{/if}

							{#if selectedServiceIsOpenRouter}
						<Dialog.Root open={modelPickerOpen} onOpenChange={(open) => { if (!open) closeModelPicker(); }}>
							<Dialog.Content class="model-picker-dialog" aria-describedby="settings-model-picker-description">
								<Dialog.Header>
									<Dialog.Title>Escolher modelo de IA</Dialog.Title>
									<Dialog.Description id="settings-model-picker-description">
										Busque e compare opções. Ao confirmar, o modelo será salvo imediatamente para este provedor.
									</Dialog.Description>
								</Dialog.Header>
								<div class="model-picker-body">
									<OpenRouterModelSelector
										idPrefix="settings-openrouter"
										providerName={selectedServiceLabel}
										models={openRouterModels}
										benchmarks={openRouterBenchmarks}
										bind:selectedModelId={modelPickerDraftId}
										loading={openRouterModelsLoading}
										error={openRouterModelsError}
										benchmarksLoading={openRouterBenchmarksLoading}
										benchmarksError={openRouterBenchmarksError}
										loadHint={!apiKey.trim() ? 'Informe a chave de API para carregar o catálogo.' : !baseUrl.trim() ? 'Informe a URL base do OpenRouter para carregar o catálogo.' : 'Carregue o catálogo para comparar os modelos disponíveis.'}
										onRetry={loadOpenRouterModels}
									/>
								</div>
									{#if aiSaving}
										<p class="model-picker-saving" role="status" aria-live="polite">{aiSavingStage}</p>
									{/if}
								<Dialog.Footer class="model-picker-actions">
									<Button variant="ghost" onclick={closeModelPicker} disabled={aiSaving}>Cancelar</Button>
									<Button variant="default" onclick={saveAndUseModel} disabled={aiSaving || !modelPickerCanConfirm} aria-live="polite">
										{aiSaving ? 'Salvando modelo…' : 'Salvar e usar modelo'}
									</Button>
								</Dialog.Footer>
								{#if aiError && aiMessage}
									<p class="model-picker-error" role="alert" aria-live="assertive">Não foi possível salvar o modelo: {aiMessage}</p>
								{/if}
							</Dialog.Content>
						</Dialog.Root>
					{/if}

							{#if selectedServiceIsOpenRouter}
								<div class="settings-field fallback-field">
						<label class="field-label" for="settings-fallback-manage">Modelos de fallback — ordem de prioridade</label>
						<div class="fallback-list" aria-label="Modelos de fallback salvos por prioridade">
							{#each fallbackModelIds as fbId, i}
								<div class="fallback-item" aria-label={`Prioridade ${i + 1}: ${fallbackModelLabel(fbId)}`}>
									<span class="fallback-priority" aria-hidden="true">{i + 1}</span>
									<span class="fallback-model-name">{fallbackModelLabel(fbId)}</span>
								</div>
							{/each}
							{#if fallbackModelIds.length === 0}
								<p class="fallback-empty">Nenhum modelo de fallback configurado.</p>
							{/if}
							<Button id="settings-fallback-manage" variant="secondary" size="sm" bind:ref={fallbackPickerTrigger} onclick={openFallbackPicker}>
								<span class="material-symbols-outlined" aria-hidden="true">tune</span>
								Gerenciar modelos de fallback
							</Button>
						</div>
						<p class="field-save-status" class:saving={aiFieldStatus.fallback === 'saving'} class:error={aiFieldStatus.fallback === 'error'} role="status" aria-live="polite">{aiFieldStatusLabel('fallback')}</p>
						<Dialog.Root open={fallbackPickerOpen} onOpenChange={(open) => { if (!open) closeFallbackPicker(); }}>
							<Dialog.Content class="model-picker-dialog fallback-picker-dialog" aria-describedby="settings-fallback-picker-description">
								<Dialog.Header>
									<Dialog.Title>Adicionar modelos de fallback</Dialog.Title>
									<Dialog.Description id="settings-fallback-picker-description">
										Busque e ordene o catálogo. A ordem na lista de prioridade define qual modelo será tentado primeiro.
									</Dialog.Description>
								</Dialog.Header>
								<div class="fallback-picker-body">
									<div class="fallback-catalog">
										<OpenRouterModelSelector
											idPrefix="settings-fallback"
											providerName={selectedServiceLabel}
											models={openRouterModels}
											benchmarks={openRouterBenchmarks}
											bind:selectedModelIds={fallbackPickerDraftIds}
											selectionMode="multiple"
											loading={openRouterModelsLoading}
											error={openRouterModelsError}
											benchmarksLoading={openRouterBenchmarksLoading}
											benchmarksError={openRouterBenchmarksError}
											loadHint={!apiKey.trim() ? 'Informe e salve a chave de API para carregar o catálogo.' : !baseUrl.trim() ? 'Informe a URL base do OpenRouter para carregar o catálogo.' : 'Carregue o catálogo para comparar os modelos disponíveis.'}
											onRetry={loadOpenRouterModels}
										/>
									</div>
									<aside class="fallback-priority-panel" aria-labelledby="settings-fallback-priority-heading">
										<h3 id="settings-fallback-priority-heading">Ordem de prioridade</h3>
										<p>Ordenar os resultados não altera esta sequência.</p>
										{#if fallbackPickerDraftIds.length === 0}
											<p class="fallback-empty" role="status">Selecione modelos no catálogo para montar a lista.</p>
										{:else}
											<ol class="fallback-priority-list">
												{#each fallbackPickerDraftIds as fallbackId, i (fallbackId)}
													<li class="fallback-priority-entry">
														<span class="fallback-priority" aria-label={`Prioridade ${i + 1}`}>{i + 1}</span>
														<span class="fallback-model-name">{fallbackModelLabel(fallbackId)}</span>
														<div class="fallback-priority-actions">
															<Button variant="ghost" size="icon" aria-label={`Subir ${fallbackModelLabel(fallbackId)}`} title="Subir prioridade" disabled={i === 0} onclick={() => moveFallbackDraft(i, -1)}><span class="material-symbols-outlined" aria-hidden="true">arrow_upward</span></Button>
															<Button variant="ghost" size="icon" aria-label={`Descer ${fallbackModelLabel(fallbackId)}`} title="Descer prioridade" disabled={i === fallbackPickerDraftIds.length - 1} onclick={() => moveFallbackDraft(i, 1)}><span class="material-symbols-outlined" aria-hidden="true">arrow_downward</span></Button>
															<Button variant="ghost" size="icon" aria-label={`Remover ${fallbackModelLabel(fallbackId)}`} title="Remover fallback" onclick={() => { fallbackPickerDraftIds = fallbackPickerDraftIds.filter((_, index) => index !== i); }}><span class="material-symbols-outlined" aria-hidden="true">remove</span></Button>
														</div>
													</li>
												{/each}
											</ol>
										{/if}
									</aside>
								</div>
								{#if fallbackPickerSaving}
									<p class="model-picker-saving" role="status" aria-live="polite">Salvando ordem de prioridade…</p>
								{/if}
								{#if fallbackPickerError}
									<p class="model-picker-error" role="alert" aria-live="assertive">{fallbackPickerError}</p>
								{/if}
								<Dialog.Footer class="model-picker-actions">
									<Button variant="ghost" onclick={closeFallbackPicker} disabled={fallbackPickerSaving}>Cancelar</Button>
									<Button variant="default" onclick={saveFallbackSelection} disabled={fallbackPickerSaving || (fallbackPickerDraftIds.length === loadedFallbackModelIds.length && fallbackPickerDraftIds.every((id, index) => id === loadedFallbackModelIds[index]))}>
										{fallbackPickerSaving ? 'Salvando…' : 'Salvar seleção'}
									</Button>
								</Dialog.Footer>
							</Dialog.Content>
						</Dialog.Root>
								</div>
					{/if}

							{#if !selectedServiceIsOpenRouter && models.length > 0}
								<div class="settings-field">
						<label class="field-label" for="settings-model-select">Modelo padrão</label>
						<div class="field-row">
							<Select.Root
								type="single"
								value={selectedModelIri}
								onValueChange={(v) => { if (v !== undefined && !aiSaving) { selectedModelIri = v; saveSelectedModel(v); } }}
							>
								<Select.Trigger id="settings-model-select" class="flex-1" disabled={aiSaving}>
									<Select.Value placeholder="Selecione um modelo…" />
								</Select.Trigger>
								<Select.Content>
									{#each models as model}
										<Select.Item value={model.iri}>{model.label}</Select.Item>
									{/each}
								</Select.Content>
							</Select.Root>
						</div>
						<p class="field-save-status" class:saving={aiFieldStatus.model === 'saving'} class:error={aiFieldStatus.model === 'error'} role="status" aria-live="polite">{aiFieldStatusLabel('model')}</p>
						{#if !selectedServiceIsLocal && !apiKey.trim()}
							<p class="feedback error">Informe a chave de API antes de ativar este provedor.</p>
						{/if}
								</div>
					{/if}

						</div>
					</section>
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

	#settings-panel-ai {
		width: 100%;
		max-width: var(--size-settings-content-max-width);
		margin-inline: auto;
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

		.provider-field-grid {
			grid-template-columns: minmax(0, 1fr);
		}
	}

	.settings-section {
		display: flex;
		flex-direction: column;
		gap: var(--spacing-field-block);
		min-width: 0;
	}

	.provider-selector {
		min-width: 0;
		margin: 0;
		padding: 0;
		border: 0;
	}

	.provider-selector legend {
		margin-bottom: var(--spacing-field-group);
		color: var(--text-primary);
		font-size: var(--type-body-prominent);
	}

	.provider-card-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(min(100%, var(--size-settings-provider-option-min-width)), 1fr));
		gap: var(--spacing-field-group);
	}

	.provider-option {
		display: grid;
		align-content: start;
		gap: var(--spacing-control-internal);
		min-width: 0;
		padding: var(--spacing-widget-inset);
		border: var(--size-hud-frame-line) solid var(--color-hud-line-subtle);
		border-radius: var(--radius);
		background: var(--color-surface-1);
		transition: border-color 140ms ease, background-color 140ms ease;
	}

	.provider-option.selected {
		border-color: var(--color-selection);
		background: var(--color-selection-subtle);
	}

	.provider-choice {
		display: flex;
		align-items: start;
		gap: var(--spacing-control-internal);
		min-width: 0;
		color: var(--text-body);
		cursor: pointer;
	}

	.provider-choice input {
		flex: 0 0 auto;
		margin-block-start: var(--spacing-inline-tight);
		accent-color: var(--color-selection);
	}

	.provider-choice:focus-within .provider-brand-icon {
		outline: var(--size-hud-frame-line) solid var(--color-selection);
		outline-offset: var(--spacing-inline-tight);
	}

	.provider-brand-icon {
		display: grid;
		place-items: center;
		flex: 0 0 auto;
		width: var(--size-touch-target);
		aspect-ratio: 1;
		border-radius: var(--radius);
		background: var(--color-surface-3);
		color: var(--text-accent);
	}

	.provider-brand-icon .material-symbols-outlined {
		font-size: var(--type-icon-medium);
	}

	.provider-brand-icon img {
		width: var(--type-icon-medium);
		max-height: var(--type-icon-medium);
		object-fit: contain;
	}

	.provider-brand-icon:has(img[alt*="Anthropic"]) {
		background: var(--color-neutral);
	}

	.provider-choice-copy {
		display: grid;
		gap: var(--spacing-label-to-value);
		min-width: 0;
	}

	.provider-name {
		color: var(--text-primary);
		font-size: var(--type-body-prominent);
	}

	.provider-description {
		color: var(--text-secondary);
		font-size: var(--type-body-compact);
		line-height: var(--line-height-body);
	}

	.provider-readiness {
		font-size: var(--type-caption);
		line-height: var(--line-height-hud-compact);
	}

	.provider-readiness.ready {
		color: var(--color-success);
	}

	.provider-readiness.incomplete {
		color: var(--color-warning);
	}

	.provider-links {
		display: flex;
		flex-wrap: wrap;
		gap: var(--spacing-inline-related);
		padding-inline-start: calc(var(--size-touch-target) + var(--spacing-control-internal));
	}

	.provider-links a {
		color: var(--text-accent);
		font-size: var(--type-caption);
		text-decoration: underline;
		text-underline-offset: var(--spacing-inline-tight);
	}

	.provider-settings {
		display: grid;
		gap: var(--spacing-field-group);
		min-width: 0;
		padding: var(--spacing-widget-inset);
		border: var(--size-hud-frame-line) solid var(--color-hud-line-subtle);
		border-radius: var(--radius);
		background: var(--color-surface-1);
	}

	.provider-settings-heading {
		display: grid;
		gap: var(--spacing-field-group);
		min-width: 0;
	}

	.provider-settings-title {
		display: flex;
		align-items: center;
		gap: var(--spacing-control-internal);
		min-width: 0;
	}

	.provider-settings-title h4 {
		margin: 0;
		color: var(--text-primary);
		font-size: var(--type-heading);
		font-weight: 400;
	}

	.provider-settings-title p {
		margin: var(--spacing-label-to-value) 0 0;
		color: var(--text-secondary);
		font-size: var(--type-body-compact);
	}

	.provider-field-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(min(100%, var(--size-settings-field-min-width)), 1fr));
		gap: var(--spacing-field-group) var(--spacing-section);
		align-items: start;
		min-width: 0;
	}

	.settings-field {
		display: grid;
		gap: var(--spacing-label-to-value);
		min-width: 0;
	}

	.fallback-field {
		grid-column: 1 / -1;
	}

	.fallback-list {
		display: grid;
		gap: var(--spacing-inline-related);
		min-width: 0;
	}

	.fallback-item {
		display: flex;
		align-items: center;
		gap: var(--spacing-control-internal);
		min-width: 0;
		padding-inline-start: var(--spacing-control-internal);
		border-bottom: var(--size-hud-frame-line) solid var(--color-hud-line-subtle);
	}

	.fallback-model-name {
		flex: 1;
		min-width: 0;
		color: var(--text-body);
		font-size: var(--type-body-compact);
		overflow-wrap: anywhere;
	}

	.fallback-empty {
		margin: 0;
		color: var(--text-secondary);
		font-size: var(--type-body-compact);
	}

	.fallback-priority {
		display: inline-grid;
		place-items: center;
		flex: 0 0 auto;
		min-width: var(--size-settings-priority-indicator);
		min-height: var(--size-settings-priority-indicator);
		padding-inline: var(--space-2);
		border: var(--size-hud-frame-line) solid var(--color-hud-line);
		border-radius: 999px;
		color: var(--text-body);
		font-family: var(--font-mono);
		font-size: var(--type-body-compact);
	}

	.field-save-status {
		min-height: var(--type-caption);
		margin: 0;
		color: var(--text-secondary);
		font-size: var(--type-caption);
		line-height: var(--line-height-hud-compact);
		overflow-wrap: anywhere;
	}

	.field-save-status.saving {
		color: var(--color-transition);
	}

	.field-save-status.error {
		color: var(--color-error);
	}

	.configured-model {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-4);
		padding: var(--spacing-widget-inset);
		border: 1px solid var(--color-hud-line);
		border-radius: var(--radius);
		background: var(--color-surface-2);
	}

	.configured-model-copy {
		display: grid;
		gap: var(--space-1);
		min-width: 0;
	}

	.configured-model-label {
		color: var(--text-secondary);
		font-family: var(--font-mono);
		font-size: var(--type-label);
		letter-spacing: var(--tracking-hud-label);
	}

	.configured-model-copy strong {
		color: var(--text-body);
		font-size: var(--type-title);
		font-weight: 400;
		overflow-wrap: anywhere;
	}

	.configured-model-provider {
		color: var(--text-secondary);
		font-size: var(--type-body-compact);
	}

	.configured-model :global([data-slot="button"]) {
		flex-shrink: 0;
	}

	:global([data-slot="dialog-content"].model-picker-dialog) {
		display: flex;
		flex-direction: column;
		width: min(92vw, 80rem);
		max-width: min(92vw, 80rem);
		max-height: 90vh;
		min-height: 0;
		overflow: hidden;
		background: var(--color-surface-3);
		color: var(--text-body);
	}

	.model-picker-body {
		min-width: 0;
		min-height: 0;
		overflow: auto;
		padding-inline-end: var(--space-1);
	}

	:global([data-slot="dialog-content"].fallback-picker-dialog) {
		width: min(96vw, 96rem);
		max-width: min(96vw, 96rem);
	}

	.fallback-picker-body {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(18rem, 0.34fr);
		align-items: start;
		gap: var(--spacing-section);
		min-width: 0;
		min-height: 0;
		overflow: auto;
		padding-inline-end: var(--space-1);
	}

	.fallback-catalog {
		min-width: 0;
	}

	.fallback-priority-panel {
		position: sticky;
		top: 0;
		display: grid;
		gap: var(--spacing-inline-related);
		min-width: 0;
		max-height: 72vh;
		overflow: auto;
		padding: var(--spacing-widget-inset);
		border: var(--size-hud-frame-line) solid var(--color-hud-line-subtle);
		border-radius: var(--radius);
		background: var(--color-surface-2);
	}

	.fallback-priority-panel h3 {
		margin: 0;
		color: var(--text-body);
		font-size: var(--type-heading);
		font-weight: 400;
	}

	.fallback-priority-panel > p {
		margin: 0;
		color: var(--text-secondary);
		font-size: var(--type-body-compact);
	}

	.fallback-priority-list {
		display: grid;
		gap: var(--spacing-inline-related);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.fallback-priority-entry {
		display: grid;
		grid-template-columns: auto minmax(0, 1fr) auto;
		align-items: center;
		gap: var(--spacing-control-internal);
		min-width: 0;
		padding-block: var(--space-1);
		border-bottom: var(--size-hud-frame-line) solid var(--color-hud-line-subtle);
	}

	.fallback-priority-entry .fallback-priority {
		min-width: var(--size-settings-priority-indicator);
		min-height: var(--size-settings-priority-indicator);
	}

	.fallback-priority-actions {
		display: flex;
		align-items: center;
		gap: var(--spacing-control-internal);
	}

	.fallback-priority-actions :global([data-slot="button"]) {
		min-width: var(--size-touch-target);
		min-height: var(--size-touch-target);
	}

	:global([data-slot="dialog-footer"].model-picker-actions) {
		flex-shrink: 0;
		padding-top: var(--space-2);
		border-top: 1px solid var(--color-hud-line-subtle);
	}

	.model-picker-error {
		margin: 0;
		color: var(--color-error);
		font-size: var(--type-body-compact);
	}

	.model-picker-saving {
		margin: var(--space-2) 0 0;
		color: var(--color-transition);
		font-size: var(--type-body-compact);
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

	@media (max-width: 42rem) {
		.configured-model {
			align-items: flex-start;
			flex-direction: column;
		}

		:global([data-slot="dialog-content"].model-picker-dialog) {
			width: calc(100vw - var(--space-4));
			max-width: calc(100vw - var(--space-4));
			max-height: 94vh;
		}

		.fallback-picker-body {
			grid-template-columns: minmax(0, 1fr);
		}

		.fallback-priority-panel {
			position: static;
			max-height: none;
		}

		.fallback-priority-entry {
			grid-template-columns: auto minmax(0, 1fr);
		}

		.fallback-priority-actions {
			grid-column: 2;
			justify-content: end;
		}
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
