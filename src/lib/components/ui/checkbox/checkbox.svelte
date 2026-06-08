<script lang="ts">
	import { cn, type WithElementRef } from "$lib/utils.js";
	import type { HTMLInputAttributes } from "svelte/elements";

	type Props = WithElementRef<Omit<HTMLInputAttributes, "type">, HTMLInputElement> & {
		checked?: boolean;
		indeterminate?: boolean;
	};

	let {
		ref = $bindable(null),
		checked = $bindable(false),
		indeterminate = false,
		class: className,
		...restProps
	}: Props = $props();
</script>

<span class={cn("relative inline-flex items-center justify-center", className)}>
	<input
		bind:this={ref}
		type="checkbox"
		class="peer sr-only"
		bind:checked
		{...restProps}
	/>
	<span
		class={cn(
			"border-input bg-background flex size-4 shrink-0 items-center justify-center rounded-sm border shadow-xs transition-[color,box-shadow,background]",
			"peer-focus-visible:border-ring peer-focus-visible:ring-ring/50 peer-focus-visible:ring-[3px]",
			"peer-checked:bg-primary peer-checked:border-primary peer-checked:text-primary-foreground",
			"peer-disabled:cursor-not-allowed peer-disabled:opacity-50",
			"aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive"
		)}
		aria-hidden="true"
	>
		{#if indeterminate}
			<span class="material-symbols-outlined" style="font-size:12px;line-height:1;">remove</span>
		{:else if checked}
			<span class="material-symbols-outlined" style="font-size:12px;line-height:1;">check</span>
		{/if}
	</span>
</span>
