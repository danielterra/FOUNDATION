<script>
	import { invoke } from '@tauri-apps/api/core';
	import posthog from 'posthog-js';

	let { onComplete = () => {} } = $props();

	// ── estado global ──────────────────────────────────────────────────────────
	let step = $state(1); // 1 nome | 2 email | 3 ia | 4 pronto
	let isSubmitting = $state(false);
	let submitError = $state('');

	// ── step 1: identificação ──────────────────────────────────────────────────
	let personName = $state('');
	let nameInputEl = $state(null);
	$effect(() => {
		if (step === 1 && nameInputEl) nameInputEl.focus();
	});

	// ── step 2: e-mail ─────────────────────────────────────────────────────────
	const PROVIDERS = [
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

	let selectedProviderId = $state('');
	let emailUsername = $state('');
	let emailPassword = $state('');
	let emailHost = $state('');
	let emailPort = $state(993);
	let emailUseTls = $state(true);
	let emailTesting = $state(false);
	let emailTestResult = $state(null); // null | 'ok' | 'error'
	let emailTestMessage = $state('');
	let showInstructions = $state(false);

	let selectedProvider = $derived(PROVIDERS.find(p => p.id === selectedProviderId) ?? null);

	function selectProvider(provider) {
		if (provider.disabled) return;
		selectedProviderId = provider.id;
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

	async function testEmailConnection() {
		if (!emailUsername.trim() || !emailPassword.trim() || !emailHost.trim()) return;
		emailTesting = true;
		emailTestResult = null;
		emailTestMessage = '';
		try {
			const msg = await invoke('imap__test_connection', {
				host: emailHost,
				port: emailPort,
				useTls: emailUseTls,
				username: emailUsername,
				password: emailPassword,
			});
			emailTestResult = 'ok';
			emailTestMessage = msg;
		} catch (e) {
			emailTestResult = 'error';
			emailTestMessage = String(e);
		} finally {
			emailTesting = false;
		}
	}

	let emailValid = $derived(
		selectedProviderId !== '' &&
		emailUsername.trim() !== '' &&
		emailPassword.trim() !== '' &&
		emailHost.trim() !== '' &&
		emailTestResult === 'ok'
	);

	// ── step 3: ia ─────────────────────────────────────────────────────────────
	let aiChoice = $state(''); // 'chat' | 'mcp'
	let apiKey = $state('');
	let showApiKey = $state(false);

	// ── step 4: privacidade ────────────────────────────────────────────────────
	let analyticsConsent = $state(false);

	// ── finalização ────────────────────────────────────────────────────────────
	async function finish() {
		isSubmitting = true;
		submitError = '';
		try {
			// 1. setup principal (nome + email de identidade)
			await invoke('setup__init', {
				userName: personName.trim(),
				email: emailUsername.trim() || null,
				aiServiceIri: aiChoice === 'chat' ? 'foundation:ClaudeAIService' : null,
				aiModelIri: null,
			});

			// 2. salvar conta IMAP
			await invoke('imap__save_account', {
				accountIri: null,
				input: {
					label: selectedProvider?.label ?? 'E-mail',
					host: emailHost,
					port: emailPort,
					use_tls: emailUseTls,
					username: emailUsername.trim(),
					password: emailPassword,
					sync_interval_minutes: 15,
				},
			});

			// 3. salvar API key se escolheu chat interno
			if (aiChoice === 'chat' && apiKey.trim()) {
				await invoke('ai__save_api_key', {
					apiKey: apiKey.trim(),
					serviceIri: 'foundation:ClaudeAIService',
				});
			}

			if (analyticsConsent) {
				posthog.opt_in_capturing();
			} else {
				posthog.opt_out_capturing();
			}

			onComplete();
		} catch (e) {
			submitError = String(e);
			isSubmitting = false;
		}
	}
</script>

<div class="overlay">
	<div class="wizard">

		<!-- cabeçalho -->
		<div class="header">
			<span class="logo">FOUNDATION</span>
			<div class="steps">
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
				<input
					class="field"
					type="text"
					placeholder="Seu nome"
					bind:value={personName}
					bind:this={nameInputEl}
					onkeydown={(e) => e.key === 'Enter' && personName.trim() && (step = 2)}
				/>
			</div>
			<div class="footer">
				<button class="btn-primary" onclick={() => step = 2} disabled={!personName.trim()}>
					Continuar
				</button>
			</div>

		<!-- ── step 2: e-mail ── -->
		{:else if step === 2}
			<div class="body">
				<span class="material-symbols-outlined step-icon">inbox</span>
				<h2>Configure seu e-mail</h2>
				<p class="subtitle">O FOUNDATION usa seu e-mail para criar memória de conversas e contexto. Selecione seu provedor:</p>

				<div class="provider-grid">
					{#each PROVIDERS as provider}
						<button
							class="provider-card"
							class:selected={selectedProviderId === provider.id}
							class:disabled={provider.disabled}
							onclick={() => selectProvider(provider)}
							disabled={provider.disabled}
						>
							<span class="material-symbols-outlined">{provider.icon}</span>
							<span class="provider-label">{provider.label}</span>
							{#if provider.disabled}
								<span class="badge-soon">em breve</span>
							{/if}
						</button>
					{/each}
				</div>

				{#if selectedProvider && !selectedProvider.disabled}
					<div class="email-form">
						{#if selectedProvider.instructions.length > 0}
							<button class="instructions-toggle" onclick={() => showInstructions = !showInstructions}>
								<span class="material-symbols-outlined">{showInstructions ? 'expand_less' : 'help_outline'}</span>
								{showInstructions ? 'Ocultar instruções' : 'Como configurar o ' + selectedProvider.label + '?'}
							</button>

							{#if showInstructions}
								<ol class="instructions">
									{#each selectedProvider.instructions as step_instruction}
										<li>{step_instruction}</li>
									{/each}
								</ol>
							{/if}
						{/if}

						{#if selectedProviderId === 'custom'}
							<div class="field-group">
								<label for="email-host">Servidor IMAP</label>
								<input id="email-host" class="field" type="text" placeholder="imap.exemplo.com" bind:value={emailHost} />
							</div>
							<div class="field-row">
								<div class="field-group">
									<label for="email-port">Porta</label>
									<input id="email-port" class="field" type="number" bind:value={emailPort} />
								</div>
								<div class="field-group checkbox-group">
									<label for="email-tls">
										<input id="email-tls" type="checkbox" bind:checked={emailUseTls} />
										Usar TLS
									</label>
								</div>
							</div>
						{/if}

						<div class="field-group">
							<label for="email-username">Usuário (e-mail)</label>
							<input id="email-username" class="field" type="email" placeholder="seu@email.com" bind:value={emailUsername} />
						</div>

						<div class="field-group">
							<label for="email-password">
								{selectedProvider.passwordLabel}
								{#if selectedProvider.passwordHint}
									<span class="hint">— {selectedProvider.passwordHint}</span>
								{/if}
							</label>
							<input id="email-password" class="field" type="password" placeholder="••••••••••••" bind:value={emailPassword} />
						</div>

						<button
							class="btn-test"
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
						</button>

						{#if emailTestResult === 'ok'}
							<p class="test-ok">
								<span class="material-symbols-outlined">check_circle</span>
								{emailTestMessage}
							</p>
						{:else if emailTestResult === 'error'}
							<p class="test-error">
								<span class="material-symbols-outlined">error</span>
								{emailTestMessage}
							</p>
						{/if}
					</div>
				{/if}
			</div>
			<div class="footer">
				<button class="btn-secondary" onclick={() => step = 1}>Voltar</button>
				<button class="btn-primary" onclick={() => step = 3} disabled={!emailValid}>
					Continuar
				</button>
			</div>

		<!-- ── step 3: ia ── -->
		{:else if step === 3}
			<div class="body">
				<span class="material-symbols-outlined step-icon">smart_toy</span>
				<h2>Como você quer usar a IA?</h2>
				<p class="subtitle">Escolha como o FOUNDATION vai funcionar. Você pode mudar isso depois.</p>

				<div class="ai-cards">
					<button
						class="ai-card"
						class:selected={aiChoice === 'chat'}
						onclick={() => aiChoice = 'chat'}
					>
						<span class="material-symbols-outlined ai-card-icon">chat</span>
						<div class="ai-card-text">
							<strong>Chat interno</strong>
							<span>Use o chat integrado do FOUNDATION com a API do Claude.</span>
						</div>
						<div class="ai-card-check">
							{#if aiChoice === 'chat'}
								<span class="material-symbols-outlined">check_circle</span>
							{/if}
						</div>
					</button>

					<button
						class="ai-card"
						class:selected={aiChoice === 'mcp'}
						onclick={() => { aiChoice = 'mcp'; apiKey = ''; }}
					>
						<span class="material-symbols-outlined ai-card-icon">hub</span>
						<div class="ai-card-text">
							<strong>Via MCP</strong>
							<span>Use seu app favorito (Claude Code, Claude.ai, etc.) conectado ao FOUNDATION pelo protocolo MCP.</span>
						</div>
						<div class="ai-card-check">
							{#if aiChoice === 'mcp'}
								<span class="material-symbols-outlined">check_circle</span>
							{/if}
						</div>
					</button>
				</div>

				{#if aiChoice === 'chat'}
					<div class="api-key-section">
						<p class="api-key-hint">
							Você precisa de uma chave de API do Claude.
							<a href="https://console.anthropic.com/" target="_blank" rel="noreferrer">Obter chave →</a>
						</p>
						<div class="field-group">
							<label for="api-key">Chave de API</label>
							<div class="field-row">
								<input
									id="api-key"
									class="field"
									type={showApiKey ? 'text' : 'password'}
									placeholder="sk-ant-…"
									bind:value={apiKey}
								/>
								<button class="btn-icon" onclick={() => showApiKey = !showApiKey} title={showApiKey ? 'Ocultar' : 'Mostrar'}>
									<span class="material-symbols-outlined">{showApiKey ? 'visibility_off' : 'visibility'}</span>
								</button>
							</div>
						</div>
					</div>
				{/if}

				{#if aiChoice === 'mcp'}
					<div class="mcp-info">
						<p>
							Quando o FOUNDATION estiver aberto, ele expõe um servidor MCP local em
							<code>http://127.0.0.1:47178/mcp</code>.
							Configure seu app de IA favorito para usar esse endereço.
						</p>
					</div>
				{/if}
			</div>
			<div class="footer">
				<button class="btn-secondary" onclick={() => step = 2}>Voltar</button>
				<button
					class="btn-primary"
					onclick={() => step = 4}
					disabled={!aiChoice || (aiChoice === 'chat' && !apiKey.trim())}
				>
					Continuar
				</button>
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
						<span>{emailUsername} <em>({selectedProvider?.label})</em></span>
					</div>
					<div class="summary-row">
						<span class="material-symbols-outlined">{aiChoice === 'chat' ? 'chat' : 'hub'}</span>
						<span>{aiChoice === 'chat' ? 'Chat interno com Claude' : 'Via MCP'}</span>
					</div>
				</div>

				<label class="consent-row">
					<input type="checkbox" bind:checked={analyticsConsent} />
					<span>
						Compartilhar dados de uso anônimos para nos ajudar a melhorar o FOUNDATION.
						Nenhuma informação pessoal ou conteúdo de e-mail é enviado.
					</span>
				</label>

				{#if submitError}
					<p class="submit-error">
						<span class="material-symbols-outlined">error</span>
						{submitError}
					</p>
				{/if}
			</div>
			<div class="footer">
				<button class="btn-secondary" onclick={() => { step = 3; submitError = ''; }}>Voltar</button>
				<button class="btn-primary" onclick={finish} disabled={isSubmitting}>
					{isSubmitting ? 'Configurando…' : 'Começar'}
				</button>
			</div>
		{/if}

	</div>
</div>

<style>
	.overlay {
		position: fixed;
		inset: 0;
		background: var(--color-black);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 9999;
	}

	.wizard {
		display: flex;
		flex-direction: column;
		width: 520px;
		max-width: 95vw;
		max-height: 92vh;
		background: var(--color-surface-1);
		border-radius: var(--radius-lg);
		overflow: hidden;
	}

	/* cabeçalho */
	.header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 1.25rem 1.75rem;
		border-bottom: 1px solid color-mix(in srgb, var(--color-neutral) 10%, transparent);
	}

	.logo {
		font-size: 0.875rem;
		font-weight: 700;
		letter-spacing: 0.2rem;
		color: var(--color-neutral);
		text-transform: uppercase;
	}

	.steps {
		display: flex;
		gap: 0.4rem;
	}

	.step-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--color-surface-3);
		transition: all 0.2s;
	}

	.step-dot.active {
		background: var(--color-interactive);
		width: 18px;
		border-radius: 3px;
	}

	.step-dot.done {
		background: color-mix(in srgb, var(--color-interactive) 50%, transparent);
	}

	/* corpo */
	.body {
		flex: 1;
		overflow-y: auto;
		padding: 2rem 1.75rem 1rem;
		display: flex;
		flex-direction: column;
		gap: 1.25rem;
	}

	.body-center {
		align-items: center;
		text-align: center;
	}

	.step-icon {
		font-size: 2.25rem;
		color: var(--color-interactive);
	}

	.done-icon {
		color: var(--color-success, #4caf50);
	}

	h2 {
		font-size: 1.25rem;
		font-weight: 600;
		color: var(--color-neutral-active);
		margin: 0;
	}

	.subtitle {
		font-size: 0.875rem;
		color: var(--color-neutral);
		margin: 0;
		line-height: 1.6;
	}

	/* campo de texto */
	.field {
		width: 100%;
		box-sizing: border-box;
		background: var(--color-surface-2);
		color: var(--color-neutral-active);
		border: 1px solid transparent;
		border-radius: var(--radius-sm);
		padding: 0.625rem 0.875rem;
		font-size: 0.9375rem;
		transition: border-color 0.15s;
	}

	.field:focus {
		outline: none;
		border-color: var(--color-interactive);
	}

	.field-group {
		display: flex;
		flex-direction: column;
		gap: 0.375rem;
	}

	.field-row {
		display: flex;
		gap: 0.625rem;
		align-items: flex-end;
	}

	.field-row .field {
		flex: 1;
	}

	label {
		font-size: 0.8125rem;
		font-weight: 500;
		color: var(--color-neutral);
		text-transform: uppercase;
		letter-spacing: 0.05rem;
	}

	.hint {
		font-weight: 400;
		text-transform: none;
		letter-spacing: 0;
		opacity: 0.75;
	}

	.checkbox-group label {
		flex-direction: row;
		align-items: center;
		gap: 0.5rem;
		text-transform: none;
		letter-spacing: 0;
		font-weight: 400;
		cursor: pointer;
	}

	/* provedores */
	.provider-grid {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 0.625rem;
	}

	.provider-card {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.375rem;
		padding: 0.875rem 0.5rem;
		background: var(--color-surface-2);
		border: 1.5px solid transparent;
		border-radius: var(--radius-sm);
		cursor: pointer;
		transition: all 0.15s;
		position: relative;
	}

	.provider-card:hover:not(.disabled) {
		background: var(--color-surface-3);
	}

	.provider-card.selected {
		border-color: var(--color-interactive);
		background: color-mix(in srgb, var(--color-interactive) 8%, var(--color-surface-2));
	}

	.provider-card.disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}

	.provider-card .material-symbols-outlined {
		font-size: 1.5rem;
		color: var(--color-neutral);
	}

	.provider-card.selected .material-symbols-outlined {
		color: var(--color-interactive);
	}

	.provider-label {
		font-size: 0.75rem;
		color: var(--color-neutral);
		text-align: center;
		line-height: 1.3;
	}

	.badge-soon {
		position: absolute;
		top: 0.25rem;
		right: 0.25rem;
		font-size: 0.625rem;
		background: var(--color-surface-3);
		color: var(--color-neutral);
		padding: 0.1rem 0.35rem;
		border-radius: 2px;
		text-transform: uppercase;
		letter-spacing: 0.05rem;
	}

	/* formulário de e-mail */
	.email-form {
		display: flex;
		flex-direction: column;
		gap: 1rem;
		padding-top: 0.5rem;
	}

	.instructions-toggle {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		background: none;
		border: none;
		color: var(--color-interactive);
		font-size: 0.8125rem;
		cursor: pointer;
		padding: 0;
		transition: opacity 0.15s;
	}

	.instructions-toggle:hover {
		opacity: 0.8;
	}

	.instructions-toggle .material-symbols-outlined {
		font-size: 1rem;
	}

	.instructions {
		margin: 0;
		padding-left: 1.25rem;
		font-size: 0.8125rem;
		color: var(--color-neutral);
		line-height: 1.7;
		background: var(--color-surface-2);
		border-radius: var(--radius-sm);
		padding: 1rem 1rem 1rem 2rem;
	}

	.btn-test {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 0.5rem;
		background: var(--color-surface-2);
		color: var(--color-neutral);
		border: 1.5px solid color-mix(in srgb, var(--color-neutral) 20%, transparent);
		border-radius: var(--radius-sm);
		padding: 0.625rem 1.25rem;
		font-size: 0.875rem;
		cursor: pointer;
		transition: all 0.15s;
	}

	.btn-test:hover:not(:disabled) {
		background: var(--color-surface-3);
		color: var(--color-neutral-active);
		border-color: var(--color-interactive);
	}

	.btn-test:disabled {
		opacity: 0.4;
		cursor: not-allowed;
	}

	.btn-test .material-symbols-outlined {
		font-size: 1.125rem;
	}

	@keyframes spin {
		to { transform: rotate(360deg); }
	}

	.spin {
		animation: spin 0.8s linear infinite;
		display: inline-block;
	}

	.test-ok {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		font-size: 0.8125rem;
		color: var(--color-success, #4caf50);
		margin: 0;
	}

	.test-ok .material-symbols-outlined,
	.test-error .material-symbols-outlined {
		font-size: 1rem;
	}

	.test-error {
		display: flex;
		align-items: flex-start;
		gap: 0.4rem;
		font-size: 0.8125rem;
		color: var(--color-danger);
		margin: 0;
		line-height: 1.5;
	}

	/* cards de ia */
	.ai-cards {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
	}

	.ai-card {
		display: flex;
		align-items: center;
		gap: 1rem;
		padding: 1rem 1.25rem;
		background: var(--color-surface-2);
		border: 1.5px solid transparent;
		border-radius: var(--radius-sm);
		cursor: pointer;
		text-align: left;
		transition: all 0.15s;
	}

	.ai-card:hover {
		background: var(--color-surface-3);
	}

	.ai-card.selected {
		border-color: var(--color-interactive);
		background: color-mix(in srgb, var(--color-interactive) 8%, var(--color-surface-2));
	}

	.ai-card-icon {
		font-size: 1.75rem;
		color: var(--color-neutral);
		flex-shrink: 0;
	}

	.ai-card.selected .ai-card-icon {
		color: var(--color-interactive);
	}

	.ai-card-text {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 0.2rem;
	}

	.ai-card-text strong {
		font-size: 0.9375rem;
		color: var(--color-neutral-active);
		font-weight: 600;
	}

	.ai-card-text span {
		font-size: 0.8125rem;
		color: var(--color-neutral);
		line-height: 1.4;
	}

	.ai-card-check .material-symbols-outlined {
		font-size: 1.25rem;
		color: var(--color-interactive);
	}

	/* api key */
	.api-key-section {
		display: flex;
		flex-direction: column;
		gap: 0.75rem;
		padding: 1rem;
		background: var(--color-surface-2);
		border-radius: var(--radius-sm);
	}

	.api-key-hint {
		font-size: 0.8125rem;
		color: var(--color-neutral);
		margin: 0;
	}

	.api-key-hint a {
		color: var(--color-interactive);
		text-decoration: none;
	}

	.api-key-hint a:hover {
		text-decoration: underline;
	}

	.btn-icon {
		flex-shrink: 0;
		background: var(--color-surface-2);
		color: var(--color-neutral);
		border: none;
		padding: 0.625rem 0.75rem;
		border-radius: var(--radius-sm);
		cursor: pointer;
		display: flex;
		align-items: center;
		transition: background 0.15s;
	}

	.btn-icon:hover {
		background: var(--color-surface-3);
	}

	/* mcp info */
	.mcp-info {
		padding: 1rem;
		background: var(--color-surface-2);
		border-radius: var(--radius-sm);
		font-size: 0.8125rem;
		color: var(--color-neutral);
		line-height: 1.6;
	}

	.mcp-info p {
		margin: 0;
	}

	.mcp-info code {
		background: var(--color-surface-3);
		padding: 0.1rem 0.35rem;
		border-radius: 3px;
		font-size: 0.8rem;
		color: var(--color-interactive);
	}

	/* resumo */
	.summary {
		display: flex;
		flex-direction: column;
		gap: 0.625rem;
		width: 100%;
		padding: 1.25rem;
		background: var(--color-surface-2);
		border-radius: var(--radius-sm);
	}

	.summary-row {
		display: flex;
		align-items: center;
		gap: 0.75rem;
		font-size: 0.875rem;
		color: var(--color-neutral);
	}

	.summary-row .material-symbols-outlined {
		font-size: 1.125rem;
		color: var(--color-interactive);
		flex-shrink: 0;
	}

	.summary-row em {
		opacity: 0.6;
		font-style: normal;
	}

	/* consentimento de analytics */
	.consent-row {
		display: flex;
		align-items: flex-start;
		gap: 0.625rem;
		padding: 0.875rem 1rem;
		background: var(--color-surface-2);
		border-radius: var(--radius-sm);
		font-size: 0.8125rem;
		color: var(--color-neutral);
		line-height: 1.55;
		cursor: pointer;
		width: 100%;
		text-align: left;
	}

	.consent-row input[type="checkbox"] {
		margin-top: 0.15rem;
		flex-shrink: 0;
		accent-color: var(--color-interactive);
		width: 1rem;
		height: 1rem;
	}

	/* erro de submissão */
	.submit-error {
		display: flex;
		align-items: flex-start;
		gap: 0.5rem;
		font-size: 0.8125rem;
		color: var(--color-danger);
		margin: 0;
		line-height: 1.5;
	}

	.submit-error .material-symbols-outlined {
		font-size: 1rem;
		flex-shrink: 0;
		margin-top: 0.1rem;
	}

	/* rodapé */
	.footer {
		padding: 1.25rem 1.75rem;
		display: flex;
		justify-content: flex-end;
		gap: 0.75rem;
		border-top: 1px solid color-mix(in srgb, var(--color-neutral) 10%, transparent);
	}

	.btn-primary {
		background: var(--color-interactive);
		color: var(--color-neutral-on-interactive);
		border: none;
		padding: 0.75rem 2rem;
		font-size: 0.9375rem;
		font-weight: 600;
		letter-spacing: 0.05rem;
		border-radius: var(--radius-sm);
		cursor: pointer;
		transition: all 0.15s;
	}

	.btn-primary:hover:not(:disabled) {
		background: var(--color-interactive-hover);
		transform: translateY(-1px);
		box-shadow: 0 4px 12px color-mix(in srgb, var(--color-interactive) 35%, transparent);
	}

	.btn-primary:disabled {
		background: var(--color-interactive-disabled);
		cursor: not-allowed;
		transform: none;
		box-shadow: none;
	}

	.btn-secondary {
		background: transparent;
		color: var(--color-neutral);
		border: 1.5px solid color-mix(in srgb, var(--color-neutral) 25%, transparent);
		padding: 0.75rem 1.5rem;
		font-size: 0.9375rem;
		border-radius: var(--radius-sm);
		cursor: pointer;
		transition: all 0.15s;
	}

	.btn-secondary:hover {
		background: var(--color-surface-2);
		color: var(--color-neutral-active);
	}
</style>
