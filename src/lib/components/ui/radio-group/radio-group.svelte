<script lang="ts">
	import { RadioGroup as RadioGroupPrimitive } from "bits-ui";
	import { cn } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		children,
		...restProps
	}: RadioGroupPrimitive.RootProps & { ref?: HTMLDivElement | null } = $props();
</script>

<RadioGroupPrimitive.Root
	bind:ref
	data-slot="radio-group"
	class={cn("flex flex-col gap-2", className)}
	{...restProps}
>
	{@render children?.()}
</RadioGroupPrimitive.Root>
