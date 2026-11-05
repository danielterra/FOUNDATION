<script lang="ts">
	import { onMount } from 'svelte';
	import { invoke } from '@tauri-apps/api/core';
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import { Checkbox } from '$lib/components/ui/checkbox';
	import * as Select from '$lib/components/ui/select';
	import OpenRouterModelSelector from '$lib/components/OpenRouterModelSelector.svelte';
	import { aiServiceDisplayName } from '$lib/utils/aiServiceDisplayName';

	type EmailProvider = {
		id: string;
		label: string;
		icon: string;
		host: string;
		port: number;
		use_tls: boolean;
		passwordLabel: string;
		passwordHint: string;
		instructions: string[];
		disabled?: boolean;
	};

	type EmailTestCredentials = {
		host: string;
		port: number;
		useTls: boolean;
		username: string;
		password: string;
	};

	type EmailTestResult = {
		message: string;
		verificationToken: string;
	};

	type AIService = {
		iri: string;
		label: string;
		description: string;
		isLocal: boolean;
		seeAlsoUrl?: string | null;
	};

	type AIModel = {
		iri: string;
		label: string;
		modelIdentifier: string | null;
		modelVersion: string | null;
		description?: string | null;
		isDefault?: boolean;
	};

	type AIModelPage = {
		items: AIModel[];
		next_cursor: number | null;
		has_more: boolean;
		snapshot_tx: number;
	};

	type OpenRouterModel = {
		id: string;
		name: string;
		publisher?: string | null;
		description?: string;
		recommendedUse?: string | null;
		recommendedUseStatus?: 'provided' | 'not_provided';
		contextLength: number | null;
		inputCostPerToken: number | null;
		outputCostPerToken: number | null;
		parameterCountTotal?: number | null;
		parameterCountActive?: number | null;
		parameterCountStatus?: 'reported' | 'not_provided_or_ambiguous';
		supportedParameters?: string[] | null;
		supportsTools?: boolean | null;
		inputModalities?: string[] | null;
		outputModalities?: string[] | null;
	};
	type OpenRouterBenchmark = { score: number; dimension: string; source: string; asOf: string };
	type OpenRouterBenchmarks = { available: boolean; availableDimensions: string[]; source: string; sourceUrl: string | null; citation: string | null; asOf: string | null; models: { modelId: string; benchmarks: Record<string, OpenRouterBenchmark> }[]; ambiguousModelIds: string[] };
	let { onComplete } = $props<{ onComplete?: () => void }>();

	let step = $state(1);
	let isSubmitting = $state(false);
	let submitError = $state('');

	let personName = $state('');
	let nameInputEl = $state<HTMLInputElement | null>(null);

	function focusNameInput() {
		nameInputEl?.focus();
	}

	onMount(focusNameInput);

	const EMAIL_PROVIDERS: EmailProvider[] = [
		{
			id: 'icloud',
			label: 'iCloud Mail',
			icon: 'cloud',
			host: 'imap.mail.me.com',
			port: 993,
			use_tls: true,
			passwordLabel: 'Senha de app',
			passwordHint: 'Gere em appleid.apple.com → Senhas de app',
			instructions: [
				'Acesse appleid.apple.com e entre na sua conta.',
				'Vá em "Iniciar sessão e segurança" → "Senhas de app".',
				'Clique em "Gerar uma senha de app" e dê o nome "FOUNDATION".',
				'Copie a senha gerada e cole no campo abaixo.',
				'Use seu endereço @icloud.com como nome de usuário.',
			],
		},
		{
			id: 'outlook',
			label: 'Outlook / Hotmail',
			icon: 'mail',
			host: 'outlook.office365.com',
			port: 993,
			use_tls: true,
			passwordLabel: 'Senha da conta Microsoft',
			passwordHint: 'A mesma senha usada para entrar em outlook.com',
			instructions: [
				'Use seu endereço @outlook.com ou @hotmail.com como nome de usuário.',
				'Use a senha da sua conta Microsoft.',
				'Se tiver autenticação em dois fatores ativada, gere uma senha de app em account.microsoft.com → Segurança.',
			],
		},
		{
			id: 'yahoo',
			label: 'Yahoo Mail',
			icon: 'alternate_email',
			host: 'imap.mail.yahoo.com',
			port: 993,
			use_tls: true,
			passwordLabel: 'Senha de app',
			passwordHint: 'Gere em login.yahoo.com → Segurança da conta → Senhas de app',
			instructions: [
				'Acesse login.yahoo.com e vá em "Segurança da conta".',
				'Clique em "Gerar senha de app" e selecione "Outro aplicativo".',
				'Nomeie como "FOUNDATION" e copie a senha gerada.',
				'Use seu endereço @yahoo.com como nome de usuário.',
			],
		},
		{
			id: 'custom',
			label: 'Outro servidor IMAP',
			icon: 'dns',
			host: '',
			port: 993,
			use_tls: true,
			passwordLabel: 'Senha',
			passwordHint: '',
			instructions: [],
		},
		{
			id: 'gmail',
			label: 'Gmail',
			icon: 'mail',
			disabled: true,
			host: '',
			port: 993,
			use_tls: true,
			passwordLabel: '',
			passwordHint: '',
			instructions: [],
		},
	];

	let selectedEmailProviderId = $state('');
	let emailUsername = $state('');
	let emailPassword = $state('');
	let emailHost = $state('');
	let emailPort = $state(993);
	let emailUseTls = $state(true);
	let emailTesting = $state(false);
	let emailTestResult = $state<'ok' | 'error' | null>(null);
	let emailTestMessage = $state('');
	let emailVerificationToken = $state<string | null>(null);
	let emailTestRequestId = 0;
	let showInstructions = $state(false);
	let savedEmailAccountIri = $state<string | null>(null);

	function getSelectedEmailProvider(): EmailProvider | null {
		for (const provider of EMAIL_PROVIDERS) {
			if (provider.id === selectedEmailProviderId) return provider;
		}
		return null;
	}

	let selectedEmailProvider = $derived(getSelectedEmailProvider());

	function selectEmailProvider(provider: EmailProvider) {
		if (provider.disabled) return;
		selectedEmailProviderId = provider.id;
		invalidateEmailTest();
		emailTestResult = null;
		emailTestMessage = '';
		showInstructions = false;
		if (provider.id !== 'custom') {
			emailHost = provider.host;
			emailPort = provider.port;
			emailUseTls = provider.use_tls;
		} else {
			emailHost = '';
			emailPort = 993;
			emailUseTls = true;
		}
	}

	function handleEmailProviderClick(event: MouseEvent) {
		const target = event.currentTarget;
		if (!(target instanceof HTMLElement)) return;
		const providerId = target.dataset.emailProviderId;
		let provider: EmailProvider | null = null;
		for (const item of EMAIL_PROVIDERS) {
			if (item.id === providerId) provider = item;
		}
		if (provider) selectEmailProvider(provider);
	}

	function invalidateEmailTest() {
		emailTestRequestId += 1;
		emailTesting = false;
		emailTestResult = null;
		emailTestMessage = '';
		emailVerificationToken = null;
	}

	function handleEmailTlsChange() {
		invalidateEmailTest();
	}

	function emailCredentialsMatch(credentials: EmailTestCredentials): boolean {
		return credentials.host === emailHost &&
			credentials.port === emailPort &&
			credentials.useTls === emailUseTls &&
			credentials.username === emailUsername &&
			credentials.password === emailPassword;
	}

	function handleNameKeydown(event: KeyboardEvent) {
		if (event.key === 'Enter' && personName.trim()) goToEmailStep();
	}

	function goToEmailStep() {
		step = 2;
	}

	function goToNameStep() {
		step = 1;
	}

	function toggleEmailInstructions() {
		showInstructions = !showInstructions;
	}

	async function testEmailConnection() {
		if (!emailUsername.trim() || !emailPassword.trim() || !emailHost.trim()) return;
		const testedCredentials: EmailTestCredentials = {
			host: emailHost,
			port: emailPort,
			useTls: emailUseTls,
			username: emailUsername,
			password: emailPassword,
		};
		const requestId = ++emailTestRequestId;
		emailTesting = true;
		emailTestResult = null;
		emailTestMessage = '';
		try {
			const result = await invoke<EmailTestResult>('imap__test_connection', testedCredentials);
			if (requestId !== emailTestRequestId || !emailCredentialsMatch(testedCredentials)) return;
			emailTestResult = 'ok';
			emailTestMessage = result.message;
			emailVerificationToken = result.verificationToken;
		} catch (e) {
			if (requestId !== emailTestRequestId || !emailCredentialsMatch(testedCredentials)) return;
			emailTestResult = 'error';
			emailTestMessage = String(e);
		} finally {
			if (requestId === emailTestRequestId) emailTesting = false;
		}
	}

	let emailValid = $derived(
		selectedEmailProviderId !== '' &&
		emailUsername.trim() !== '' &&
		emailPassword.trim() !== '' &&
		emailHost.trim() !== '' &&
		emailTestResult === 'ok' &&
		emailVerificationToken !== null
	);

	let aiChoice = $state<'chat' | 'mcp' | ''>('');
	let apiKey = $state('');
	let showApiKey = $state(false);
	let apiBaseUrl = $state('');
	let aiServices = $state<AIService[]>([]);
	let aiModels = $state<AIModel[]>([]);
	let openRouterModels = $state<OpenRouterModel[]>([]);
	let openRouterBenchmarks = $state<OpenRouterBenchmarks | null>(null);
	let openRouterBenchmarksLoading = $state(false);
	let openRouterBenchmarksError = $state('');
	let selectedAIServiceIri = $state('');
	let selectedAIModelIri = $state('');
	let selectedOpenRouterModelId = $state('');
	let aiServicesLoading = $state(false);
	let aiModelsLoading = $state(false);
	let openRouterModelsLoading = $state(false);
	let openRouterModelsError = $state('');
	let aiServicesLoaded = $state(false);
	let aiConfigError = $state('');
	let aiServicesRequestId = 0;
	let aiModelsRequestId = 0;
	let openRouterModelsRequestId = 0;

	function getSelectedAIService(): AIService | null {
		for (const service of aiServices) {
			if (service.iri === selectedAIServiceIri) return service;
		}
		return null;
	}

	function getSelectedAIModel(): AIModel | null {
		for (const model of aiModels) {
			if (model.iri === selectedAIModelIri) return model;
		}
		return null;
	}

	function getSelectedOpenRouterModel(): OpenRouterModel | null {
		for (const model of openRouterModels) {
			if (model.id === selectedOpenRouterModelId) return model;
		}
		return null;
	}

	let selectedAIService = $derived(getSelectedAIService());
	let selectedAIModel = $derived(getSelectedAIModel());
	let selectedOpenRouterModel = $derived(getSelectedOpenRouterModel());
	let isOpenRouterService = $derived(
		selectedAIService ? aiServiceDisplayName(selectedAIService).toLowerCase().includes('openrouter') : false
	);
	let requiresAIKey = $derived(
		selectedAIService !== null && (!selectedAIService.isLocal || isOpenRouterService)
	);
	let aiModelSelected = $derived(
		isOpenRouterService ? selectedOpenRouterModel !== null : selectedAIModel !== null
	);
	let aiChatConfigurationValid = $derived(
		selectedAIService !== null &&
		aiModelSelected &&
		(!requiresAIKey || apiKey.trim() !== '') &&
		(!isOpenRouterService || apiBaseUrl.trim() !== '')
	);
	let aiChoiceValid = $derived(
		aiChoice === 'mcp' || (aiChoice === 'chat' && aiChatConfigurationValid)
	);

	async function loadAIServices() {
		const requestId = ++aiServicesRequestId;
		aiServicesLoading = true;
		aiConfigError = '';
		try {
			const services = await invoke<AIService[]>('setup__list_ai_services');
			if (requestId !== aiServicesRequestId) return;
			aiServices = services;
			aiServicesLoaded = true;
		} catch (error) {
			if (requestId !== aiServicesRequestId) return;
			aiConfigError = String(error);
		} finally {
			if (requestId === aiServicesRequestId) aiServicesLoading = false;
		}
	}

	async function goToAIChoiceStep() {
		step = 3;
		if (!aiServicesLoaded) await loadAIServices();
	}

	function chooseChatMode() {
		aiChoice = 'chat';
	}

	function chooseMcpMode() {
		aiChoice = 'mcp';
	}

	function handleAIServiceChange(serviceIri: string | undefined) {
		if (!serviceIri) return;
		selectAIService(serviceIri);
	}

	async function selectAIService(serviceIri: string) {
		const requestId = ++aiModelsRequestId;
		selectedAIServiceIri = serviceIri;
		selectedAIModelIri = '';
		selectedOpenRouterModelId = '';
		apiKey = '';
		apiBaseUrl = '';
		aiModels = [];
		openRouterModels = [];
		openRouterBenchmarks = null;
		openRouterBenchmarksError = '';
		openRouterModelsError = '';
		openRouterModelsRequestId += 1;
		openRouterModelsLoading = false;
		aiConfigError = '';
		aiModelsLoading = true;
		try {
			const [models, savedKey, savedBaseUrl] = await Promise.all([
				loadAllAIModels(serviceIri, requestId),
				invoke<string>('ai__get_api_key', { serviceIri }),
				invoke<string>('ai__get_service_base_url', { serviceIri }),
			]);
			if (requestId !== aiModelsRequestId) return;
			aiModels = models;
			apiKey = savedKey ?? '';
			apiBaseUrl = savedBaseUrl ?? '';
		} catch (error) {
			if (requestId !== aiModelsRequestId) return;
			aiConfigError = String(error);
		} finally {
			if (requestId === aiModelsRequestId) aiModelsLoading = false;
		}
	}

	async function loadAllAIModels(serviceIri: string, requestId: number): Promise<AIModel[]> {
		const pageSize = 100;
		let offset = 0;
		let snapshotTx: number | null = null;
		const models: AIModel[] = [];
		let hasMore = true;

		while (hasMore) {
			const page = await invoke<AIModelPage>('setup__list_ai_models', {
				serviceIri,
				offset,
				limit: pageSize,
				snapshotTx,
			});
			if (requestId !== aiModelsRequestId || serviceIri !== selectedAIServiceIri) return [];
			if (snapshotTx === null) snapshotTx = page.snapshot_tx;
			else if (page.snapshot_tx !== snapshotTx) {
				throw new Error('O catálogo de modelos mudou durante a paginação. Tente novamente.');
			}
			models.push(...page.items);
			hasMore = page.has_more;
			if (hasMore) {
				if (page.next_cursor === null || page.next_cursor <= offset) {
					throw new Error('O catálogo de modelos retornou uma paginação inválida.');
				}
				offset = page.next_cursor;
			}
		}

		return models;
	}

	function handleAIModelChange(modelIri: string | undefined) {
		if (modelIri !== undefined) selectedAIModelIri = modelIri;
	}

	function handleOpenRouterModelChange(modelId: string | undefined) {
		if (modelId !== undefined) selectedOpenRouterModelId = modelId;
	}

	function handleAPIKeyInput(event: Event) {
		const target = event.currentTarget;
		if (!(target instanceof HTMLInputElement)) return;
		apiKey = target.value;
		openRouterModelsRequestId += 1;
		openRouterModelsLoading = false;
		openRouterModels = [];
		openRouterBenchmarks = null;
		openRouterBenchmarksError = '';
		openRouterModelsError = '';
	}

	function handleAPIBaseUrlInput(event: Event) {
		const target = event.currentTarget;
		if (!(target instanceof HTMLInputElement)) return;
		apiBaseUrl = target.value;
		openRouterModelsRequestId += 1;
		openRouterModelsLoading = false;
		openRouterModels = [];
		openRouterBenchmarks = null;
		openRouterBenchmarksError = '';
		openRouterModelsError = '';
	}

	async function loadOpenRouterModels() {
		if (!apiKey.trim() || !apiBaseUrl.trim()) return;
		const requestId = ++openRouterModelsRequestId;
		const requestedApiKey = apiKey.trim();
		const requestedBaseUrl = apiBaseUrl.trim();
		openRouterModelsLoading = true;
		aiConfigError = '';
		openRouterModelsError = '';
		try {
			await invoke('ai__validate_provider_key', { apiKey: requestedApiKey, baseUrl: requestedBaseUrl });
			const models = await invoke<OpenRouterModel[]>(
				'ai__list_openrouter_models',
				{ apiKey: requestedApiKey, baseUrl: requestedBaseUrl }
			);
			if (requestId !== openRouterModelsRequestId || requestedApiKey !== apiKey.trim() || requestedBaseUrl !== apiBaseUrl.trim()) return;
			openRouterModels = models;
			await loadOpenRouterBenchmarks(requestedApiKey, requestedBaseUrl, requestId);
			if (openRouterModels.length === 0) {
				openRouterModelsError = 'Nenhum modelo foi retornado pelo OpenRouter.';
			}
		} catch (error) {
			if (requestId !== openRouterModelsRequestId || requestedApiKey !== apiKey.trim() || requestedBaseUrl !== apiBaseUrl.trim()) return;
			openRouterModelsError = String(error);
		} finally {
			if (requestId === openRouterModelsRequestId) openRouterModelsLoading = false;
		}
	}

	async function loadOpenRouterBenchmarks(requestedApiKey: string, requestedBaseUrl: string, requestId: number) {
		openRouterBenchmarksLoading = true;
		openRouterBenchmarksError = '';
		try {
			const result = await invoke<OpenRouterBenchmarks>('ai__list_openrouter_benchmarks', { apiKey: requestedApiKey, baseUrl: requestedBaseUrl });
			if (requestId !== openRouterModelsRequestId || requestedApiKey !== apiKey.trim() || requestedBaseUrl !== apiBaseUrl.trim()) return;
			openRouterBenchmarks = result;
		} catch (error) {
			if (requestId === openRouterModelsRequestId) {
				openRouterBenchmarks = null;
				openRouterBenchmarksError = String(error);
			}
		} finally {
			if (requestId === openRouterModelsRequestId) openRouterBenchmarksLoading = false;
		}
	}

	function handleRetryLoadAIServices() {
		aiServicesLoaded = false;
		loadAIServices();
	}

	function handleRetryLoadSelectedAIService() {
		if (selectedAIServiceIri) selectAIService(selectedAIServiceIri);
	}

	function handleRetryLoadOpenRouterModels() {
		loadOpenRouterModels();
	}

	function goToEmailStepFromAI() {
		step = 2;
	}

	function goToSummaryStep() {
		step = 4;
		submitError = '';
	}

	function goToAIChoiceStepFromSummary() {
		step = 3;
		submitError = '';
	}

	async function finish() {
		if (!emailValid || !emailVerificationToken || !aiChoiceValid) return;
		isSubmitting = true;
		submitError = '';
		try {
			const emailAccountIri = await invoke<string>('imap__save_account', {
				accountIri: savedEmailAccountIri,
				input: {
					label: selectedEmailProvider?.label ?? 'E-mail',
					host: emailHost,
					port: emailPort,
					use_tls: emailUseTls,
					username: emailUsername.trim(),
					password: emailPassword,
					sync_interval_minutes: 15,
				},
			});
			savedEmailAccountIri = emailAccountIri;

			let aiModelIri: string | null = null;
			let aiServiceIri: string | null = null;
			if (aiChoice === 'chat' && selectedAIService) {
				aiServiceIri = selectedAIService.iri;
				if (requiresAIKey) {
					if (isOpenRouterService) {
						await invoke('ai__validate_provider_key', {
							apiKey: apiKey.trim(),
							baseUrl: apiBaseUrl.trim(),
						});
						await invoke('ai__save_service_base_url', {
							serviceIri: selectedAIService.iri,
							baseUrl: apiBaseUrl.trim(),
						});
					}
				await invoke('ai__save_api_key', {
					apiKey: apiKey.trim(),
					serviceIri: selectedAIService.iri,
				});
				}

				if (isOpenRouterService && selectedOpenRouterModel) {
					aiModelIri = await invoke<string>('ai__ensure_openrouter_model', {
						serviceIri: selectedAIService.iri,
						modelId: selectedOpenRouterModel.id,
						modelName: selectedOpenRouterModel.name,
						contextLength: selectedOpenRouterModel.contextLength,
						inputCostPerToken: selectedOpenRouterModel.inputCostPerToken,
						outputCostPerToken: selectedOpenRouterModel.outputCostPerToken,
						supportsTools: selectedOpenRouterModel.supportsTools ?? false,
					});
				} else if (selectedAIModel) {
					aiModelIri = selectedAIModel.iri;
				}
			}

			await invoke('setup__init', {
				userName: personName.trim(),
				email: emailUsername.trim(),
				emailAccountIri,
				emailTestToken: emailVerificationToken,
				aiIntegrationMode: aiChoice,
				aiServiceIri,
				aiModelIri,
			});

			onComplete?.();
		} catch (e) {
			submitError = String(e);
			isSubmitting = false;
		}
	}

	function handleStepOneContinue() {
		if (personName.trim()) goToEmailStep();
	}

	function handleInstructionsToggle() {
		toggleEmailInstructions();
	}

	function toggleApiKeyVisibility() {
		showApiKey = !showApiKey;
	}
