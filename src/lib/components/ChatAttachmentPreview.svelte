<script>
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
				<button
					class="remove-attachment"
					onclick={() => onRemove(attachment.iri)}
					aria-label="Remove attachment"
				>
					<span class="material-symbols-outlined">close</span>
				</button>
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
		background: color-mix(in srgb, var(--color-white) 5%, transparent);
		border: 1px solid color-mix(in srgb, var(--color-white) 15%, transparent);
		border-radius: 8px;
	}

	.attachment-item {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px;
		background: color-mix(in srgb, var(--color-white) 8%, transparent);
		border-radius: 6px;
		font-size: 13px;
	}

	.attachment-item .material-symbols-outlined {
		font-size: 20px;
		color: var(--color-interactive);
	}

	.attachment-name {
		flex: 1;
		color: var(--color-neutral-active);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.attachment-size {
		color: var(--color-neutral);
		font-size: 11px;
	}

	.remove-attachment {
		width: 24px;
		height: 24px;
		border-radius: 4px;
		background: transparent;
		border: none;
		color: var(--color-neutral);
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		transition: all 0.2s;
	}

	.remove-attachment:hover {
		background: color-mix(in srgb, var(--color-white) 10%, transparent);
		color: #f44336;
	}

	.remove-attachment .material-symbols-outlined {
		font-size: 16px;
	}
</style>
