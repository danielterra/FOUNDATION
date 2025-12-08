<script>
	let { shortcuts = [] } = $props();

	// Split keys and format for display
	function parseKeys(keyString) {
		const isMac = navigator.platform.toUpperCase().indexOf('MAC') >= 0;
		return keyString.split('+').map(key => {
			if (key === 'CMD') return isMac ? '⌘' : 'Ctrl';
			if (key === 'CTRL') return 'Ctrl';
			return key;
		});
	}
</script>

<div class="shortcuts-bar">
	{#each shortcuts as shortcut}
		<div class="shortcut">
			<div class="keys">
				{#each parseKeys(shortcut.keys) as key}
					<kbd class="key" class:symbol={key === '⌘'}>{key}</kbd>
				{/each}
			</div>
			<span class="label">{shortcut.label}</span>
		</div>
	{/each}
</div>

<style>
	.shortcuts-bar {
		position: fixed;
		bottom: 0;
		left: 0;
		right: 0;
		display: flex;
		justify-content: center;
		align-items: center;
		gap: 32px;
		padding: 12px 16px;
		background: color-mix(in srgb, var(--color-black) 95%, transparent);
		backdrop-filter: blur(8px);
		border-top: 1px solid color-mix(in srgb, var(--color-neutral) 10%, transparent);
		z-index: 1000;
		font-family: 'Science Gothic', sans-serif;
	}

	.shortcut {
		display: flex;
		align-items: center;
		gap: 10px;
		font-size: 14px;
		color: var(--color-neutral);
	}

	.keys {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.key {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		min-width: 32px;
		height: 32px;
		padding: 0 10px;
		background: linear-gradient(180deg,
			color-mix(in srgb, var(--color-neutral) 12%, transparent) 0%,
			color-mix(in srgb, var(--color-neutral) 8%, transparent) 100%);
		border: 1px solid color-mix(in srgb, var(--color-neutral) 25%, transparent);
		border-bottom-width: 2px;
		border-radius: 5px;
		box-shadow:
			0 1px 0 0 color-mix(in srgb, var(--color-neutral) 20%, transparent),
			inset 0 1px 0 0 color-mix(in srgb, var(--color-neutral) 15%, transparent);
		font-weight: 600;
		font-family: 'SF Mono', Monaco, 'Cascadia Code', 'Roboto Mono', Consolas, 'Courier New', monospace;
		font-size: 13px;
		font-style: normal;
		letter-spacing: 0.3px;
		color: var(--color-neutral);
		text-transform: uppercase;
	}

	.key.symbol {
		font-size: 18px;
		font-weight: 400;
	}

	.label {
		opacity: 0.9;
		font-size: 14px;
	}
</style>
