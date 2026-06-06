<script>
	let {
		variant = 'primary',
		size = 'md',
		disabled = false,
		type = 'button',
		title = null,
		onclick = null,
		icon = null,
		children = null,
	} = $props();

	const iconOnly = $derived(icon && !children);
</script>

<button
	class="btn btn-{variant} btn-{size}"
	class:btn-icon-only={iconOnly}
	{type}
	{disabled}
	{title}
	{onclick}
>
	{#if icon}
		<span class="material-symbols-outlined btn-icon">{icon}</span>
	{/if}
	{#if children}
		{@render children()}
	{/if}
</button>

<style>
	.btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 6px;
		font-family: var(--font-body);
		font-weight: 500;
		cursor: pointer;
		white-space: nowrap;
		flex-shrink: 0;
		border: none;
		border-radius: var(--radius);
	}

	.btn:disabled {
		opacity: 0.45;
		cursor: not-allowed;
	}

	.btn-sm {
		padding: 4px 10px;
		font-size: 12px;
	}

	.btn-md {
		padding: 7px 14px;
		font-size: 14px;
	}

	.btn-icon-only.btn-sm { padding: 4px; }
	.btn-icon-only.btn-md { padding: 6px; }

	.btn-primary {
		background: color-mix(in srgb, var(--color-interactive) 15%, transparent);
		color: var(--color-interactive);
	}

	.btn-danger {
		background: color-mix(in srgb, var(--color-danger) 15%, transparent);
		color: var(--color-danger);
	}

	.btn-icon-only {
		background: none;
	}

	.btn-icon-only:hover:not(:disabled) {
		background: color-mix(in srgb, var(--color-white) 8%, transparent);
	}

	.btn-icon {
		font-size: 18px;
	}

	.btn-sm .btn-icon { font-size: 14px; }
</style>
