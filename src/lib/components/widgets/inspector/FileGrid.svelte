<script>
  import { convertFileSrc } from '@tauri-apps/api/core';
  import { openPath } from '@tauri-apps/plugin-opener';

  let { values, openEntityInspector } = $props();

  const FILE_TYPE_MIME = {
    'foundation:FileType_PDF':  'application/pdf',
    'foundation:FileType_PNG':  'image/png',
    'foundation:FileType_JPEG': 'image/jpeg',
    'foundation:FileType_JPG':  'image/jpeg',
    'foundation:FileType_GIF':  'image/gif',
    'foundation:FileType_WEBP': 'image/webp',
    'foundation:FileType_BMP':  'image/bmp',
    'foundation:FileType_SVG':  'image/svg+xml',
  };

  function getFileMimeType(fileInfo) {
    if (fileInfo?.fileTypeIri) return FILE_TYPE_MIME[fileInfo.fileTypeIri] ?? null;
    const fileName = fileInfo?.fileName;
    if (fileName) {
      const ext = fileName.split('.').pop()?.toLowerCase();
      const extToMime = { jpg: 'image/jpeg', jpeg: 'image/jpeg', png: 'image/png', gif: 'image/gif', webp: 'image/webp', pdf: 'application/pdf' };
      if (ext && extToMime[ext]) return extToMime[ext];
    }
    return null;
  }

  function getFileDisplayPath(filePath) {
    if (!filePath) return null;
    return filePath.startsWith('file://') ? filePath.replace('file://', '') : filePath;
  }

  function formatFileSize(bytes) {
    if (!bytes) return null;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(1024));
    return `${(bytes / Math.pow(1024, i)).toFixed(1)} ${sizes[i]}`;
  }

  async function openFileFromInfo(fileInfo) {
    const cleanPath = getFileDisplayPath(fileInfo?.filePath);
    if (!cleanPath) return;
    try { await openPath(cleanPath); } catch (err) { console.error('Failed to open file:', err); }
  }
</script>

<div class="file-grid">
  {#each values as val, idx (val.value + '_' + idx)}
    {@const mimeType = getFileMimeType(val.fileInfo)}
    {@const cleanPath = getFileDisplayPath(val.fileInfo?.filePath)}
    {@const fileName = val.fileInfo?.fileName || val.valueLabel || val.value}
    {@const fileSize = val.fileInfo?.fileSize}
    <button
      class="file-grid-card"
      onclick={() => val.fileInfo ? openFileFromInfo(val.fileInfo) : openEntityInspector(val.value)}
      title={fileName}
    >
      <div class="file-grid-preview">
        {#if mimeType?.startsWith('image/') && cleanPath}
          <img src={convertFileSrc(cleanPath)} alt={fileName} class="file-grid-img" />
        {:else if mimeType === 'application/pdf'}
          <span class="material-symbols-outlined file-grid-icon">picture_as_pdf</span>
        {:else}
          <span class="material-symbols-outlined file-grid-icon">insert_drive_file</span>
        {/if}
      </div>
      <div class="file-grid-info">
        <span class="file-grid-name">{fileName}</span>
        {#if fileSize}
          <span class="file-grid-size">{formatFileSize(fileSize)}</span>
        {/if}
      </div>
    </button>
  {/each}
</div>

<style>
  .file-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
    gap: 8px;
    margin-top: 4px;
  }

  .file-grid-card {
    display: flex;
    flex-direction: column;
    gap: 6px;
    background: color-mix(in srgb, var(--color-white) 5%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-white) 12%, transparent);
    padding: 8px;
    cursor: pointer;
    transition: background 0.15s, border-color 0.15s, transform 0.15s;
    text-align: left;
  }

  .file-grid-card:hover {
    background: color-mix(in srgb, var(--color-white) 10%, transparent);
    border-color: color-mix(in srgb, var(--color-white) 22%, transparent);
    transform: translateY(-1px);
  }

  .file-grid-preview {
    width: 100%;
    aspect-ratio: 4 / 3;
    overflow: hidden;
    background: color-mix(in srgb, var(--color-white) 3%, transparent);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .file-grid-img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }

  .file-grid-icon {
    font-size: 36px;
    color: var(--color-neutral);
    opacity: 0.5;
  }

  .file-grid-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }

  .file-grid-name {
    font-size: 11px;
    color: var(--color-neutral-active);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 500;
  }

  .file-grid-size {
    font-size: 10px;
    color: var(--color-neutral);
    opacity: 0.7;
  }
</style>
