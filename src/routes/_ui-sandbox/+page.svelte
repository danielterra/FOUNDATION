<script lang="ts">
	import * as Button from "$lib/components/ui/button/index.js";
	import * as Card from "$lib/components/ui/card/index.js";
	import * as Dialog from "$lib/components/ui/dialog/index.js";
	import { Input } from "$lib/components/ui/input/index.js";
	import { Badge } from "$lib/components/ui/badge/index.js";

	let dialogOpen = $state(false);
</script>

<svelte:head>
	<title>UI Sandbox — FOUNDATION</title>
</svelte:head>

<main class="min-h-screen bg-background text-foreground p-8 space-y-12">
	<header class="space-y-1">
		<h1 class="text-2xl font-semibold">UI Sandbox — Referência de Surfaces e Primitivas</h1>
		<p class="text-muted-foreground text-sm">
			Validação visual da hierarquia de elevação dark e dos tokens shadcn antes de qualquer
			migração de componente de domínio.
		</p>
	</header>

	<!-- Hierarquia de superfícies -->
	<section class="space-y-4">
		<h2 class="text-lg font-semibold border-b border-border pb-2">Superfícies — Elevação Crescente</h2>
		<p class="text-muted-foreground text-sm">
			Cada nível deve ser visivelmente mais claro que o anterior.
			background → card → muted → popover → accent
		</p>

		<div class="flex flex-wrap gap-4 items-start">
			<!-- --background = surface-0 (preto puro) -->
			<div class="flex flex-col gap-1">
				<span class="text-xs text-muted-foreground font-mono">--background</span>
				<span class="text-xs text-muted-foreground">surface-0</span>
				<div
					class="w-32 h-20 rounded-[var(--radius)] border border-border flex items-center justify-center"
					style="background: var(--background);"
				>
					<span class="text-xs text-muted-foreground">base</span>
				</div>
			</div>

			<!-- --card = surface-1 -->
			<div class="flex flex-col gap-1">
				<span class="text-xs text-muted-foreground font-mono">--card</span>
				<span class="text-xs text-muted-foreground">surface-1</span>
				<div
					class="w-32 h-20 rounded-[var(--radius)] border border-border flex items-center justify-center"
					style="background: var(--card);"
				>
					<span class="text-xs text-muted-foreground">painel</span>
				</div>
			</div>

			<!-- --muted = surface-2 -->
			<div class="flex flex-col gap-1">
				<span class="text-xs text-muted-foreground font-mono">--muted</span>
				<span class="text-xs text-muted-foreground">surface-2</span>
				<div
					class="w-32 h-20 rounded-[var(--radius)] border border-border flex items-center justify-center"
					style="background: var(--muted);"
				>
					<span class="text-xs text-muted-foreground">card/input</span>
				</div>
			</div>

			<!-- --popover = surface-3 -->
			<div class="flex flex-col gap-1">
				<span class="text-xs text-muted-foreground font-mono">--popover</span>
				<span class="text-xs text-muted-foreground">surface-3</span>
				<div
					class="w-32 h-20 rounded-[var(--radius)] border border-border flex items-center justify-center"
					style="background: var(--popover);"
				>
					<span class="text-xs text-muted-foreground">modal</span>
				</div>
			</div>

			<!-- --accent = surface-4 -->
			<div class="flex flex-col gap-1">
				<span class="text-xs text-muted-foreground font-mono">--accent</span>
				<span class="text-xs text-muted-foreground">surface-4</span>
				<div
					class="w-32 h-20 rounded-[var(--radius)] border border-border flex items-center justify-center"
					style="background: var(--accent);"
				>
					<span class="text-xs text-muted-foreground">item elevado</span>
				</div>
			</div>
		</div>

		<!-- Aninhamento visual para confirmar hierarquia -->
		<div class="bg-background p-4 rounded-[var(--radius)] border border-border w-fit">
			<span class="text-xs text-muted-foreground block mb-2">Aninhamento (background → card → muted → popover → accent)</span>
			<div class="bg-card p-4 rounded-[var(--radius)]">
				<div class="bg-muted p-4 rounded-[var(--radius)]">
					<div class="bg-popover p-4 rounded-[var(--radius)]">
						<div class="bg-accent p-3 rounded-[var(--radius)] text-xs text-accent-foreground text-center">
							accent (surface-4)
						</div>
					</div>
				</div>
			</div>
		</div>
	</section>

	<!-- Tokens de texto -->
	<section class="space-y-4">
		<h2 class="text-lg font-semibold border-b border-border pb-2">Tokens de Texto</h2>
		<div class="space-y-2">
			<p class="text-foreground">text-foreground — texto principal (--color-neutral)</p>
			<p class="text-muted-foreground">text-muted-foreground — texto secundário (--color-neutral-disabled)</p>
			<p class="text-primary">text-primary — laranja interativo (--color-interactive)</p>
			<p class="text-destructive">text-destructive — vermelho perigo (--color-danger)</p>
		</div>
	</section>

	<!-- Botões -->
	<section class="space-y-4">
		<h2 class="text-lg font-semibold border-b border-border pb-2">Botões shadcn</h2>
		<p class="text-muted-foreground text-sm">
			Use Tab para navegar e verificar o anel de foco (--ring = laranja interativo).
		</p>
		<div class="flex flex-wrap gap-3 items-center">
			<Button.Button variant="default">Default</Button.Button>
			<Button.Button variant="destructive">Destructive</Button.Button>
			<Button.Button variant="outline">Outline</Button.Button>
			<Button.Button variant="secondary">Secondary</Button.Button>
			<Button.Button variant="ghost">Ghost</Button.Button>
			<Button.Button variant="link">Link</Button.Button>
			<Button.Button disabled>Disabled</Button.Button>
		</div>
		<div class="flex flex-wrap gap-3 items-center">
			<Button.Button size="sm">Small</Button.Button>
			<Button.Button size="default">Default</Button.Button>
			<Button.Button size="lg">Large</Button.Button>
			<Button.Button size="icon" aria-label="Ícone fechar">
				<span class="material-symbols-outlined" style="font-size:18px;">close</span>
			</Button.Button>
		</div>
	</section>

	<!-- Badges -->
	<section class="space-y-4">
		<h2 class="text-lg font-semibold border-b border-border pb-2">Badges</h2>
		<div class="flex flex-wrap gap-3 items-center">
			<Badge variant="default">Default</Badge>
			<Badge variant="secondary">Secondary</Badge>
			<Badge variant="destructive">Destructive</Badge>
			<Badge variant="outline">Outline</Badge>
		</div>
	</section>

	<!-- Input -->
	<section class="space-y-4">
		<h2 class="text-lg font-semibold border-b border-border pb-2">Input shadcn</h2>
		<p class="text-muted-foreground text-sm">
			Clique no campo e verifique o anel de foco laranja (--ring).
		</p>
		<div class="max-w-sm space-y-3">
			<Input placeholder="Placeholder de exemplo..." />
			<Input value="Com valor preenchido" />
			<Input disabled placeholder="Desabilitado..." />
		</div>
	</section>

	<!-- Card -->
	<section class="space-y-4">
		<h2 class="text-lg font-semibold border-b border-border pb-2">Card shadcn</h2>
		<div class="max-w-sm">
			<Card.Card>
				<Card.CardHeader>
					<Card.CardTitle>Título do Card</Card.CardTitle>
					<Card.CardDescription>
						Descrição secundária com text-muted-foreground.
					</Card.CardDescription>
				</Card.CardHeader>
				<Card.CardContent>
					<p class="text-sm">Conteúdo do card sobre a superfície --card (surface-1).</p>
				</Card.CardContent>
				<Card.CardFooter>
					<Button.Button size="sm" variant="outline">Ação</Button.Button>
				</Card.CardFooter>
			</Card.Card>
		</div>
	</section>

	<!-- Dialog -->
	<section class="space-y-4">
		<h2 class="text-lg font-semibold border-b border-border pb-2">Dialog shadcn</h2>
		<p class="text-muted-foreground text-sm">
			Abra o dialog e verifique: overlay escuro, foco preso dentro (Tab circula),
			botão fechar com ícone Material Symbols, background do conteúdo = --background (surface-0).
		</p>

		<Dialog.Dialog bind:open={dialogOpen}>
			<Dialog.DialogTrigger>
				<Button.Button onclick={() => (dialogOpen = true)}>Abrir Dialog</Button.Button>
			</Dialog.DialogTrigger>
			<Dialog.DialogContent>
				<Dialog.DialogHeader>
					<Dialog.DialogTitle>Dialog de Referência</Dialog.DialogTitle>
					<Dialog.DialogDescription>
						Overlay com bg-black/50 sobre a tela. Conteúdo sobre --background. Feche com
						Escape ou o botão X (Material Symbols).
					</Dialog.DialogDescription>
				</Dialog.DialogHeader>
				<div class="space-y-3">
					<Input placeholder="Input dentro do dialog..." />
					<div class="flex gap-2">
						<Badge>Token ok</Badge>
						<Badge variant="destructive">Perigo</Badge>
					</div>
				</div>
				<Dialog.DialogFooter>
					<Dialog.DialogClose>
						<Button.Button variant="outline">Cancelar</Button.Button>
					</Dialog.DialogClose>
					<Button.Button>Confirmar</Button.Button>
				</Dialog.DialogFooter>
			</Dialog.DialogContent>
		</Dialog.Dialog>
	</section>

	<!-- Mapeamento de tokens -->
	<section class="space-y-4">
		<h2 class="text-lg font-semibold border-b border-border pb-2">Mapeamento Token → CSS Var</h2>
		<div class="overflow-x-auto">
			<table class="text-sm w-full border-collapse">
				<thead>
					<tr class="bg-muted text-muted-foreground">
						<th class="text-left p-2 border border-border">Token shadcn</th>
						<th class="text-left p-2 border border-border">CSS Var Semântica</th>
						<th class="text-left p-2 border border-border">Papel</th>
					</tr>
				</thead>
				<tbody>
					{#each [
						["--background", "--color-surface-0", "Fundo da app"],
						["--card", "--color-surface-1", "Painéis / widgets"],
						["--muted", "--color-surface-2", "Cards / inputs"],
						["--popover", "--color-surface-3", "Modais / dropdowns"],
						["--accent", "--color-surface-4", "Itens em elevações"],
						["--foreground", "--color-neutral", "Texto principal"],
						["--muted-foreground", "--color-neutral-disabled", "Texto secundário"],
						["--primary", "--color-interactive", "Ação interativa (laranja)"],
						["--primary-foreground", "--color-neutral-on-interactive", "Texto sobre laranja"],
						["--destructive", "--color-danger", "Ação destrutiva / erro"],
						["--border", "--color-surface-3", "Bordas"],
						["--input", "--color-surface-2", "Fundo de inputs"],
						["--ring", "--color-interactive", "Foco-visível unificado"],
						["--radius", "--radius (10px)", "Raio de borda"],
					] as [token, cssVar, papel]}
						<tr class="hover:bg-muted/50">
							<td class="p-2 border border-border font-mono text-xs text-primary">{token}</td>
							<td class="p-2 border border-border font-mono text-xs text-muted-foreground">{cssVar}</td>
							<td class="p-2 border border-border text-xs">{papel}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	</section>
</main>
