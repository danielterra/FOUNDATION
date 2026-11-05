<script lang="ts">
	import { Button } from '$lib/components/ui/button';
	import { Input } from '$lib/components/ui/input';
	import * as Select from '$lib/components/ui/select';

	type Model = {
		id: string;
		name: string;
		publisher?: string | null;
		description?: string | null;
		recommendedUse?: string | null;
		recommendedUseStatus?: 'provided' | 'not_provided' | 'ambiguous';
		contextLength?: number | null;
		inputCostPerToken?: number | null;
		outputCostPerToken?: number | null;
		parameterCountTotal?: number | null;
		parameterCountActive?: number | null;
		parameterCountStatus?: 'reported' | 'not_provided_or_ambiguous';
		supportedParameters?: string[] | null;
		supportsTools?: boolean | null;
		inputModalities?: string[] | null;
		outputModalities?: string[] | null;
	};

	type Benchmark = { score: number; dimension: string; source: string; asOf: string };
	type Benchmarks = {
		available: boolean;
		availableDimensions: string[];
		source: string;
		sourceUrl: string | null;
		citation: string | null;
		asOf: string | null;
		models: { modelId: string; benchmarks: Record<string, Benchmark> }[];
		ambiguousModelIds: string[];
	};
	type SortBy = 'capability' | 'price' | 'size' | 'publisher';
	type Direction = 'asc' | 'desc';

	let {
		models = [],
		benchmarks = null,
		selectedModelId = $bindable(''),
		selectedModelIds = $bindable([]),
		selectionMode = 'single',
		loading = false,
		error = '',
		benchmarksLoading = false,
		benchmarksError = '',
		loadHint = '',
		providerName = 'OpenRouter',
		onRetry,
		idPrefix = 'openrouter-model',
	}: {
		models?: Model[];
		benchmarks?: Benchmarks | null;
		selectedModelId?: string;
		selectedModelIds?: string[];
		selectionMode?: 'single' | 'multiple';
		loading?: boolean;
		error?: string;
		benchmarksLoading?: boolean;
		benchmarksError?: string;
		loadHint?: string;
		providerName?: string;
		onRetry?: () => void;
		idPrefix?: string;
	} = $props();

	const pageSize = 25;
	const benchmarkDimensions = ['intelligence', 'coding', 'agentic'];
	let search = $state('');
	let sortBy = $state<SortBy>('price');
	let direction = $state<Direction>('asc');
	let benchmarkDimension = $state('intelligence');
	let page = $state(1);

	let selectedModel = $derived(models.find((model) => model.id === selectedModelId) ?? null);
	let filteredModels = $derived.by(() => {
		const query = search.trim().toLocaleLowerCase();
		const matched = models.filter((model) => {
			const searchable = [
				model.name,
				model.id,
				model.publisher ?? '',
				model.description ?? '',
				model.recommendedUse ?? '',
				model.supportedParameters?.join(' ') ?? '',
			].join(' ').toLocaleLowerCase();
			return !query || searchable.includes(query);
		});
		return matched.sort(compareModels);
	});
	let pageCount = $derived(Math.max(1, Math.ceil(filteredModels.length / pageSize)));
	let pageModels = $derived(filteredModels.slice((page - 1) * pageSize, page * pageSize));
	let pageRange = $derived.by(() => {
		if (filteredModels.length === 0) return '0 modelos';
		const first = (page - 1) * pageSize + 1;
		const last = Math.min(page * pageSize, filteredModels.length);
		return `Exibindo ${first}–${last} de ${filteredModels.length} modelos`;
	});

	$effect(() => {
		if (benchmarks?.availableDimensions.length && !benchmarks.availableDimensions.includes(benchmarkDimension)) {
			benchmarkDimension = benchmarks.availableDimensions[0];
		}
	});

	function compareModels(first: Model, second: Model): number {
		const firstValue = sortValue(first);
		const secondValue = sortValue(second);
		if (firstValue == null && secondValue != null) return 1;
		if (secondValue == null && firstValue != null) return -1;
		if (firstValue != null && secondValue != null) {
			const comparison = typeof firstValue === 'number' && typeof secondValue === 'number'
				? firstValue - secondValue
				: String(firstValue).localeCompare(String(secondValue), 'pt-BR', { sensitivity: 'base' });
			if (comparison !== 0) return direction === 'desc' ? -comparison : comparison;
		}
		return first.name.localeCompare(second.name, 'pt-BR', { sensitivity: 'base' }) || first.id.localeCompare(second.id);
	}

	function sortValue(model: Model): number | string | null {
		if (sortBy === 'capability') return benchmarkFor(model, benchmarkDimension)?.score ?? null;
		if (sortBy === 'price') {
			if (model.inputCostPerToken == null || model.outputCostPerToken == null) return null;
			return model.inputCostPerToken + model.outputCostPerToken;
		}
		if (sortBy === 'size') {
			if (model.parameterCountStatus !== 'reported') return null;
			return model.parameterCountTotal ?? null;
		}
		return model.publisher?.trim() || null;
	}

	function benchmarkFor(model: Model, dimension: string): Benchmark | null {
		return benchmarks?.models.find((entry) => entry.modelId === model.id)?.benchmarks[dimension] ?? null;
	}

	function handleSearch(event: Event) {
		search = (event.currentTarget as HTMLInputElement).value;
		page = 1;
	}

	function handleSortChange(value: string | undefined) {
		if (value === 'capability' || value === 'price' || value === 'size' || value === 'publisher') {
			sortBy = value;
			page = 1;
		}
	}

	function handleDirectionChange(value: string | undefined) {
		if (value === 'asc' || value === 'desc') {
			direction = value;
			page = 1;
		}
	}

	function handleDimensionChange(value: string | undefined) {
		if (value && benchmarks?.availableDimensions.includes(value)) {
			benchmarkDimension = value;
			page = 1;
		}
	}

	function handleSelectModel(event: MouseEvent) {
		const modelId = (event.currentTarget as HTMLButtonElement).dataset.modelId;
		if (!modelId) return;
		if (selectionMode === 'multiple') {
			selectedModelIds = selectedModelIds.includes(modelId)
				? selectedModelIds.filter((selectedId) => selectedId !== modelId)
				: [...selectedModelIds, modelId];
			return;
		}
		selectedModelId = modelId;
	}

	function handlePreviousPage() {
		page = Math.max(1, page - 1);
	}

	function handleNextPage() {
		page = Math.min(pageCount, page + 1);
	}

	function handleClearSearch() {
		search = '';
		page = 1;
	}

	function provider(model: Model): string {
		return model.publisher?.trim() || 'Publicador não informado';
	}

	function formatPrice(price: number | null | undefined): string {
		return price == null ? 'Não informado pelo publicador' : `$${(price * 1_000_000).toFixed(2)} / 1M tokens`;
	}

	function formatCombinedPrice(model: Model): string {
		if (model.inputCostPerToken == null || model.outputCostPerToken == null) return 'Não informado pelo publicador';
		return formatPrice(model.inputCostPerToken + model.outputCostPerToken);
	}

	function formatContext(model: Model): string {
		return model.contextLength == null ? 'Não informado pelo publicador' : `${(model.contextLength / 1000).toFixed(0)}k tokens`;
	}

	function formatSize(model: Model): string {
		if (model.parameterCountStatus !== 'reported' || model.parameterCountTotal == null) return 'Tamanho não informado pelo provedor';
		const total = `${(model.parameterCountTotal / 1_000_000_000).toLocaleString('pt-BR', { maximumFractionDigits: 1 })}B parâmetros`;
		if (model.parameterCountActive == null) return total;
		return `${total} · ${(model.parameterCountActive / 1_000_000_000).toLocaleString('pt-BR', { maximumFractionDigits: 1 })}B ativos`;
	}

	function dimensionLabel(dimension: string): string {
		if (dimension === 'intelligence') return 'Geral';
		if (dimension === 'coding') return 'Programação';
		return dimension === 'agentic' ? 'Agente' : dimension;
	}

	function formatDate(value: string): string {
		const date = new Date(value);
		return Number.isNaN(date.getTime()) ? 'Data não informada' : date.toLocaleDateString('pt-BR');
	}

	function capabilityState(model: Model): string {
		return model.supportedParameters?.length ? model.supportedParameters.join(', ') : 'Não informado pelo publicador';
	}

	function modalities(values: string[] | null | undefined): string {
		return values?.length ? values.join(', ') : 'Não informado pelo publicador';
	}

	function recommendation(model: Model): string {
		return model.recommendedUseStatus === 'provided' && model.recommendedUse?.trim()
			? model.recommendedUse
			: 'Orientação de uso não informada pelo publicador';
	}

	function benchmarkLabel(model: Model, dimension: string): string {
		const benchmark = benchmarkFor(model, dimension);
		return benchmark ? Number(benchmark.score).toLocaleString('pt-BR', { maximumFractionDigits: 1 }) : 'Sem avaliação publicada';
	}
