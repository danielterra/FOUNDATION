<script>
	import { Button } from '$lib/components/ui/button';

	let { pendingAttachments, onRemove } = $props();

	function formatFileSize(bytes) {
		if (bytes === 0) return '0 Bytes';
		const k = 1024;
		const sizes = ['Bytes', 'KB', 'MB', 'GB'];
		const i = Math.floor(Math.log(bytes) / Math.log(k));
		return Math.round(bytes / Math.pow(k, i) * 100) / 100 + ' ' + sizes[i];
	}
</script>

{#if pendingAttachments.length > 0}
	<div class="attachments-preview">
		{#each pendingAttachments as attachment}
			<div class="attachment-item">
				<span class="material-symbols-outlined">
					{attachment.mimeType.startsWith('image/') ? 'image' : 'picture_as_pdf'}
				</span>
				<span class="attachment-name">{attachment.fileName}</span>
				<span class="attachment-size">{formatFileSize(attachment.fileSize)}</span>
				<Button
					variant="ghost"
					size="icon-sm"
					onclick={() => onRemove(attachment.iri)}
					aria-label="Remove attachment"
					class="remove-attachment"
				>
					<span class="material-symbols-outlined">close</span>
				</Button>
			</div>
		{/each}
	</div>
{/if}

<style>
	.attachments-preview {
		display: flex;
		flex-direction: column;
		gap: 8px;
		margin-bottom: 12px;
		padding: 12px;
		background: var(--muted);
		border-radius: var(--radius);
	}

	.attachment-item {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px;
		background: var(--input);
		border-radius: var(--radius);
		font-size: 14px;
	}

	.attachment-item .material-symbols-outlined {
		font-size: 20px;
		color: var(--primary);
	}

	.attachment-name {
		flex: 1;
		color: var(--accent-foreground);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.attachment-size {
		color: var(--foreground);
		font-size: 11px;
	}

	:global(.remove-attachment:hover) {
		color: var(--destructive) !important;
	}
</style>
