<script>
	import { onDestroy, onMount } from 'svelte';
	import { invoke } from '@tauri-apps/api/core';
	import { listen } from '@tauri-apps/api/event';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import * as Dialog from '$lib/components/ui/dialog';
	import chatgptLogo from '$lib/assets/providers/chatgpt-logo-white.svg';

	let {
		serviceIri = '',
		selectedModelId = '',
		selectedModelName = '',
		onSelectModel,
		onAcknowledgePlanUse,
	} = $props();

	let connection = $state({
		status: 'loading',
		email: null,
		planUsageEnabled: false,
		expiresAt: null,
		error: null,
		showWelcomeConfirmation: false,
	});
	let authSessionId = $state('');
	let authStage = $state('');
	let authStarting = $state(false);
	let authCancelling = $state(false);
	let disconnecting = $state(false);
	let statusError = $state('');
	let models = $state([]);
	let modelsLoading = $state(false);
	let modelsError = $state('');
	let modelPickerOpen = $state(false);
	let modelSearch = $state('');
	let draftModelId = $state('');
	let modelSaving = $state(false);
	let modelSaveError = $state('');
	let welcomeOpen = $state(false);
	let pollingTimer;
	let statusRefreshSequence = 0;
	let modelLoadSequence = 0;
	let unlistenAuthStatus;
	let destroyed = false;

	let selectedModel = $derived(models.find((model) => model.iri === selectedModelId) ?? null);
	const authStages = [
		{ id: 'waiting_for_callback', label: 'Aguardando navegador' },
		{ id: 'callback_received', label: 'Retorno recebido' },
		{ id: 'exchanging_code', label: 'Validando autorização' },
		{ id: 'validating_identity', label: 'Validando conta' },
		{ id: 'saving_credentials', label: 'Salvando conexão' },
	];
	let authStageIndex = $derived(authStages.findIndex((stage) => stage.id === authStage));
	let filteredModels = $derived.by(() => {
		const query = modelSearch.trim().toLocaleLowerCase();
		return [...models]
			.filter((model) => `${model.label} ${model.modelIdentifier} ${model.ownedBy ?? ''}`.toLocaleLowerCase().includes(query))
			.sort((first, second) => first.label.localeCompare(second.label));
	});
	let usageLimitReached = $derived(
		String(connection.error ?? modelsError).toLowerCase().includes('subscription_sharing_usage_limit_exceeded')
			|| String(connection.error ?? modelsError).toLowerCase().includes('usage_limit'),
	);

	onMount(async () => {
		unlistenAuthStatus = await listen('chatgpt-auth-status', (event) => {
			void handleAuthStatus(event.payload);
		});
		if (destroyed) {
			unlistenAuthStatus();
			return;
		}
		await refreshConnection();
	});

	onDestroy(() => {
		destroyed = true;
		clearTimeout(pollingTimer);
		unlistenAuthStatus?.();
	});

	async function refreshConnection() {
		const refreshSequence = ++statusRefreshSequence;
		try {
			const next = await invoke('ai__chatgpt_get_connection_status');
			if (destroyed || refreshSequence !== statusRefreshSequence) return;
			if (next.authSessionId && next.status === 'authorizing') {
				authSessionId = next.authSessionId;
				authStage = next.stage ?? 'waiting_for_callback';
				connection = { ...connection, ...next, status: 'authorizing', error: null };
				statusError = '';
				scheduleStatusPoll();
				return;
			}

			clearTimeout(pollingTimer);
			authSessionId = '';
			authStage = next.status === 'connected' ? 'connected' : '';
			connection = { ...connection, ...next };
			statusError = next.status === 'error' ? (next.error ?? 'Não foi possível confirmar a conexão com ChatGPT.') : '';
			if (next.status === 'connected' && next.planUsageEnabled) {
				welcomeOpen = Boolean(next.showWelcomeConfirmation);
				void loadModels();
			} else {
				modelLoadSequence += 1;
				models = [];
				modelsError = '';
			}
		} catch (error) {
			if (!destroyed && refreshSequence === statusRefreshSequence) {
				connection = { ...connection, status: 'error' };
				statusError = String(error);
			}
		} finally {
			if (!destroyed && refreshSequence === statusRefreshSequence) scheduleStatusPoll();
		}
	}

	function scheduleStatusPoll() {
		clearTimeout(pollingTimer);
		if (!authSessionId || connection.status !== 'authorizing') return;
		pollingTimer = setTimeout(refreshConnection, 2000);
	}

	function authStageLabel(stage) {
		const labels = {
			waiting_for_callback: 'Aguardando retorno do navegador',
			callback_received: 'Retorno recebido',
			exchanging_code: 'Concluindo autorização segura',
			validating_identity: 'Validando sua conta',
			saving_credentials: 'Salvando a conexão',
			browser_open: 'Não foi possível abrir o navegador',
			connected: 'Conta conectada',
			cancelled: 'Autorização cancelada',
		};
		return labels[stage] ?? 'Conectando à conta ChatGPT';
	}

	async function handleAuthStatus(payload) {
		if (destroyed || !payload) return;
		// Events may be delayed or replayed. Reconcile with the backend's canonical
		// session/credential state instead of allowing an event ID to own the UI.
		await refreshConnection();
	}

	async function loadModels() {
		const loadSequence = ++modelLoadSequence;
		modelsLoading = true;
		modelsError = '';
		try {
			const result = await invoke('ai__chatgpt_list_models', { serviceIri });
			if (!destroyed && loadSequence === modelLoadSequence) models = result.models ?? [];
		} catch (error) {
			if (!destroyed && loadSequence === modelLoadSequence) {
				models = [];
				modelsError = String(error);
			}
		} finally {
			if (!destroyed && loadSequence === modelLoadSequence) modelsLoading = false;
		}
	}

	async function startAuthorization() {
		if (authStarting || authCancelling || connection.status === 'authorizing') return;
		statusError = '';
		authStarting = true;
		authStage = 'waiting_for_callback';
		connection = { ...connection, status: 'authorizing', error: null };
		try {
			await invoke('ai__chatgpt_start_auth');
			await refreshConnection();
		} catch (error) {
			const message = String(error);
			await refreshConnection();
			if (connection.status !== 'connected' && connection.status !== 'authorizing') {
				statusError = message;
				connection = { ...connection, status: 'error', error: message };
				authSessionId = '';
			}
		} finally {
			authStarting = false;
		}
	}

	async function cancelAuthorization() {
		if (!authSessionId || authCancelling) return;
		authCancelling = true;
		try {
			await invoke('ai__chatgpt_cancel_auth', { authSessionId });
			await refreshConnection();
		} catch (error) {
			statusError = String(error);
		} finally {
			authCancelling = false;
		}
	}

	async function disconnect() {
		if (disconnecting) return;
		disconnecting = true;
		statusError = '';
		try {
			await invoke('ai__chatgpt_disconnect');
			connection = { ...connection, status: 'disconnected', email: null, planUsageEnabled: false, expiresAt: null, error: null };
			models = [];
			welcomeOpen = false;
		} catch (error) {
			statusError = String(error);
		} finally {
			disconnecting = false;
		}
	}

	function openModelPicker() {
		draftModelId = selectedModelId;
		modelSearch = '';
		modelSaveError = '';
		modelPickerOpen = true;
	}

	async function saveSelectedModel() {
		if (!draftModelId || !onSelectModel || modelSaving) return;
		modelSaving = true;
		modelSaveError = '';
		try {
			await onSelectModel(draftModelId);
			modelPickerOpen = false;
		} catch (error) {
			modelSaveError = String(error);
		} finally {
			modelSaving = false;
		}
	}

	async function acknowledgePlanUse() {
		if (!onAcknowledgePlanUse) return;
		try {
			await onAcknowledgePlanUse();
			connection = { ...connection, showWelcomeConfirmation: false };
			welcomeOpen = false;
		} catch (error) {
			statusError = String(error);
		}
	}

	function formattedExpiry(value) {
		if (!value) return '';
		const date = new Date(value);
		return Number.isNaN(date.getTime()) ? '' : date.toLocaleString();
	}

	function modelValue(model) {
		return model.iri;
	}