</script>

<section class="model-selector" aria-labelledby={`${idPrefix}-heading`}>
	<header class="selector-heading">
		<div>
			<h3 id={`${idPrefix}-heading`}>Catálogo de modelos</h3>
			<p><span class="catalog-provider">{providerName}</span> · compare capacidade, tamanho e custo dos modelos disponíveis.</p>
		</div>
		{#if onRetry}
			<Button variant="ghost" size="sm" onclick={onRetry} disabled={loading}>Carregar modelos</Button>
		{/if}
	</header>

	{#if loading}
		<p class="state-message" role="status" aria-live="polite">Carregando catálogo de modelos…</p>
	{:else if error}
		<div class="state-error" role="alert" aria-live="assertive">
			<p>{error}</p>
			{#if onRetry}<Button variant="ghost" size="sm" onclick={onRetry}>Tentar novamente</Button>{/if}
		</div>
	{:else if models.length === 0}
		<div class="state-empty" role="status">
			<p>{loadHint || 'Carregue o catálogo para comparar os modelos disponíveis.'}</p>
		</div>
	{:else}
		<div class="selector-controls">
			<Input id={`${idPrefix}-search`} type="search" aria-label="Buscar modelos por nome, identificador, publicador ou descrição" placeholder="Buscar modelo, publicador ou descrição" value={search} oninput={handleSearch} />
			<div class="control-field">
				<label for={`${idPrefix}-sort`}>Ordenar por</label>
				<Select.Root type="single" value={sortBy} onValueChange={handleSortChange}>
					<Select.Trigger id={`${idPrefix}-sort`}><Select.Value /></Select.Trigger>
					<Select.Content>
						<Select.Item value="capability" disabled={!benchmarks?.available}>Capacidade avaliada</Select.Item>
						<Select.Item value="price">Preço combinado</Select.Item>
						<Select.Item value="size">Tamanho</Select.Item>
						<Select.Item value="publisher">Publicador</Select.Item>
					</Select.Content>
				</Select.Root>
			</div>
			<div class="control-field">
				<label for={`${idPrefix}-direction`}>Direção</label>
				<Select.Root type="single" value={direction} onValueChange={handleDirectionChange}>
					<Select.Trigger id={`${idPrefix}-direction`}><Select.Value /></Select.Trigger>
					<Select.Content>
						<Select.Item value="asc">Crescente</Select.Item>
						<Select.Item value="desc">Decrescente</Select.Item>
					</Select.Content>
				</Select.Root>
			</div>
			{#if sortBy === 'capability' && benchmarks?.available}
				<div class="control-field">
					<label for={`${idPrefix}-dimension`}>Dimensão de capacidade</label>
					<Select.Root type="single" value={benchmarkDimension} onValueChange={handleDimensionChange}>
						<Select.Trigger id={`${idPrefix}-dimension`}><Select.Value /></Select.Trigger>
						<Select.Content>
							{#each benchmarks.availableDimensions as dimension}
								<Select.Item value={dimension}>{dimensionLabel(dimension)}</Select.Item>
							{/each}
						</Select.Content>
					</Select.Root>
				</div>
			{/if}
		</div>

		<p class="catalog-range" role="status" aria-live="polite">{pageRange} · Página {Math.min(page, pageCount)} de {pageCount}</p>
		{#if benchmarksLoading}
			<p class="state-message" role="status" aria-live="polite">Consultando avaliações de capacidade verificáveis…</p>
		{:else if benchmarksError}
			<p class="state-message">Avaliações indisponíveis: {benchmarksError}</p>
		{:else if benchmarks?.available}
			<p class="benchmark-source">Fonte das avaliações: {benchmarks.source} · {benchmarks.asOf ? formatDate(benchmarks.asOf) : 'data não informada'}{#if benchmarks.citation} · {benchmarks.citation}{/if}</p>
		{:else}
			<p class="benchmark-source">Não há avaliações verificáveis disponíveis para ordenar por capacidade.</p>
		{/if}

		{#if filteredModels.length === 0}
			<div class="state-empty" role="status">
				<p>Nenhum modelo corresponde à busca atual.</p>
				<Button variant="ghost" size="sm" onclick={handleClearSearch}>Limpar busca</Button>
			</div>
		{:else}
			{#if selectionMode === 'multiple' && selectedModelIds.length > 0}
				<div class="selected-summary" role="status" aria-live="polite">
					<span class="material-symbols-outlined" aria-hidden="true">check_circle</span>
					<span class="selected-summary-model">
						<span class="selected-summary-label">Modelos selecionados</span>
						<span class="selected-summary-name">{selectedModelIds.length} na lista de prioridade</span>
					</span>
				</div>
			{:else if selectedModel}
				<div class="selected-summary" role="status" aria-live="polite">
					<span class="material-symbols-outlined" aria-hidden="true">check_circle</span>
					<span class="selected-summary-model">
						<span class="selected-summary-label">Modelo selecionado</span>
						<span class="selected-summary-name">{selectedModel.name}</span>
					</span>
					<span class="selected-summary-publisher">{provider(selectedModel)}</span>
				</div>
			{/if}
			{#if selectionMode === 'single' && selectedModel && !pageModels.some((model) => model.id === selectedModelId)}
				<div class="selected-note" role="status">Modelo selecionado: <span>{selectedModel.name}</span> · está fora da página ou da busca atual.</div>
			{/if}
			<div class="model-list" aria-label="Modelos disponíveis">
				{#each pageModels as model (model.id)}
					<button
						type="button"
						class="model-option"
						class:selected={selectionMode === 'multiple' ? selectedModelIds.includes(model.id) : model.id === selectedModelId}
						data-model-id={model.id}
						aria-pressed={selectionMode === 'multiple' ? selectedModelIds.includes(model.id) : model.id === selectedModelId}
						onclick={handleSelectModel}
					>
						<span class="model-title-row">
							<span class="model-name">{model.name}</span>
							{#if (selectionMode === 'multiple' && selectedModelIds.includes(model.id)) || (selectionMode === 'single' && model.id === selectedModelId)}<span class="selection-state"><span class="material-symbols-outlined" aria-hidden="true">check_circle</span>{selectionMode === 'multiple' ? `Prioridade ${selectedModelIds.indexOf(model.id) + 1}` : 'Selecionado'}</span>{/if}
						</span>
						<span class="model-publisher-row"><span class="model-publisher">{provider(model)}</span><span class="model-id">{model.id}</span></span>
						{#if model.description}<span class="model-description"><span class="metric-label">Descrição do publicador</span>{model.description}</span>{/if}
						<span class="benchmark-grid" aria-label="Avaliações de capacidade">
							{#each benchmarkDimensions as dimension}
								{@const benchmark = benchmarkFor(model, dimension)}
								<span class="benchmark-item">
									<span class="metric-label">{dimensionLabel(dimension)}</span>
									<span class="metric-value">{benchmarkLabel(model, dimension)}</span>
									{#if benchmark}<span class="metric-source">{benchmark.source} · {formatDate(benchmark.asOf)}</span>{/if}
								</span>
							{/each}
						</span>
						<span class="model-facts">
							<span><span class="metric-label">Tamanho</span>{formatSize(model)}</span>
							<span><span class="metric-label">Contexto</span>{formatContext(model)}</span>
							<span><span class="metric-label">Custo total / 1M</span>{formatCombinedPrice(model)}</span>
							<span><span class="metric-label">Entrada</span>{formatPrice(model.inputCostPerToken)}</span>
							<span><span class="metric-label">Saída</span>{formatPrice(model.outputCostPerToken)}</span>
						</span>
						<span class="model-capabilities">
							<span><span class="metric-label">Recursos declarados</span>{capabilityState(model)}</span>
							<span><span class="metric-label">Entrada</span>{modalities(model.inputModalities)}</span>
							<span><span class="metric-label">Saída</span>{modalities(model.outputModalities)}</span>
							<span><span class="metric-label">Ferramentas</span>{model.supportsTools == null ? 'Não informado pelo publicador' : model.supportsTools ? 'Suportadas' : 'Não suportadas'}</span>
						</span>
						<span class="model-guidance"><span class="metric-label">Orientação de uso</span>{recommendation(model)}</span>
					</button>
				{/each}
			</div>

			<nav class="model-pagination" aria-label="Paginação do catálogo de modelos">
				<Button variant="ghost" size="sm" onclick={handlePreviousPage} disabled={page <= 1}>Anterior</Button>
				<p role="status" aria-live="polite">{pageRange}</p>
				<Button variant="ghost" size="sm" onclick={handleNextPage} disabled={page >= pageCount}>Próxima</Button>
			</nav>
		{/if}
	{/if}
</section>

<style>
	.model-selector { display: grid; gap: var(--spacing-section); min-width: 0; }
	.selector-heading { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--space-3); }
	.selector-heading h3 { margin: 0; color: var(--text-body); font-size: var(--type-heading); font-weight: 400; }
	.selector-heading p, .state-message, .catalog-range, .benchmark-source, .model-pagination p { margin: 0; color: var(--text-secondary); font-size: var(--type-body-compact); line-height: var(--line-height-body); }
	.catalog-provider { color: var(--text-body); }
	.selector-controls { display: grid; grid-template-columns: minmax(12rem, 2fr) repeat(2, minmax(8rem, 1fr)); align-items: end; gap: var(--space-2); }
	.control-field { display: grid; gap: var(--space-1); min-width: 0; }
	.control-field label, .metric-label { color: var(--text-secondary); font-size: var(--type-label); letter-spacing: var(--tracking-hud-label); text-transform: uppercase; }
	.catalog-range { color: var(--text-body); }
	.state-error, .state-empty { display: grid; justify-items: start; gap: var(--space-2); padding: var(--space-3); border-inline-start: var(--size-hud-frame-line) solid var(--color-danger); background: var(--color-surface-2); color: var(--text-body); }
	.state-error p, .state-empty p { margin: 0; }
	.selected-summary { display: grid; grid-template-columns: auto minmax(0, 1fr) minmax(0, 1fr); align-items: center; gap: var(--space-2); padding: var(--space-2) var(--space-3); border: var(--size-hud-frame-line) solid var(--color-interactive); border-radius: var(--radius); background: var(--color-surface-2); }
	.selected-summary > .material-symbols-outlined { color: var(--color-interactive); font-size: var(--type-icon-medium); }
	.selected-summary-model { display: grid; min-width: 0; }
	.selected-summary-label { color: var(--text-secondary); font-size: var(--type-label); text-transform: uppercase; letter-spacing: var(--tracking-hud-label); }
	.selected-summary-name { color: var(--text-body); font-size: var(--type-body-prominent); overflow-wrap: anywhere; }
	.selected-summary-publisher { color: var(--text-secondary); font-size: var(--type-body-compact); text-align: end; overflow-wrap: anywhere; }
	.selected-note { padding: var(--space-2) var(--space-3); border-inline-start: var(--size-hud-frame-line) solid var(--color-interactive); background: var(--color-surface-2); color: var(--text-secondary); font-size: var(--type-body-compact); }
	.selected-note span { color: var(--text-body); }
	.model-list { display: grid; gap: var(--space-2); max-height: 34rem; overflow-y: auto; padding-inline-end: var(--space-1); }
	.model-option { display: grid; gap: var(--space-2); width: 100%; min-width: 0; padding: var(--spacing-widget-inset); border: var(--size-hud-frame-line) solid var(--color-hud-line-subtle); border-radius: var(--radius); background: var(--color-surface-1); color: var(--text-body); text-align: start; cursor: pointer; transition: border-color 140ms ease, background-color 140ms ease; }
	.model-option:hover { border-color: var(--color-hud-line); background: var(--color-surface-2); }
	.model-option:focus-visible { outline: var(--size-hud-frame-line) solid var(--color-interactive); outline-offset: 2px; }
	.model-option.selected { border-color: var(--color-interactive); background: var(--color-surface-2); }
	.model-title-row { display: flex; align-items: flex-start; justify-content: space-between; gap: var(--space-2); flex-wrap: wrap; }
	.model-name { color: var(--text-body); font-size: var(--type-title); line-height: var(--line-height-body); overflow-wrap: anywhere; }
	.model-publisher-row { display: flex; align-items: baseline; justify-content: space-between; flex-wrap: wrap; gap: var(--space-1) var(--space-3); }
	.model-publisher { color: var(--text-body); font-family: var(--font-mono); font-size: var(--type-body-compact); overflow-wrap: anywhere; }
	.model-id { color: var(--text-secondary); font-family: var(--font-mono); font-size: var(--type-label); overflow-wrap: anywhere; }
	.selection-state { display: inline-flex; align-items: center; gap: var(--space-1); color: var(--color-interactive); font-size: var(--type-label); }
	.selection-state .material-symbols-outlined { font-size: var(--type-body); }
	.benchmark-grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: var(--space-2); }
	.benchmark-item { display: grid; align-content: start; gap: var(--space-1); min-width: 0; padding: var(--space-2); border-inline-start: var(--size-hud-frame-line) solid var(--color-hud-line-subtle); background: var(--color-surface-2); }
	.metric-value { color: var(--text-body); font-size: var(--type-body-compact); overflow-wrap: anywhere; }
	.metric-source { color: var(--text-secondary); font-size: var(--type-label); overflow-wrap: anywhere; }
	.model-facts, .model-capabilities { display: grid; grid-template-columns: repeat(auto-fit, minmax(8rem, 1fr)); gap: var(--space-2); }
	.model-facts > span, .model-capabilities > span, .model-guidance, .model-description { display: grid; gap: var(--space-1); min-width: 0; color: var(--text-body); font-size: var(--type-body-compact); overflow-wrap: anywhere; }
	.model-guidance, .model-description { padding-inline-start: var(--space-2); border-inline-start: var(--size-hud-frame-line) solid var(--color-hud-line-subtle); }
	.model-pagination { display: flex; align-items: center; justify-content: space-between; gap: var(--space-2); }
	.model-pagination p { text-align: center; }
	@media (max-width: 42rem) {
		.selector-controls { grid-template-columns: minmax(0, 1fr); }
		.benchmark-grid { grid-template-columns: minmax(0, 1fr); }
		.model-pagination { flex-wrap: wrap; }
		.selected-summary { grid-template-columns: auto minmax(0, 1fr); }
		.selected-summary-publisher { grid-column: 2; text-align: start; }
	}
</style>