</script>

<div class="overlay">
	<div class="wizard">

		<!-- cabeçalho -->
		<div class="header">
			<span class="logo">FOUNDATION</span>
			<span class="step-counter">ETAPA {step} DE 4</span>
			<div class="steps" role="progressbar" aria-label="Progresso da configuração" aria-valuemin="1" aria-valuemax="4" aria-valuenow={step}>
				{#each [1,2,3,4] as s}
					<div class="step-dot" class:active={step === s} class:done={step > s}></div>
				{/each}
			</div>
		</div>

		<!-- ── step 1: nome ── -->
		{#if step === 1}
			<div class="body">
				<span class="material-symbols-outlined step-icon">waving_hand</span>
				<h2>Bem-vindo ao FOUNDATION</h2>
				<p class="subtitle">Vamos configurar tudo em poucos minutos. Primeiro, como podemos te chamar?</p>
				<label for="person-name">Seu nome</label>
				<Input
					id="person-name"
					type="text"
					placeholder="Seu nome"
					bind:value={personName}
					bind:ref={nameInputEl}
					onkeydown={handleNameKeydown}
				/>
			</div>
			<div class="footer">
				<Button variant="default" onclick={handleStepOneContinue} disabled={!personName.trim()}>
					Continuar
				</Button>
			</div>

		<!-- ── step 2: e-mail ── -->
		{:else if step === 2}
			<div class="body">
				<span class="material-symbols-outlined step-icon">inbox</span>
				<h2>Configure seu e-mail</h2>
				<p class="subtitle">O FOUNDATION usa seu e-mail para criar memória de conversas e contexto. Selecione seu provedor:</p>

				<div class="provider-grid">
					{#each EMAIL_PROVIDERS as provider}
						<Button
							variant="ghost"
							class={`provider-card${selectedEmailProviderId === provider.id ? ' selected' : ''}${provider.disabled ? ' disabled' : ''}`}
							data-email-provider-id={provider.id}
							onclick={handleEmailProviderClick}
											aria-pressed={selectedEmailProviderId === provider.id}
							disabled={provider.disabled}
						>
							<span class="material-symbols-outlined">{provider.icon}</span>
							<span class="provider-label">{provider.label}</span>
							{#if provider.disabled}
								<span class="badge-soon">em breve</span>
							{/if}
						</Button>
					{/each}
				</div>

				{#if selectedEmailProvider && !selectedEmailProvider.disabled}
					<div class="email-form">
						{#if selectedEmailProvider.instructions.length > 0}
							<Button variant="ghost" class="instructions-toggle" onclick={handleInstructionsToggle}>
								<span class="material-symbols-outlined">{showInstructions ? 'expand_less' : 'help_outline'}</span>
								{showInstructions ? 'Ocultar instruções' : 'Como configurar o ' + selectedEmailProvider.label + '?'}
							</Button>

							{#if showInstructions}
								<ol class="instructions">
									{#each selectedEmailProvider.instructions as step_instruction}
										<li>{step_instruction}</li>
									{/each}
								</ol>
							{/if}
						{/if}

						{#if selectedEmailProviderId === 'custom'}
							<div class="field-group">
								<label for="email-host">Servidor IMAP</label>
								<Input id="email-host" type="text" placeholder="imap.exemplo.com" bind:value={emailHost} oninput={invalidateEmailTest} />
							</div>
							<div class="field-row">
								<div class="field-group">
									<label for="email-port">Porta</label>
									<Input id="email-port" type="number" bind:value={emailPort} oninput={invalidateEmailTest} />
								</div>
								<div class="field-group checkbox-group">
									<label for="email-tls" class="checkbox-row">
										<Checkbox id="email-tls" bind:checked={emailUseTls} onchange={handleEmailTlsChange} />
										Usar TLS
									</label>
								</div>
							</div>
						{/if}

						<div class="field-group">
							<label for="email-username">Usuário (e-mail)</label>
							<Input id="email-username" type="email" placeholder="seu@email.com" bind:value={emailUsername} oninput={invalidateEmailTest} />
						</div>

						<div class="field-group">
							<label for="email-password">
								{selectedEmailProvider.passwordLabel}
								{#if selectedEmailProvider.passwordHint}
									<span class="hint">— {selectedEmailProvider.passwordHint}</span>
								{/if}
							</label>
							<Input id="email-password" type="password" placeholder="••••••••••••" bind:value={emailPassword} oninput={invalidateEmailTest} />
						</div>

						<Button
							variant="secondary"
							onclick={testEmailConnection}
							disabled={emailTesting || !emailUsername.trim() || !emailPassword.trim() || !emailHost.trim()}
						>
							{#if emailTesting}
								<span class="material-symbols-outlined spin">sync</span>
								Testando conexão…
							{:else}
								<span class="material-symbols-outlined">wifi_tethering</span>
								Testar conexão
							{/if}
						</Button>

						{#if emailTestResult === 'ok'}
							<p class="test-ok" role="status" aria-live="polite">
								<span class="material-symbols-outlined">check_circle</span>
								{emailTestMessage}
							</p>
						{:else if emailTestResult === 'error'}
							<p class="test-error" role="alert" aria-live="assertive">
								<span class="material-symbols-outlined">error</span>
								{emailTestMessage}
							</p>
						{/if}
						{#if emailTestResult !== 'ok'}<p class="state-message" role="status">A conexão precisa ser testada novamente antes de continuar.</p>{/if}
					</div>
				{/if}
			</div>
			<div class="footer">
				<Button variant="ghost" onclick={goToNameStep}>Voltar</Button>
				<Button variant="default" onclick={goToAIChoiceStep} disabled={!emailValid}>
					Continuar
				</Button>
			</div>

		<!-- ── step 3: ia ── -->
		{:else if step === 3}
			<div class="body">
				<span class="material-symbols-outlined step-icon">smart_toy</span>
				<h2>Escolha como usar a IA</h2>
				<p class="subtitle">O modo define como você acessa a IA. No chat integrado, selecione também o serviço e o modelo.</p>

				<div class="ai-cards">
					<Button
						variant="ghost"
						class={`ai-card${aiChoice === 'chat' ? ' selected' : ''}`}
						aria-pressed={aiChoice === 'chat'}
						onclick={chooseChatMode}
					>
						<span class="material-symbols-outlined ai-card-icon">chat</span>
						<div class="ai-card-text">
							<span class="ai-card-title">Chat integrado</span>
							<span>Converse no FOUNDATION com um serviço e modelo configurados por você.</span>
						</div>
						<div class="ai-card-check">
							{#if aiChoice === 'chat'}
								<span class="material-symbols-outlined">check_circle</span>
							{/if}
						</div>
					</Button>

					<Button
						variant="ghost"
						class={`ai-card${aiChoice === 'mcp' ? ' selected' : ''}`}
						aria-pressed={aiChoice === 'mcp'}
						onclick={chooseMcpMode}
					>
						<span class="material-symbols-outlined ai-card-icon">hub</span>
						<div class="ai-card-text">
							<span class="ai-card-title">Via MCP</span>
							<span>Use seu app favorito (Claude Code, Claude.ai, etc.) conectado ao FOUNDATION pelo protocolo MCP.</span>
						</div>
						<div class="ai-card-check">
							{#if aiChoice === 'mcp'}
								<span class="material-symbols-outlined">check_circle</span>
							{/if}
						</div>
					</Button>
				</div>

				{#if aiChoice === 'chat'}
					{#if aiServicesLoading}
						<p class="state-message" role="status" aria-live="polite">Carregando serviços de IA…</p>
					{:else if aiServices.length === 0 && aiConfigError}
						<div class="state-panel" role="alert">
							<p>Não foi possível carregar os serviços de IA.</p>
							<p class="state-detail">{aiConfigError}</p>
							<Button variant="secondary" onclick={handleRetryLoadAIServices}>Tentar novamente</Button>
						</div>
					{:else if aiServices.length === 0}
						<div class="state-panel" role="status">
							<p>Nenhum serviço de IA com suporte ativo está disponível.</p>
							<Button variant="secondary" onclick={handleRetryLoadAIServices}>Verificar novamente</Button>
						</div>
					{:else}
						<div class="field-group">
							<label for="setup-ai-service">Serviço de IA</label>
							<Select.Root
								type="single"
								value={selectedAIServiceIri}
								onValueChange={handleAIServiceChange}
							>
								<Select.Trigger id="setup-ai-service">
					{selectedAIService ? aiServiceDisplayName(selectedAIService) : 'Selecione um serviço'}
								</Select.Trigger>
								<Select.Content>
									{#each aiServices as service}
						<Select.Item value={service.iri}>{aiServiceDisplayName(service)}</Select.Item>
									{/each}
								</Select.Content>
							</Select.Root>
							{#if selectedAIService}
								<div class="service-description" aria-live="polite">
									<span class="service-kind">{selectedAIService.isLocal ? 'Processamento local' : 'Provedor online'}</span>
									{#if selectedAIService.description.trim()}
										<p>{selectedAIService.description}</p>
									{/if}
								</div>
							{/if}
						</div>

						{#if selectedAIService}
							{#if requiresAIKey}
								{#if isOpenRouterService}
									<div class="field-group">
									<label for="setup-ai-base-url">URL base</label>
									<Input
										id="setup-ai-base-url"
										type="url"
										placeholder="URL do serviço OpenRouter"
										value={apiBaseUrl}
										oninput={handleAPIBaseUrlInput}
									/>
								</div>
								{/if}

								<div class="field-group">
									<label for="setup-ai-api-key">Chave de API</label>
									<div class="field-row">
										<Input
											id="setup-ai-api-key"
											type={showApiKey ? 'text' : 'password'}
											placeholder="Chave do serviço selecionado"
											value={apiKey}
											oninput={handleAPIKeyInput}
										/>
										<Button
											variant="ghost"
											size="icon-sm"
											onclick={toggleApiKeyVisibility}
							aria-pressed={showApiKey}
											title={showApiKey ? 'Ocultar chave' : 'Mostrar chave'}
											aria-label={showApiKey ? 'Ocultar chave de API' : 'Mostrar chave de API'}
										>
											<span class="material-symbols-outlined">{showApiKey ? 'visibility_off' : 'visibility'}</span>
										</Button>
									</div>
								</div>
							{/if}

							{#if isOpenRouterService}
								<div class="field-group">
					<OpenRouterModelSelector
						idPrefix="setup-openrouter"
						providerName={selectedAIService ? aiServiceDisplayName(selectedAIService) : 'OpenRouter'}
						models={openRouterModels}
						benchmarks={openRouterBenchmarks}
						bind:selectedModelId={selectedOpenRouterModelId}
						loading={openRouterModelsLoading}
						error={openRouterModelsError}
						benchmarksLoading={openRouterBenchmarksLoading}
						benchmarksError={openRouterBenchmarksError}
						loadHint={!apiKey.trim() ? 'Informe a chave de API para carregar o catálogo.' : !apiBaseUrl.trim() ? 'Informe a URL base do OpenRouter para carregar o catálogo.' : 'Carregue o catálogo para comparar os modelos disponíveis.'}
						onRetry={handleRetryLoadOpenRouterModels}
					/>
								</div>
							{:else}
								<div class="field-group">
									<label for="setup-ai-model">Modelo</label>
									{#if aiModelsLoading}
										<p class="state-message" role="status" aria-live="polite">Carregando modelos…</p>
									{:else if aiConfigError}
										<div class="state-panel" role="alert">
											<p>Não foi possível carregar os modelos deste serviço.</p>
											<p class="state-detail">{aiConfigError}</p>
											<Button variant="secondary" onclick={handleRetryLoadSelectedAIService}>Tentar novamente</Button>
										</div>
									{:else if aiModels.length === 0}
										<div class="state-panel" role="status">
											<p>Nenhum modelo está disponível para este serviço.</p>
											<Button variant="secondary" onclick={handleRetryLoadSelectedAIService}>Verificar novamente</Button>
										</div>
									{:else}
										<Select.Root
											type="single"
											value={selectedAIModelIri}
											onValueChange={handleAIModelChange}
										>
											<Select.Trigger id="setup-ai-model">
												<Select.Value placeholder="Selecione um modelo" />
											</Select.Trigger>
											<Select.Content>
												{#each aiModels as model}
													<Select.Item value={model.iri}>{model.label}</Select.Item>
												{/each}
											</Select.Content>
										</Select.Root>
									{/if}
								</div>
							{/if}
						{/if}
					{/if}
				{/if}

				{#if aiChoice === 'mcp'}
					<div class="mcp-info">
						<p>
							O provedor e o modelo são escolhidos no cliente externo de IA; o FOUNDATION não seleciona um padrão neste modo.
							Quando o FOUNDATION estiver aberto, ele expõe um servidor MCP local em
							<code>http://127.0.0.1:47178/mcp</code>.
							Configure seu app de IA favorito para usar esse endereço.
						</p>
					</div>
				{/if}
			</div>
			<div class="footer">
				<Button variant="ghost" onclick={goToEmailStepFromAI}>Voltar</Button>
				<Button
					variant="default"
					onclick={goToSummaryStep}
					disabled={!aiChoiceValid}
				>
					Continuar
				</Button>
			</div>

		<!-- ── step 4: pronto ── -->
		{:else if step === 4}
			<div class="body body-center">
				<span class="material-symbols-outlined step-icon done-icon">check_circle</span>
				<h2>Tudo pronto, {personName}!</h2>
				<p class="subtitle">Aqui está um resumo da sua configuração:</p>

				<div class="summary">
					<div class="summary-row">
						<span class="material-symbols-outlined">person</span>
						<span>{personName}</span>
					</div>
					<div class="summary-row">
						<span class="material-symbols-outlined">inbox</span>
						<span>{emailUsername} <span class="summary-meta">({selectedEmailProvider?.label})</span></span>
					</div>
					<div class="summary-row">
						<span class="material-symbols-outlined">{aiChoice === 'chat' ? 'chat' : 'hub'}</span>
						<span>
							{#if aiChoice === 'chat'}
								{selectedAIService ? aiServiceDisplayName(selectedAIService) : ''} · {#if isOpenRouterService}{selectedOpenRouterModel?.name}{:else}{selectedAIModel?.label}{/if}
							{:else}
								Via MCP
							{/if}
						</span>
					</div>
				</div>

				{#if submitError}
					<p class="submit-error" role="alert" aria-live="polite">
						<span class="material-symbols-outlined">error</span>
						{submitError}
					</p>
				{/if}
			</div>
			<div class="footer">
				<Button variant="ghost" onclick={goToAIChoiceStepFromSummary}>Voltar</Button>
				<Button variant="default" onclick={finish} disabled={isSubmitting}>
					{isSubmitting ? 'Configurando…' : 'Começar'}
				</Button>
			</div>
		{/if}

	</div>
</div>

<style>
	.overlay {
		position: fixed;
		inset: 0;
		background-color: var(--color-surface-0);
		background-image: radial-gradient(
			circle,
			var(--color-hud-grid-dot) var(--size-hud-grid-dot),
			transparent var(--size-hud-grid-dot)
		);
		background-size: var(--size-hud-grid) var(--size-hud-grid);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 9999;
	}

	.wizard {
		position: relative;
		display: flex;
		flex-direction: column;
		width: var(--size-setup-panel-width);
		max-width: var(--size-setup-panel-max-width);
		max-height: var(--size-setup-panel-max-height);
		background: var(--color-surface-1);
		border-radius: var(--radius);
		border: var(--size-setup-outline) solid var(--color-hud-line-subtle);
		overflow: hidden;
		box-shadow: 0 0 var(--size-hud-glow-blur) var(--color-hud-glow);
	}

	.wizard::before {
		position: absolute;
		inset: 0 var(--space-5) auto;
		height: var(--size-hud-frame-line);
		background: var(--color-hud-line);
		content: '';
		pointer-events: none;
	}

	.header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: var(--space-4) var(--space-5);
		border-bottom: var(--size-hud-frame-line) solid var(--color-hud-line-subtle);
	}

	.logo {
		font-size: var(--type-body-compact);
		letter-spacing: var(--tracking-hud-label);
		color: var(--text-primary);
		text-transform: uppercase;
	}

	.step-counter {
		font-size: var(--type-overline);
		letter-spacing: var(--tracking-hud-label);
		color: var(--text-secondary);
		text-transform: uppercase;
	}

	.steps {
		display: flex;
		gap: var(--space-2);
		align-items: center;
	}

	.step-dot {
		width: var(--size-setup-progress-dot);
		height: var(--size-setup-progress-dot);
		border-radius: 50%;
		background: var(--color-neutral-disabled);
		transition: all var(--duration-setup-transition) ease;
	}

	.step-dot.active {
		background: var(--color-selection);
		width: var(--size-setup-progress-active);
		border-radius: var(--radius);
	}

	.step-dot.done {
		background: var(--color-interactive);
	}

	.body {
		flex: 1;
		overflow-y: auto;
		padding: var(--space-5) var(--space-5) var(--space-4);
		display: flex;
		flex-direction: column;
		gap: var(--space-4);
	}

	.body-center {
		align-items: center;
		text-align: center;
	}

	.step-icon {
		font-size: var(--type-icon-large);
		color: var(--color-interactive);
	}

	.done-icon {
		color: var(--color-success);
	}

	h2 {
		font-size: var(--type-title);
		color: var(--text-primary);
		margin: 0;
	}

	.subtitle {
		font-size: var(--type-body);
		color: var(--text-body);
		margin: 0;
		line-height: var(--line-height-body);
	}

	.field-group {
		display: flex;
		flex-direction: column;
		gap: var(--spacing-label-to-value);
	}

	.field-row {
		display: flex;
		gap: var(--spacing-control-internal);
		align-items: flex-end;
	}

	label {
		font-size: var(--type-label);
		color: var(--text-secondary);
		text-transform: uppercase;
		letter-spacing: var(--tracking-hud-label);
	}

	.hint {
		font-weight: 400;
		text-transform: none;
		letter-spacing: 0;
		color: var(--text-secondary);
	}

	.checkbox-row {
		display: flex;
		flex-direction: row;
		align-items: center;
		gap: var(--spacing-control-internal);
		text-transform: none;
		letter-spacing: 0;
		cursor: pointer;
	}

	.provider-grid {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: var(--spacing-control-internal);
	}

	:global([data-slot="button"].provider-card) {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: var(--spacing-inline-tight);
		padding: var(--space-3) var(--space-2);
		min-height: var(--size-touch-target);
		height: auto;
		background: var(--color-surface-2);
		border: var(--size-setup-outline) solid var(--color-hud-line-subtle);
		border-radius: var(--radius);
		cursor: pointer;
		transition: all var(--duration-setup-transition) ease;
		position: relative;
	}

	:global([data-slot="button"].provider-card:hover:not(.disabled)) {
		background: var(--color-surface-3);
	}

	:global([data-slot="button"].provider-card.selected) {
		border-color: var(--color-selection);
		background: var(--color-selection-subtle);
	}

	:global([data-slot="button"].provider-card.disabled) {
		opacity: 0.4;
		cursor: not-allowed;
	}

	:global([data-slot="button"].provider-card .material-symbols-outlined) {
		font-size: var(--type-icon-medium);
		color: var(--text-secondary);
	}

	:global([data-slot="button"].provider-card.selected .material-symbols-outlined) {
		color: var(--color-selection);
	}

	.provider-label {
		font-size: var(--type-caption);
		color: var(--text-body);
		text-align: center;
		line-height: var(--line-height-compact);
	}

	.badge-soon {
		position: absolute;
		top: var(--space-1);
		right: var(--space-1);
		font-size: var(--type-overline);
		background: var(--color-surface-3);
		color: var(--text-secondary);
		padding: var(--space-1) var(--space-2);
		border-radius: var(--radius);
		text-transform: uppercase;
		letter-spacing: var(--tracking-hud-label);
	}

	.email-form {
		display: flex;
		flex-direction: column;
		gap: var(--space-3);
		padding-top: var(--space-2);
	}

	:global([data-slot="button"].instructions-toggle) {
		display: flex;
		align-items: center;
		gap: var(--spacing-inline-tight);
		color: var(--color-interactive);
		font-size: var(--type-body-compact);
		padding: 0;
		height: auto;
	}

	:global([data-slot="button"].instructions-toggle:hover) {
		background: transparent;
		color: var(--color-interactive);
	}

	:global([data-slot="button"].instructions-toggle .material-symbols-outlined) {
		font-size: var(--type-icon-small);
	}

	.instructions {
		margin: 0;
		padding: var(--space-3) var(--space-3) var(--space-3) var(--space-6);
		font-size: var(--type-body-compact);
		color: var(--text-body);
		line-height: var(--line-height-body);
		background: var(--color-surface-2);
		border-radius: var(--radius);
		border-inline-start: var(--size-hud-frame-line) solid var(--color-hud-line);
	}

	@keyframes spin {
		to { transform: rotate(360deg); }
	}

	.spin {
		animation: spin var(--duration-setup-spinner) linear infinite;
		display: inline-block;
	}

	.test-ok {
		display: flex;
		align-items: center;
		gap: var(--spacing-inline-tight);
		font-size: var(--type-body-compact);
		color: var(--color-success);
		margin: 0;
	}

	.test-ok .material-symbols-outlined,
	.test-error .material-symbols-outlined {
		font-size: var(--type-icon-small);
	}

	.test-error {
		display: flex;
		align-items: flex-start;
		gap: var(--spacing-inline-tight);
		font-size: var(--type-body-compact);
		color: var(--color-danger);
		margin: 0;
		line-height: var(--line-height-body);
	}

	.ai-cards {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
	}

	:global([data-slot="button"].ai-card) {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		padding: var(--space-3) var(--space-4);
		min-height: var(--size-touch-target);
		height: auto;
		justify-content: flex-start;
		width: 100%;
		background: var(--color-surface-2);
		border: var(--size-setup-outline) solid var(--color-hud-line-subtle);
		border-radius: var(--radius);
		cursor: pointer;
		transition: all var(--duration-setup-transition) ease;
	}

	:global([data-slot="button"].ai-card:hover) {
		background: var(--color-surface-3);
	}

	:global([data-slot="button"].ai-card.selected) {
		border-color: var(--color-selection);
		background: var(--color-selection-subtle);
	}

	.ai-card-icon {
		font-size: var(--type-icon-medium);
		color: var(--text-secondary);
		flex-shrink: 0;
	}

	:global([data-slot="button"].ai-card.selected .ai-card-icon) {
		color: var(--color-selection);
	}

	.ai-card-text {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: var(--spacing-inline-tight);
	}

	.ai-card-title {
		font-size: var(--type-body-prominent);
		color: var(--text-primary);
	}

	.ai-card-text span {
		font-size: var(--type-body-compact);
		color: var(--text-body);
		line-height: var(--line-height-compact);
	}

	.ai-card-check .material-symbols-outlined {
		font-size: var(--type-icon-medium);
		color: var(--color-selection);
	}

	.mcp-info {
		padding: var(--space-3);
		background: var(--color-surface-2);
		border-radius: var(--radius);
		font-size: var(--type-body-compact);
		color: var(--text-body);
		line-height: var(--line-height-body);
		border-inline-start: var(--size-hud-frame-line) solid var(--color-hud-line);
	}

	.mcp-info p {
		margin: 0;
	}

	.mcp-info code {
		background: var(--color-surface-3);
		padding: var(--space-1) var(--space-2);
		border-radius: var(--radius);
		font-size: var(--type-caption);
		color: var(--color-interactive);
	}

	.summary {
		display: flex;
		flex-direction: column;
		gap: var(--space-2);
		width: 100%;
		padding: var(--space-4);
		background: var(--color-surface-2);
		border-radius: var(--radius);
		border: var(--size-hud-frame-line) solid var(--color-hud-line-subtle);
	}

	.summary-row {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		font-size: var(--type-body);
		color: var(--text-body);
	}

	.summary-row .material-symbols-outlined {
		font-size: var(--type-icon-medium);
		color: var(--color-interactive);
		flex-shrink: 0;
	}

	.summary-meta {
		color: var(--text-secondary);
	}

	.submit-error {
		display: flex;
		align-items: flex-start;
		gap: var(--spacing-control-internal);
		font-size: var(--type-body-compact);
		color: var(--text-danger);
		margin: 0;
		line-height: var(--line-height-body);
		border-inline-start: var(--size-hud-frame-line) solid var(--color-danger);
		padding: var(--space-2);
	}

	.submit-error .material-symbols-outlined {
		font-size: var(--type-icon-small);
		flex-shrink: 0;
		margin-top: var(--space-1);
	}

	.footer {
		padding: var(--space-4) var(--space-5);
		display: flex;
		justify-content: flex-end;
		gap: var(--space-2);
		border-top: var(--size-hud-frame-line) solid var(--color-hud-line-subtle);
	}

	.service-description {
		display: grid;
		gap: var(--space-1);
		padding-inline-start: var(--space-3);
		border-inline-start: var(--size-hud-frame-line) solid var(--color-hud-line-subtle);
	}

	.service-kind {
		font-size: var(--type-label);
		letter-spacing: var(--tracking-hud-label);
		text-transform: uppercase;
		color: var(--text-secondary);
	}

	.service-description p {
		margin: 0;
		font-size: var(--type-body-compact);
		color: var(--text-body);
		line-height: var(--line-height-body);
	}

	.state-message,
	.state-detail {
		margin: 0;
		font-size: var(--type-body-compact);
		color: var(--text-secondary);
		line-height: var(--line-height-body);
	}

	.state-panel {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: var(--space-2);
		padding: var(--space-3);
		border-inline-start: var(--size-hud-frame-line) solid var(--color-danger);
		background: var(--color-surface-2);
		color: var(--text-body);
	}

	.state-panel p {
		margin: 0;
	}

</style>
