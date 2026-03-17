<script>
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { openPath } from '@tauri-apps/plugin-opener';

  let { entityData } = $props();

  function formatFileSize(bytes) {
    if (!bytes) return '0 B';
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(1024));
    return `${(bytes / Math.pow(1024, i)).toFixed(2)} ${sizes[i]}`;
  }

  function isFile() {
    return entityData?.types?.some(t => t.iri === 'foundation:File');
  }

  function getRawFilePath() {
    const prop = entityData?.properties?.find(p => p.property === 'foundation:filePath');
    return prop?.value;
  }

  function getMimeType() {
    const prop = entityData?.properties?.find(p => p.property === 'foundation:mimeType');
    if (prop?.value) return prop.value;

    const hasFileType = entityData?.properties?.find(p => p.property === 'foundation:hasFileType');
    if (hasFileType?.value) {
      const fileTypeToMime = {
        'foundation:FileType_PDF': 'application/pdf',
        'foundation:FileType_PNG': 'image/png',
        'foundation:FileType_JPEG': 'image/jpeg',
        'foundation:FileType_JPG': 'image/jpeg',
        'foundation:FileType_GIF': 'image/gif',
        'foundation:FileType_WEBP': 'image/webp',
      };
      return fileTypeToMime[hasFileType.value];
    }

    const fileName = getFileName();
    if (fileName) {
      const ext = fileName.split('.').pop()?.toLowerCase();
      const extToMime = { jpg: 'image/jpeg', jpeg: 'image/jpeg', png: 'image/png', gif: 'image/gif', webp: 'image/webp', pdf: 'application/pdf' };
      if (ext && extToMime[ext]) return extToMime[ext];
    }
    return null;
  }

  function getFileName() {
    const prop = entityData?.properties?.find(p => p.property === 'foundation:fileName');
    return prop?.value || entityData?.label;
  }

  function getFileSize() {
    const prop = entityData?.properties?.find(p => p.property === 'foundation:fileSize');
    return prop?.value ? parseInt(prop.value) : null;
  }

  async function openFile(filePath) {
    if (!filePath) return;
    try {
      const cleanPath = filePath.startsWith('file://') ? filePath.replace('file://', '') : filePath;
      await openPath(cleanPath);
    } catch (err) {
      console.error('Failed to open file:', err);
    }
  }
</script>

{#if isFile()}
  {@const rawFilePath = getRawFilePath()}
  {@const filePath = rawFilePath?.startsWith('file://') ? rawFilePath.replace('file://', '') : rawFilePath}
  {@const mimeType = getMimeType()}
  {@const fileName = getFileName()}
  {@const fileSize = getFileSize()}

  <div class="file-preview-section">
    <button
      class="file-preview-card"
      onclick={() => openFile(rawFilePath)}
      title="Click to open in default app"
    >
      {#if mimeType?.startsWith('image/')}
        <div class="file-image-preview">
          <img src={convertFileSrc(filePath)} alt={fileName} />
        </div>
      {:else if mimeType === 'application/pdf'}
        <div class="file-pdf-preview">
          <embed src={convertFileSrc(filePath)} type="application/pdf" />
        </div>
      {:else}
        <div class="file-generic-preview">
          <span class="material-symbols-outlined">description</span>
          <span class="file-label">File</span>
        </div>
      {/if}

      <div class="file-preview-info">
        <div class="file-preview-name">{fileName}</div>
        {#if fileSize}
          <div class="file-preview-size">{formatFileSize(fileSize)}</div>
        {/if}
        <div class="file-preview-action">
          <span class="material-symbols-outlined">open_in_new</span>
          <span>Open file</span>
        </div>
      </div>
    </button>
  </div>
{/if}

<style>
  .file-preview-section {
    margin-bottom: 16px;
  }

  .file-preview-card {
    width: 100%;
    display: flex;
    flex-direction: column;
    gap: 12px;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-white) 15%, transparent);
    border-radius: 8px;
    padding: 12px;
    cursor: pointer;
    transition: all 0.2s;
  }

  .file-preview-card:hover {
    background: color-mix(in srgb, var(--color-white) 8%, transparent);
    border-color: color-mix(in srgb, var(--color-white) 25%, transparent);
    transform: translateY(-1px);
  }

  .file-image-preview {
    width: 100%;
    height: 200px;
    border-radius: 6px;
    overflow: hidden;
    background: color-mix(in srgb, var(--color-white) 3%, transparent);
  }

  .file-image-preview img {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .file-pdf-preview,
  .file-generic-preview {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    min-height: 400px;
    background: color-mix(in srgb, var(--color-white) 3%, transparent);
    border-radius: 6px;
    gap: 8px;
    overflow: hidden;
  }

  .file-pdf-preview embed {
    width: 100%;
    height: 400px;
    border: none;
    border-radius: 6px;
  }

  .file-generic-preview .material-symbols-outlined {
    font-size: 64px;
    color: var(--color-neutral);
  }

  .file-label {
    font-family: var(--font-body);
    font-size: 14px;
    font-weight: 500;
    color: var(--color-neutral);
  }

  .file-preview-info {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .file-preview-name {
    font-family: var(--font-body);
    font-size: 13px;
    font-weight: 500;
    color: var(--color-neutral-active);
    word-break: break-word;
  }

  .file-preview-size {
    font-size: 12px;
    color: var(--color-neutral);
  }

  .file-preview-action {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: 4px;
    font-size: 12px;
    font-weight: 500;
    color: var(--color-interactive);
  }

  .file-preview-action .material-symbols-outlined {
    font-size: 16px;
  }
</style>