</script>

<section class="chatgpt-settings" aria-labelledby="chatgpt-settings-title">
	<header class="chatgpt-heading">
		<h4 id="chatgpt-settings-title">Usar a conta ChatGPT</h4>
		<p>Conecte sua conta para usar a franquia elegível do plano em solicitações compatíveis. A OpenAI controla a elegibilidade e os limites da conta; isso é separado da cobrança da API.</p>
	</header>

	{#if connection.status === 'loading'}
		<p class="chatgpt-status" role="status" aria-live="polite">Verificando conexão com ChatGPT…</p>
	{:else if connection.status === 'authorizing'}
		<div class="chatgpt-state" role="status" aria-live="polite">
			<span class="material-symbols-outlined" aria-hidden="true">open_in_browser</span>
			<div>
				<strong>{authStageLabel(authStage)}</strong>
				<p>O Foundation aguarda o retorno da OpenAI. Nenhuma solicitação será enviada até a conexão terminar.</p>
				<ol class="auth-progress" aria-label="Etapas da autorização">
					{#each authStages as stage, index (stage.id)}
						<li class:complete={authStageIndex > index} class:current={authStageIndex === index} aria-current={authStageIndex === index ? 'step' : undefined}>
							<span aria-hidden="true">{authStageIndex > index ? '✓' : index + 1}</span>
							{stage.label}
						</li>
					{/each}
				</ol>
			</div>
			<Button variant="ghost" onclick={cancelAuthorization} disabled={authCancelling || !authSessionId}>
				{authCancelling ? 'Cancelando…' : 'Cancelar'}
			</Button>
		</div>
	{:else if connection.status === 'connected'}
		<div class="chatgpt-state connected" role="status" aria-live="polite">
			<span class="material-symbols-outlined" aria-hidden="true">account_circle</span>
			<div class="connection-copy">
				<strong>{connection.email || 'Conta ChatGPT conectada'}</strong>
				<p>{connection.planUsageEnabled ? 'A permissão para usar o plano ChatGPT está ativa.' : 'A conta está conectada, mas não autorizou o uso do plano ChatGPT.'}</p>
				{#if connection.expiresAt && !connection.planUsageEnabled}
					<span class="connection-expiry semantic-metadata">Autorização expira em {formattedExpiry(connection.expiresAt)}</span>
				{/if}
			</div>
			{#if connection.planUsageEnabled}
				<a class="manage-usage" href="https://chatgpt.com/settings/usage" target="_blank" rel="noopener noreferrer">
					<span class="material-symbols-outlined" aria-hidden="true">open_in_new</span>
					Manage usage
				</a>
			{:else}
				<Button variant="default" onclick={startAuthorization} disabled={authStarting || connection.status === 'authorizing'} data-oai-siwc-button>
					<img src={chatgptLogo} alt="" aria-hidden="true" />
					{authStarting ? 'Abrindo autorização…' : 'Continue with ChatGPT'}
				</Button>
			{/if}
			<Button variant="destructive" onclick={disconnect} disabled={disconnecting}>
				<span class="material-symbols-outlined" aria-hidden="true">link_off</span>
				{disconnecting ? 'Desconectando…' : 'Desconectar'}
			</Button>
		</div>
	{:else}
		<div class="chatgpt-disconnected">
			<p>{connection.status === 'error' ? (statusError || connection.error || 'A autorização falhou. Tente novamente.') : 'Nenhuma conta ChatGPT conectada.'}</p>
			<Button variant="default" onclick={startAuthorization} disabled={authStarting || connection.status === 'authorizing'} data-oai-siwc-button>
				<img src={chatgptLogo} alt="" aria-hidden="true" />
				{authStarting ? 'Abrindo autorização…' : 'Continue with ChatGPT'}
			</Button>
		</div>
	{/if}

	{#if connection.status === 'connected' && connection.planUsageEnabled}
		<div class="chatgpt-models" aria-labelledby="chatgpt-model-heading">
			<div class="chatgpt-model-heading">
				<div>
					<h5 id="chatgpt-model-heading">Modelo ChatGPT</h5>
					<p>Disponibilidade determinada pela conta conectada.</p>
				</div>
				{#if !modelsLoading && models.length > 0}
					<Button variant="outline" onclick={openModelPicker} disabled={!onSelectModel}>
						<span class="material-symbols-outlined" aria-hidden="true">search</span>
						{selectedModel ? 'Alterar modelo' : 'Escolher modelo'}
					</Button>
				{/if}
			</div>
			{#if modelsLoading}
				<p class="chatgpt-status" role="status" aria-live="polite">Carregando modelos disponíveis…</p>
			{:else if modelsError}
				{#if usageLimitReached}
					<div class="chatgpt-limit" role="alert">
						<strong>Limite de uso atingido</strong>
						<p>Revise os limites da sua conta ChatGPT antes de continuar.</p>
						<a href="https://chatgpt.com/settings/usage" target="_blank" rel="noopener noreferrer">Manage usage</a>
					</div>
				{:else}
					<p class="chatgpt-error" role="alert">Não foi possível carregar os modelos: {modelsError}</p>
				{/if}
				<Button variant="ghost" onclick={loadModels}>Tentar novamente</Button>
			{:else if models.length === 0}
				<p class="chatgpt-status">A OpenAI não retornou modelos disponíveis para esta conta.</p>
			{:else if selectedModel}
				<div class="selected-chatgpt-model">
					<strong>{selectedModel.label}</strong>
					{#if selectedModel.ownedBy}<span>{selectedModel.ownedBy}</span>{/if}
				</div>
			{:else if selectedModelId}
				<div class="chatgpt-error" role="status">
					O modelo configurado{selectedModelName ? ` (${selectedModelName})` : ''} não está disponível nesta conta. Escolha explicitamente outro modelo para continuar.
				</div>
			{:else}
				<p class="chatgpt-status">Escolha um modelo para começar a usar a rota ChatGPT.</p>
			{/if}
		</div>
	{/if}

	{#if statusError && connection.status !== 'error'}
		<p class="chatgpt-error" role="alert" aria-live="assertive">{statusError}</p>
	{/if}

	<Dialog.Root open={modelPickerOpen} onOpenChange={(open) => { if (!open && !modelSaving) modelPickerOpen = false; }}>
		<Dialog.Content class="chatgpt-model-dialog" aria-describedby="chatgpt-model-description">
			<Dialog.Header>
				<Dialog.Title>Escolher modelo ChatGPT</Dialog.Title>
				<Dialog.Description id="chatgpt-model-description">Busque entre os modelos disponíveis para a conta conectada. A escolha será salva nesta rota.</Dialog.Description>
			</Dialog.Header>
			<Input id="chatgpt-model-search" type="search" aria-label="Buscar modelos ChatGPT" placeholder="Buscar modelo" bind:value={modelSearch} />
			{#if filteredModels.length > 0}
				<div class="chatgpt-model-list" role="radiogroup" aria-label="Modelos ChatGPT disponíveis">
			{#each filteredModels as model (model.iri)}
						<button
							type="button"
							class="chatgpt-model-option"
							class:selected={draftModelId === modelValue(model)}
							role="radio"
							aria-checked={draftModelId === modelValue(model)}
							onclick={() => { draftModelId = modelValue(model); }}
						>
							<span>
							<strong>{model.label}</strong>
							<small>{model.ownedBy || model.modelIdentifier}</small>
							</span>
							{#if draftModelId === modelValue(model)}<span class="material-symbols-outlined" aria-hidden="true">check_circle</span>{/if}
						</button>
					{/each}
				</div>
			{:else}
				<p class="chatgpt-status" role="status">Nenhum modelo corresponde à busca.</p>
			{/if}
			{#if modelSaveError}<p class="chatgpt-error" role="alert">{modelSaveError}</p>{/if}
			<Dialog.Footer>
				<Button variant="ghost" onclick={() => { modelPickerOpen = false; }} disabled={modelSaving}>Cancelar</Button>
				<Button variant="default" onclick={saveSelectedModel} disabled={modelSaving || !draftModelId || !onSelectModel}>
					{modelSaving ? 'Salvando…' : 'Usar modelo'}
				</Button>
			</Dialog.Footer>
		</Dialog.Content>
	</Dialog.Root>

	<Dialog.Root open={welcomeOpen} onOpenChange={(open) => { if (!open) welcomeOpen = true; }}>
		<Dialog.Content class="chatgpt-welcome-dialog" aria-describedby="chatgpt-welcome-description">
			<Dialog.Header>
				<Dialog.Title>Você está usando seu plano ChatGPT</Dialog.Title>
				<Dialog.Description id="chatgpt-welcome-description">Solicitações elegíveis neste aplicativo usam seu plano ChatGPT. Você pode revisar e gerenciar esse uso nas configurações do ChatGPT.</Dialog.Description>
			</Dialog.Header>
			<a class="manage-usage" href="https://chatgpt.com/settings/usage" target="_blank" rel="noopener noreferrer">Manage usage</a>
			<Dialog.Footer>
				<Button variant="default" onclick={acknowledgePlanUse} disabled={!onAcknowledgePlanUse}>Entendi</Button>
			</Dialog.Footer>
		</Dialog.Content>
	</Dialog.Root>
</section>

<style>
	.chatgpt-settings { display: grid; gap: var(--spacing-field-group); min-width: 0; }
	.chatgpt-heading h4, .chatgpt-heading p, .chatgpt-model-heading h5, .chatgpt-model-heading p, .chatgpt-disconnected p, .chatgpt-status, .chatgpt-state p, .chatgpt-limit p { margin: 0; }
	.chatgpt-heading h4, .chatgpt-model-heading h5 { color: var(--text-primary); font-size: var(--type-body-prominent); font-weight: 400; }
	.chatgpt-heading p, .chatgpt-model-heading p, .chatgpt-disconnected p, .chatgpt-status, .chatgpt-state p, .chatgpt-limit p { color: var(--text-secondary); font-size: var(--type-body-compact); line-height: var(--line-height-body); }
	.chatgpt-heading { display: grid; gap: var(--spacing-label-to-value); }
	.chatgpt-state, .chatgpt-disconnected, .chatgpt-models { display: grid; gap: var(--spacing-field-group); min-width: 0; padding: var(--spacing-widget-inset); border: var(--size-hud-frame-line) solid var(--color-hud-line-subtle); border-radius: var(--radius); background: var(--color-surface-2); }
	.chatgpt-state { grid-template-columns: auto minmax(0, 1fr) auto; align-items: center; }
	.chatgpt-state > .material-symbols-outlined { color: var(--color-success); font-size: var(--type-icon-medium); }
	.chatgpt-state strong, .selected-chatgpt-model strong { color: var(--text-primary); font-size: var(--type-body-prominent); font-weight: 400; overflow-wrap: anywhere; }
	.auth-progress { display: grid; grid-template-columns: repeat(auto-fit, minmax(9rem, 1fr)); gap: var(--spacing-control-internal); margin: var(--spacing-field-group) 0 0; padding: 0; list-style: none; }
	.auth-progress li { display: flex; align-items: center; gap: var(--spacing-inline-tight); color: var(--text-secondary); font-size: var(--type-caption); }
	.auth-progress li > span { display: grid; place-items: center; flex: 0 0 var(--type-icon-medium); width: var(--type-icon-medium); height: var(--type-icon-medium); border: var(--size-hud-frame-line) solid var(--color-hud-line-subtle); border-radius: 50%; color: var(--text-secondary); }
	.auth-progress li.complete > span { border-color: var(--color-success); color: var(--color-success); }
	.auth-progress li.current, .auth-progress li.current > span { color: var(--color-transition); }
	.auth-progress li.current > span { border-color: var(--color-transition); }
	.connection-copy { display: grid; gap: var(--spacing-label-to-value); min-width: 0; }
	.chatgpt-disconnected { grid-template-columns: minmax(0, 1fr) auto; align-items: center; }
	.chatgpt-disconnected :global(button img) { width: var(--type-icon-medium); height: var(--type-icon-medium); }
	.chatgpt-model-heading { display: flex; align-items: center; justify-content: space-between; gap: var(--spacing-field-group); }
	.chatgpt-model-heading > div { display: grid; gap: var(--spacing-label-to-value); }
	.selected-chatgpt-model { display: grid; gap: var(--spacing-label-to-value); }
	.selected-chatgpt-model span, .chatgpt-model-option small { color: var(--text-secondary); font-size: var(--type-caption); overflow-wrap: anywhere; }
	:global(.chatgpt-model-dialog) { max-width: 52rem; }
	.chatgpt-model-list { display: grid; gap: var(--spacing-control-internal); max-height: 52vh; overflow-y: auto; }
	.chatgpt-model-option { display: flex; align-items: center; justify-content: space-between; gap: var(--spacing-field-group); width: 100%; padding: var(--spacing-control-internal); border: var(--size-hud-frame-line) solid var(--color-hud-line-subtle); border-radius: var(--radius); color: var(--text-body); background: var(--color-surface-1); text-align: left; cursor: pointer; }
	.chatgpt-model-option:hover, .chatgpt-model-option.selected { border-color: var(--color-selection); background: var(--color-selection-subtle); }
	.chatgpt-model-option:focus-visible { outline: var(--size-hud-frame-line) solid var(--color-selection); outline-offset: var(--spacing-inline-tight); }
	.chatgpt-model-option > span:first-child { display: grid; gap: var(--spacing-label-to-value); min-width: 0; }
	.chatgpt-model-option strong { color: var(--text-primary); font-size: var(--type-body-prominent); font-weight: 400; overflow-wrap: anywhere; }
	.chatgpt-model-option > .material-symbols-outlined { flex: 0 0 auto; color: var(--color-selection); }
	.manage-usage { display: inline-flex; align-items: center; gap: var(--spacing-inline-tight); width: fit-content; color: var(--text-accent); font-size: var(--type-body-compact); text-decoration: underline; text-underline-offset: var(--spacing-inline-tight); }
	.manage-usage .material-symbols-outlined { font-size: var(--type-icon-small); }
	.chatgpt-limit { display: grid; gap: var(--spacing-label-to-value); padding: var(--spacing-control-internal); border-inline-start: var(--size-hud-frame-line) solid var(--color-warning); background: var(--color-surface-1); }
	.chatgpt-limit strong { color: var(--text-primary); font-size: var(--type-body-prominent); font-weight: 400; }
	.chatgpt-limit a { width: fit-content; color: var(--text-accent); font-size: var(--type-body-compact); font-weight: 600; text-decoration: underline; cursor: pointer; }
	.chatgpt-error { margin: 0; color: var(--color-error); font-size: var(--type-body-compact); }
	@media (max-width: 48rem) {
		.chatgpt-state { grid-template-columns: auto minmax(0, 1fr); }
		.chatgpt-state :global(button), .chatgpt-state .manage-usage { grid-column: 2; }
		.chatgpt-disconnected { grid-template-columns: minmax(0, 1fr); }
		.chatgpt-model-heading { align-items: flex-start; flex-direction: column; }
	}
</style>
