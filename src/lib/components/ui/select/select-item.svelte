<script lang="ts">
	import { Select as SelectPrimitive } from "bits-ui";
	import { cn } from "$lib/utils.js";
	import type { Snippet } from "svelte";

	type Props = {
		ref?: HTMLDivElement | null;
		class?: string;
		value: string;
		label?: string;
		disabled?: boolean;
		children?: Snippet;
	};

	let {
		ref = $bindable(null),
		class: className,
		children: label,
		...restProps
	}: Props = $props();
</script>

<SelectPrimitive.Item
	bind:ref
	data-slot="select-item"
	class={cn(
		"focus:bg-accent focus:text-accent-foreground data-[disabled]:pointer-events-none data-[disabled]:opacity-50 relative flex w-full cursor-default items-center gap-2 rounded-sm py-1.5 pr-8 pl-2 text-sm outline-none select-none",
		className
	)}
	{...restProps}
>
	{#snippet children({ selected })}
		<span class="absolute right-2 flex size-3.5 items-center justify-center">
			{#if selected}
				<span class="material-symbols-outlined" style="font-size:14px;line-height:1;">check</span>
			{/if}
		</span>
		{@render label?.()}
	{/snippet}
</SelectPrimitive.Item>
