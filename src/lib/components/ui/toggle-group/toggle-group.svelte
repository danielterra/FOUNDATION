<script lang="ts">
	import { ToggleGroup as ToggleGroupPrimitive } from "bits-ui";
	import { cn } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		...restProps
	}: ToggleGroupPrimitive.RootProps & { ref?: HTMLDivElement | null } = $props();
</script>

<ToggleGroupPrimitive.Root
	bind:ref
	data-slot="toggle-group"
	class={cn(
		"flex flex-wrap gap-1",
		className
	)}
	{...restProps}
/>
