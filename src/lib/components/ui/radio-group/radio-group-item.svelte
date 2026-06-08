<script lang="ts">
	import { RadioGroup as RadioGroupPrimitive } from "bits-ui";
	import { cn } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		...restProps
	}: RadioGroupPrimitive.ItemProps & { ref?: HTMLButtonElement | null } = $props();
</script>

<RadioGroupPrimitive.Item
	bind:ref
	data-slot="radio-group-item"
	class={cn(
		"border-input text-primary focus-visible:ring-ring/50 aria-invalid:ring-destructive/20 dark:aria-invalid:ring-destructive/40 aria-invalid:border-destructive aspect-square size-4 shrink-0 rounded-full border shadow-xs outline-none transition-[color,box-shadow] disabled:cursor-not-allowed disabled:opacity-50",
		"focus-visible:ring-[3px] focus-visible:border-ring",
		"data-[state=checked]:bg-primary data-[state=checked]:border-primary",
		className
	)}
	{...restProps}
>
	{#snippet children({ checked })}
		<span class="flex items-center justify-center">
			{#if checked}
				<span class="bg-background size-1.5 rounded-full block"></span>
			{/if}
		</span>
	{/snippet}
</RadioGroupPrimitive.Item>
