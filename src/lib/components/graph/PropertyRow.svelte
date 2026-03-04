<script>
	let {
		label = '',
		comment = '',
		value = '',
		valueLabel = null,
		valueIcon = null,
		onValueClick = null,
		unit = null,
		unitLabel = null
	} = $props();

	function getIconType(icon) {
		if (!icon) return null;
		if (icon.startsWith('http://') || icon.startsWith('https://') ||
		    icon.startsWith('file://') || icon.startsWith('data:')) {
			return 'image';
		}
		return 'material-symbol';
	}

	const displayValue = valueLabel || value;
	const iconType = valueIcon ? getIconType(valueIcon) : null;
</script>

<div class="property-row">
	<div class="property-label" title={comment || undefined}>
		{label}
	</div>
	<div class="property-value-container">
		{#if valueIcon}
			<!-- Pill with icon on the left -->
			<div class="value-pill" class:clickable={onValueClick} onclick={onValueClick}>
				<span class="value-icon">
					{#if iconType === 'image'}
						<img src={valueIcon} alt={displayValue} />
					{:else}
						<span class="material-symbols-outlined">{valueIcon}</span>
					{/if}
				</span>
				{#if unitLabel}
					<span class="unit-badge">{unitLabel}</span>
				{/if}
				<span class="value-text">{displayValue}</span>
			</div>
		{:else}
			<!-- Plain value without icon -->
			<div class="value-pill" class:clickable={onValueClick} onclick={onValueClick}>
				{#if unitLabel}
					<span class="unit-badge">{unitLabel}</span>
				{/if}
				<span class="value-text">{displayValue}</span>
			</div>
		{/if}
	</div>
</div>

<style>
	.property-row {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 10px 0;
		border-bottom: 1px solid color-mix(in srgb, var(--color-border) 20%, transparent);
	}

	.property-row:last-child {
		border-bottom: none;
	}

	.property-label {
		font-size: 9px;
		font-weight: 700;
		color: var(--color-neutral-disabled);
		letter-spacing: 0.5px;
		text-transform: uppercase;
		cursor: help;
	}

	.property-value-container {
		display: flex;
		align-items: center;
	}

	/* Base pill style */
	.value-pill {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		padding: 6px 10px;
		font-size: 13px;
		line-height: 1.4;
		word-break: break-word;
		max-width: 100%;
		border-radius: 8px;
		background: color-mix(in srgb, var(--color-neutral) 12%, black);
		border: 1px solid color-mix(in srgb, var(--color-neutral) 20%, black);
		color: var(--color-neutral);
	}

	/* Clickable state */
	.value-pill.clickable {
		cursor: pointer;
		transition: all 0.2s;
	}

	.value-pill.clickable:hover {
		background: color-mix(in srgb, var(--color-neutral) 8%, transparent);
	}

	.value-pill.clickable .value-icon .material-symbols-outlined {
		color: var(--color-interactive);
	}

	.value-pill.clickable:hover .value-icon .material-symbols-outlined {
		color: color-mix(in srgb, var(--color-interactive) 80%, white);
	}

	.value-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 16px;
		height: 16px;
		flex-shrink: 0;
	}

	.value-icon img {
		width: 100%;
		height: 100%;
		object-fit: cover;
		border-radius: 50%;
	}

	.value-icon .material-symbols-outlined {
		font-size: 16px;
		color: var(--color-neutral);
	}

	.value-text {
		color: var(--color-neutral);
	}

	.unit-badge {
		font-size: 11px;
		color: var(--color-neutral);
		padding: 2px 6px;
		background: color-mix(in srgb, var(--color-white) 5%, transparent);
		border-radius: 4px;
		flex-shrink: 0;
	}
</style>
