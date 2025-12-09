<script>
	let {
		label = '',
		comment = '',
		value = '',
		onValueClick = null
	} = $props();
</script>

<div class="property-row">
	<span class="property-label" title={comment || undefined}>
		{label}
	</span>
	<span class="property-value" class:clickable={onValueClick} onclick={onValueClick}>
		{value}
	</span>
</div>

<style>
	.property-row {
		display: flex;
		align-items: baseline;
		gap: 16px;
		padding: 10px 0;
		border-bottom: 1px solid color-mix(in srgb, var(--color-border) 30%, transparent);
	}

	.property-row:last-child {
		border-bottom: none;
	}

	.property-label {
		font-size: 13px;
		font-weight: 600;
		color: var(--color-neutral);
		text-transform: uppercase;
		letter-spacing: 0.5px;
		flex-shrink: 0;
		min-width: 120px;
		cursor: help;
	}

	.property-value {
		font-size: 14px;
		color: var(--color-neutral);
		word-break: break-word;
		line-height: 1.5;
		flex: 1;
	}

	.property-value.clickable {
		color: var(--color-interactive);
		cursor: pointer;
		text-decoration: underline;
		text-decoration-color: color-mix(in srgb, var(--color-interactive) 30%, transparent);
		transition: text-decoration-color 0.2s;
	}

	.property-value.clickable:hover {
		text-decoration-color: var(--color-interactive);
	}
</style>
