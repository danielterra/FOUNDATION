<script lang="ts">
	import { Select as SelectPrimitive } from "bits-ui";
	import { cn } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		children,
		...restProps
	}: SelectPrimitive.GroupHeadingProps & { ref?: HTMLDivElement | null } = $props();
</script>

<SelectPrimitive.GroupHeading
	bind:ref
	data-slot="select-label"
	class={cn("text-muted-foreground px-2 py-1.5 text-xs font-medium", className)}
	{...restProps}
>
	{@render children?.()}
</SelectPrimitive.GroupHeading>
