<script>
	import Button from './Button.svelte';

	let {
		open = false,
		title = '',
		icon = null,
		size = 'md',
		onClose,
		children,
		footer = null,
	} = $props();

	function handleKeydown(e) {
		if (e.key === 'Escape' && open) onClose?.();
	}

	function onBackdrop(e) {
		if (e.target === e.currentTarget) onClose?.();
	}
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open}
	<div class="modal-overlay" role="presentation" onclick={onBackdrop}>
		<div class="modal-panel modal-{size}" role="dialog" aria-modal="true" aria-label={title}>
			<div class="modal-header">
				<h3 class="section-title">
					{#if icon}<span class="material-symbols-outlined">{icon}</span>{/if}
					{title}
				</h3>
				<Button icon="close" title="Fechar" onclick={onClose} />
			</div>
			<div class="modal-body">
				{@render children?.()}
			</div>
			{#if footer}
				<div class="modal-footer">
					{@render footer()}
				</div>
			{/if}
		</div>
	</div>
{/if}

<style>
	.modal-overlay {
		position: fixed;
		inset: 0;
		z-index: 99999;
		background: color-mix(in srgb, var(--color-black) 45%, transparent);
		backdrop-filter: blur(4px);
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.modal-panel {
		background: var(--color-surface-2);
		border-radius: var(--radius);
		display: flex;
		flex-direction: column;
		max-height: 85vh;
		overflow: hidden;
	}

	.modal-sm { width: 360px; max-width: 95vw; }
	.modal-md { width: 480px; max-width: 95vw; }
	.modal-lg { width: 640px; max-width: 95vw; }

	.modal-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 16px 18px 12px;
		flex-shrink: 0;
		gap: 12px;
	}

	.modal-body {
		overflow-y: auto;
		padding: 0 18px 18px;
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.modal-footer {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		padding: 12px 18px 16px;
		flex-shrink: 0;
	}
</style>
