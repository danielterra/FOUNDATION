<script lang="ts">
	import { AlertDialog as AlertDialogPrimitive } from "bits-ui";
	import type { Snippet } from "svelte";
	import AlertDialogOverlay from "./alert-dialog-overlay.svelte";
	import { cn } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		class: className,
		children,
		...restProps
	}: AlertDialogPrimitive.ContentProps & {
		ref?: HTMLDivElement | null;
		children: Snippet;
	} = $props();
</script>

<AlertDialogPrimitive.Portal>
	<AlertDialogOverlay />
	<AlertDialogPrimitive.Content
		bind:ref
		data-slot="alert-dialog-content"
		class={cn(
			"fixed top-1/2 left-1/2 z-dialog",
			"-translate-x-1/2 -translate-y-1/2",
			"w-full max-w-md",
			"bg-[var(--color-surface-3)] border border-[var(--color-surface-4)]",
			"rounded-[var(--radius)] shadow-2xl",
			"p-6 flex flex-col gap-4",
			"data-[state=open]:animate-in data-[state=closed]:animate-out",
			"data-[state=closed]:fade-out-0 data-[state=open]:fade-in-0",
			"data-[state=closed]:zoom-out-95 data-[state=open]:zoom-in-95",
			"duration-200",
			className
		)}
		{...restProps}
	>
		{@render children?.()}
	</AlertDialogPrimitive.Content>
</AlertDialogPrimitive.Portal>
