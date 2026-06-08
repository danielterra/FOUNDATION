<script lang="ts">
	import { Select as SelectPrimitive } from "bits-ui";
	import { cn } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		children,
		...restProps
	}: SelectPrimitive.TriggerProps & { ref?: HTMLButtonElement | null } = $props();
</script>

<SelectPrimitive.Trigger
	bind:ref
	data-slot="select-trigger"
	class={cn(
		"border-input data-[placeholder]:text-muted-foreground [&_span]:line-clamp-1 bg-background dark:bg-input/30 flex h-9 w-full items-center justify-between gap-2 rounded-md border px-3 py-2 text-sm shadow-xs outline-none transition-[color,box-shadow] disabled:cursor-not-allowed disabled:opacity-50",
		"focus:border-ring focus:ring-ring/50 focus:ring-[3px]",
		"aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive",
		className
	)}
	{...restProps}
>
	{@render children?.()}
	<span class="material-symbols-outlined pointer-events-none shrink-0 opacity-50" style="font-size:16px;">expand_more</span>
</SelectPrimitive.Trigger>
