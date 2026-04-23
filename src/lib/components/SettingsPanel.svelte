<script>
	import { invoke } from '@tauri-apps/api/core';

	let { onClose } = $props();

	// --- API Key ---
	let apiKey = $state('');
	let apiKeyLoading = $state(false);
	let apiKeySaving = $state(false);
	let apiKeyMessage = $state('');
	let apiKeyError = $state(false);

	// --- Model Selection ---
	let services = $state([]);
	let models = $state([]);
	let selectedServiceIri = $state('');
	let selectedModelIri = $state('');
	let selectedServiceIsLocal = $derived(
		services.find(s => s.iri === selectedServiceIri)?.isLocal ?? false
	);
	let currentModelLabel = $state('');
	let modelSaving = $state(false);
	let modelMessage = $state('');
	let modelError = $state(false);

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

	$effect(() => {
		loadAll();
		return () => { localModelUnlisten?.(); };
	});

	async function loadAll() {
		await Promise.all([loadModelData(), loadLogPath(), loadLocalModelStatus()]);
		await loadApiKeyForService(selectedServiceIri);
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
		if (!serviceIri) return;
		apiKeyLoading = true;
		try {
			const key = await invoke('ai__get_api_key', { serviceIri });
			apiKey = key ?? '';
		} catch {
			// leave blank on error
		} finally {
			apiKeyLoading = false;
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
			}
			if (currentSvc) {
				selectedServiceIri = currentSvc.iri;
			} else if (services.length > 0) {
				selectedServiceIri = services[0].iri;
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

	async function onServiceChange(e) {
		selectedServiceIri = e.target.value;
		selectedModelIri = '';
		await Promise.all([loadModels(selectedServiceIri), loadApiKeyForService(selectedServiceIri)]);
	}

	async function saveApiKey() {
		apiKeySaving = true;
		apiKeyMessage = '';
		apiKeyError = false;
		try {
			await invoke('ai__save_api_key', { apiKey, serviceIri: selectedServiceIri });
			apiKeyMessage = 'API key saved.';
		} catch (e) {
			apiKeyMessage = String(e);
			apiKeyError = true;
		} finally {
			apiKeySaving = false;
		}
	}

	async function saveModel() {
		if (!selectedModelIri) return;
		modelSaving = true;
		modelMessage = '';
		modelError = false;
		try {
			await Promise.all([
				invoke('setup__save_ai_model', { modelIri: selectedModelIri }),
				invoke('setup__save_ai_service', { serviceIri: selectedServiceIri }),
			]);
			const selected = models.find(m => m.iri === selectedModelIri);
			currentModelLabel = selected?.label ?? '';
			modelMessage = 'Saved.';
		} catch (e) {
			modelMessage = String(e);
			modelError = true;
		} finally {
			modelSaving = false;
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
			<button class="close-btn" onclick={onClose} title="Close">
				<span class="material-symbols-outlined">close</span>
			</button>
		</div>

		<div class="panel-body">
			{#if !selectedServiceIsLocal}
			<section class="settings-section">
				<h3 class="section-title">API Key</h3>
				{#if apiKeyLoading}
					<p class="hint">Loading…</p>
				{:else}
					<div class="field-row">
						<input
							class="text-input"
							type="password"
							placeholder="sk-ant-…"
							bind:value={apiKey}
						/>
						<button class="save-btn" onclick={saveApiKey} disabled={apiKeySaving || !apiKey}>
							{apiKeySaving ? 'Saving…' : 'Save'}
						</button>
					</div>
					{#if apiKeyMessage}
						<p class="feedback" class:error={apiKeyError}>{apiKeyMessage}</p>
					{/if}
				{/if}
			</section>
		{/if}

			<section class="settings-section">
				<h3 class="section-title">AI Model</h3>
				{#if currentModelLabel}
					<p class="hint">Current: <strong>{currentModelLabel}</strong></p>
				{/if}
				{#if services.length > 0}
					<div class="field-row">
						<select class="select-input" value={selectedServiceIri} onchange={onServiceChange}>
							{#each services as svc}
								<option value={svc.iri}>{svc.label}</option>
							{/each}
						</select>
					</div>
					{#if models.length > 0}
						<div class="field-row">
							<select class="select-input" bind:value={selectedModelIri}>
								<option value="">Select a model…</option>
								{#each models as model}
									<option value={model.iri}>{model.label}</option>
								{/each}
							</select>
							<button class="save-btn" onclick={saveModel} disabled={modelSaving || !selectedModelIri || (!selectedServiceIsLocal && !apiKey.trim())}>
								{modelSaving ? 'Saving…' : 'Save'}
							</button>
						</div>
						{#if !selectedServiceIsLocal && !apiKey.trim()}
							<p class="feedback error">Configure a chave de API acima antes de ativar este provedor.</p>
						{/if}
					{/if}
					{#if modelMessage}
						<p class="feedback" class:error={modelError}>{modelMessage}</p>
					{/if}
				{:else}
					<p class="hint">No AI services found.</p>
				{/if}
			</section>

			{#if selectedServiceIsLocal}
			<section class="settings-section">
				<h3 class="section-title">Modelo Local</h3>
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
						<button class="danger-btn" onclick={cancelModelDownload}>Cancelar</button>
					</div>
				{:else}
					<p class="hint">Modelo não instalado · {localModelSizeHuman}</p>
					<div class="field-row">
						<button class="save-btn" onclick={startModelDownload}>
							<span class="material-symbols-outlined btn-icon">download</span>
							Baixar modelo
						</button>
					</div>
				{/if}
				{#if localModelMessage}
					<p class="feedback" class:error={localModelError}>{localModelMessage}</p>
				{/if}
			</section>
			{/if}

			<section class="settings-section">
				<h3 class="section-title">Logs</h3>
				{#if logPath}
					<p class="log-path">{logPath}</p>
				{/if}
				<div class="field-row">
					<button class="danger-btn" onclick={clearLogs} disabled={logClearing}>
						{logClearing ? 'Clearing…' : 'Clear logs'}
					</button>
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
		background: var(--color-surface, #1e1e2e);
		width: 420px;
		max-width: 95vw;
		max-height: 85vh;
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
		color: var(--color-neutral-active, #e0e0e0);
	}

	.close-btn {
		background: none;
		border: none;
		color: var(--color-neutral, #888);
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 2px;
	}

	.close-btn .material-symbols-outlined {
		font-size: 20px;
	}

	.panel-body {
		overflow-y: auto;
		padding: 18px;
		display: flex;
		flex-direction: column;
		gap: 24px;
	}

	.settings-section {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.section-title {
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.08em;
		color: var(--color-neutral, #888);
		margin: 0;
	}

	.hint {
		font-size: 14px;
		color: var(--color-neutral, #888);
		margin: 0;
	}

	.hint strong {
		color: var(--color-neutral-active, #e0e0e0);
	}

	.log-path {
		font-size: 11px;
		color: var(--color-neutral, #888);
		word-break: break-all;
		margin: 0;
		font-family: monospace;
	}

	.field-row {
		display: flex;
		gap: 8px;
		align-items: center;
	}

	.text-input,
	.select-input {
		flex: 1;
		background: color-mix(in srgb, var(--color-white, #fff) 6%, transparent);
		border: none;
		color: var(--color-neutral-active, #e0e0e0);
		font-size: 14px;
		padding: 7px 10px;
		outline: none;
	}

	.select-input option {
		background: var(--color-surface, #1e1e2e);
	}

	.save-btn {
		background: var(--color-interactive, #7c6fff);
		color: #fff;
		border: none;
		padding: 7px 14px;
		font-size: 14px;
		font-weight: 500;
		cursor: pointer;
		white-space: nowrap;
		flex-shrink: 0;
	}

	.save-btn:disabled {
		opacity: 0.45;
		cursor: not-allowed;
	}

	.danger-btn {
		background: color-mix(in srgb, #e53935 15%, transparent);
		color: #e57373;
		border: none;
		padding: 7px 14px;
		font-size: 14px;
		font-weight: 500;
		cursor: pointer;
	}

	.danger-btn:disabled {
		opacity: 0.45;
		cursor: not-allowed;
	}

	.feedback {
		font-size: 12px;
		color: var(--color-interactive, #7c6fff);
		margin: 0;
	}

	.feedback.error {
		color: #e57373;
	}

	.model-status-icon {
		font-size: 14px;
		vertical-align: middle;
		margin-right: 4px;
	}

	.model-status-icon.installed {
		color: #66bb6a;
	}

	.progress-bar-track {
		height: 4px;
		background: color-mix(in srgb, var(--color-white, #fff) 10%, transparent);
		width: 100%;
	}

	.progress-bar-fill {
		height: 100%;
		background: var(--color-interactive, #7c6fff);
		transition: width 0.25s linear;
	}

	.progress-detail {
		font-size: 11px;
		color: var(--color-neutral, #888);
		margin: 0;
	}

	.btn-icon {
		font-size: 16px;
		vertical-align: middle;
		margin-right: 4px;
	}
</style>
