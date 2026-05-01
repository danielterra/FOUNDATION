<script>
	import { invoke } from '@tauri-apps/api/core';
	import Button from './Button.svelte';
	import Modal from './Modal.svelte';
	import posthog from 'posthog-js';

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
			<Button icon="close" title="Close" onclick={onClose} />
		</div>

		<div class="panel-body">
			<section class="settings-section">
				<h3 class="section-title">
					<span class="material-symbols-outlined">psychology</span>
					AI Model
				</h3>
				{#if services.length > 0}
					<label class="field-label">Provedor</label>
					<div class="field-row">
						<select class="select-input" value={selectedServiceIri} onchange={onServiceChange}>
							{#each services as svc}
								<option value={svc.iri}>{svc.label}</option>
							{/each}
						</select>
					</div>
					{#if models.length > 0}
						<label class="field-label">Modelo</label>
						<div class="field-row">
							<select class="select-input" bind:value={selectedModelIri}>
								<option value="">Select a model…</option>
								{#each models as model}
									<option value={model.iri}>{model.label}</option>
								{/each}
							</select>
							<Button onclick={saveModel} disabled={modelSaving || !selectedModelIri || (!selectedServiceIsLocal && !apiKey.trim())}>
								{modelSaving ? 'Saving…' : 'Save'}
							</Button>
						</div>
						{#if !selectedServiceIsLocal && !apiKey.trim()}
							<p class="feedback error">Configure a chave de API abaixo antes de ativar este provedor.</p>
						{/if}
					{/if}
					{#if modelMessage}
						<p class="feedback" class:error={modelError}>{modelMessage}</p>
					{/if}

					{#if !selectedServiceIsLocal}
						<label class="field-label">Chave de API</label>
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
								<Button onclick={saveApiKey} disabled={apiKeySaving || !apiKey}>
									{apiKeySaving ? 'Saving…' : 'Save'}
								</Button>
							</div>
							{#if apiKeyMessage}
								<p class="feedback" class:error={apiKeyError}>{apiKeyMessage}</p>
							{/if}
						{/if}
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
						<Button variant="danger" onclick={cancelModelDownload}>Cancelar</Button>
					</div>
				{:else}
					<p class="hint">Modelo não instalado · {localModelSizeHuman}</p>
					<div class="field-row">
						<Button icon="download" onclick={startModelDownload}>Baixar modelo</Button>
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
						<Button size="sm" icon="history" title="Histórico de sincronização" onclick={() => toggleImapHistory(account.iri)} />
						<Button size="sm" icon="edit" title="Editar" onclick={() => startEditImapAccount(account)} />
						<Button size="sm" variant="danger" icon="delete" title="Remover" disabled={imapDeleting} onclick={() => deleteImapAccount(account.iri)} />
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
					<Button icon="add" onclick={startAddImapAccount}>Adicionar conta</Button>
				</div>
			</section>

			<Modal
				open={imapEditing !== null}
				title={imapEditing === 'new' ? 'Nova conta de email' : 'Editar conta de email'}
				icon="mail"
				size="md"
				onClose={cancelImapEdit}
			>
				<div class="form-field">
					<label class="field-label" for="imap-label">Rótulo</label>
					<input id="imap-label" class="text-input" type="text" placeholder="Gmail Pessoal" bind:value={imapForm.label} />
				</div>

				<div class="form-grid">
					<div class="form-field" style="flex:3; min-width:0">
						<label class="field-label" for="imap-host">Servidor IMAP</label>
						<input id="imap-host" class="text-input" type="text" placeholder="imap.gmail.com" bind:value={imapForm.host} />
					</div>
					<div class="form-field" style="flex:1; min-width:80px">
						<label class="field-label" for="imap-port">Porta</label>
						<input id="imap-port" class="text-input" type="number" placeholder="993" bind:value={imapForm.port} min="1" max="65535" />
					</div>
				</div>

				<div class="form-field">
					<label class="field-label" for="imap-username">Usuário</label>
					<input id="imap-username" class="text-input" type="text" placeholder="seu@email.com" bind:value={imapForm.username} autocomplete="off" />
				</div>

				<div class="form-field">
					<label class="field-label" for="imap-password">Senha</label>
					<input
						id="imap-password"
						class="text-input"
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
						<input type="checkbox" bind:checked={imapForm.use_tls} />
						Usar TLS
					</label>
					<label class="checkbox-label" style="margin-left:auto">
						Sincronizar a cada
						<input class="text-input interval-input" type="number" bind:value={imapForm.sync_interval_minutes} min="1" max="1440" />
						min
					</label>
				</div>

				{#if imapAvailableFolders.length > 0}
					<div class="form-field">
						<label class="field-label">Pastas monitoradas</label>
						<div class="imap-folders">
							{#each imapAvailableFolders as folder}
								<label class="checkbox-label folder-label">
									<input type="checkbox" checked={imapSelectedFolders.includes(folder)} onchange={() => toggleImapFolder(folder)} />
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

				{#snippet footer()}
					<Button onclick={cancelImapEdit}>Cancelar</Button>
					<Button onclick={testImapConnection} disabled={imapTesting || !imapForm.host || !imapForm.username || !imapForm.password}>
						{imapTesting ? 'Testando…' : 'Testar conexão'}
					</Button>
					<Button onclick={saveImapAccount} disabled={imapSaving || !imapForm.label || !imapForm.host || !imapForm.username || (!imapHasExistingPassword && !imapForm.password)}>
						{imapSaving ? 'Salvando…' : 'Salvar'}
					</Button>
				{/snippet}
			</Modal>

			<section class="settings-section">
				<h3 class="section-title">
					<span class="material-symbols-outlined">folder_open</span>
					Pasta de Dados
				</h3>
				<p class="hint">Pasta onde o banco de dados do Foundation é armazenado. O app será reiniciado ao salvar.</p>
				<div class="field-row">
					<input
						class="text-input"
						type="text"
						placeholder="/caminho/para/pasta"
						bind:value={foundationDir}
					/>
					<Button icon="folder_open" title="Selecionar pasta" onclick={browseFoundationDir} />
				</div>
				<div class="field-row">
					<Button
						icon="restart_alt"
						onclick={saveFoundationDir}
						disabled={foundationDirSaving || !foundationDir.trim()}
					>
						{foundationDirSaving ? 'Salvando…' : 'Salvar e reiniciar'}
					</Button>
				</div>
				{#if foundationDirMessage}
					<p class="feedback" class:error={foundationDirError}>{foundationDirMessage}</p>
				{/if}
			</section>

			<section class="settings-section">
				<h3 class="section-title">
					<span class="material-symbols-outlined">shield</span>
					Privacidade
				</h3>
				<label class="analytics-toggle">
					<input
						type="checkbox"
						checked={analyticsEnabled}
						onchange={(e) => toggleAnalytics(e.target.checked)}
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
					<Button
						icon="refresh"
						onclick={async () => { await invoke('setup__reset'); }}
					>
						Refazer setup inicial
					</Button>
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
					<Button variant="danger" onclick={clearLogs} disabled={logClearing}>
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
		border-radius: var(--radius-lg);
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

	/* .section-title and .field-label come from global base.css */

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
		background: var(--color-surface-3);
		border: none;
		border-radius: var(--radius-sm);
		color: var(--color-neutral-active, #e0e0e0);
		font-family: var(--font-body);
		font-size: 14px;
		padding: 7px 10px;
		line-height: 1.4;
		outline: none;
		box-sizing: border-box;
	}

	.select-input {
		appearance: none;
		-webkit-appearance: none;
		-moz-appearance: none;
		padding-right: 32px;
		background-image: url("data:image/svg+xml;utf8,<svg xmlns='http://www.w3.org/2000/svg' width='12' height='12' viewBox='0 0 24 24' fill='none' stroke='%23bfbfbf' stroke-width='2.5' stroke-linecap='round' stroke-linejoin='round'><polyline points='6 9 12 15 18 9'/></svg>");
		background-repeat: no-repeat;
		background-position: right 10px center;
		cursor: pointer;
	}

	.select-input option {
		background: var(--color-surface-2);
		color: var(--color-neutral-active);
	}

	/* save-btn / danger-btn replaced by <Button> component */

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

	.imap-account-row {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 12px;
		background: var(--color-surface-2);
		border-radius: var(--radius-sm);
		margin-bottom: 6px;
	}

	.imap-status-icon {
		font-size: 16px;
		color: var(--color-neutral, #888);
		flex-shrink: 0;
	}

	.imap-status-icon.connected {
		color: #66bb6a;
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
		color: var(--color-text-secondary, #aaa);
	}

	.sync-table th {
		font-weight: 600;
		color: var(--color-text-muted, #888);
	}

	.sync-table .sync-error {
		color: var(--color-error, #e57373);
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
		color: var(--color-neutral-active, #e0e0e0);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.imap-account-host {
		font-size: 11px;
		color: var(--color-neutral, #888);
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
		color: var(--color-neutral, #888);
		cursor: pointer;
	}

	.interval-input {
		flex: none;
		width: 52px;
		display: inline-block;
		padding: 4px 6px;
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

	.analytics-toggle {
		display: flex;
		align-items: flex-start;
		gap: 10px;
		padding: 10px 12px;
		background: var(--color-surface-2);
		border-radius: var(--radius-sm);
		cursor: pointer;
	}

	.analytics-toggle input[type="checkbox"] {
		margin-top: 2px;
		flex-shrink: 0;
		accent-color: var(--color-interactive);
		width: 14px;
		height: 14px;
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
</style>
