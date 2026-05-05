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
    return entityData?.properties?.some(p => p.property === 'foundation:filePath');
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
    <div
      class="file-preview-card"
      role="group"
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
        </div>
      {/if}

      <div class="file-preview-info">
        <div class="file-preview-name">{fileName}</div>
        {#if fileSize}
          <div class="file-preview-size">{formatFileSize(fileSize)}</div>
        {/if}
      </div>

      <button
        class="file-preview-open"
        onclick={() => openFile(rawFilePath)}
        title="Abrir no app padrão"
        aria-label="Abrir no app padrão"
      >
        <span class="material-symbols-outlined">open_in_new</span>
      </button>
    </div>
  </div>
{/if}

<style>
  .file-preview-section {
    margin-bottom: 16px;
  }

  .file-preview-card {
    width: 100%;
    display: flex;
    flex-direction: row;
    align-items: flex-start;
    gap: 10px;
    background: var(--color-surface-2);
    border-radius: var(--radius-sm);
    padding: 8px 10px;
    text-align: left;
  }

  .file-image-preview {
    width: 48px;
    height: 48px;
    overflow: hidden;
    background: var(--color-surface-3);
    border-radius: var(--radius-sm);
    flex-shrink: 0;
  }

  .file-image-preview img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .file-pdf-preview,
  .file-generic-preview {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 48px;
    height: 48px;
    background: var(--color-surface-3);
    border-radius: var(--radius-sm);
    flex-shrink: 0;
  }

  .file-pdf-preview embed {
    width: 100%;
    height: 100%;
    border: none;
    pointer-events: none;
  }

  .file-generic-preview .material-symbols-outlined {
    font-size: 28px;
    color: var(--color-neutral);
  }

  .file-preview-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1;
    text-align: left;
  }

  .file-preview-name {
    font-family: var(--font-body);
    font-size: 13px;
    font-weight: 500;
    color: var(--color-neutral-active);
    word-break: break-word;
  }

  .file-preview-size {
    font-size: 11px;
    color: var(--color-neutral);
  }

  .file-preview-open {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    padding: 0;
    background: transparent;
    color: var(--color-interactive);
    cursor: pointer;
    flex-shrink: 0;
  }

  .file-preview-open .material-symbols-outlined {
    font-size: 18px;
  }
</style>
