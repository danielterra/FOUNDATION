<script>
  import { convertFileSrc } from '@tauri-apps/api/core'
  let { icon, label, align = 'left' } = $props()

  function isImageIcon(i) {
    if (!i) return false
    return i.startsWith('http://') || i.startsWith('https://') ||
           i.startsWith('data:') || i.startsWith('file://') || i.startsWith('/')
  }

  function iconUrl(i) {
    if (!i) return ''
    if (i.startsWith('file://')) return convertFileSrc(i.replace(/^file:\/\//, ''))
    if (i.startsWith('/')) return convertFileSrc(i)
    return i
  }
</script>

{#if label}
  <div class="background" class:right={align === 'right'}>
    <div class="handle-label">
      {#if icon && isImageIcon(icon)}
        <img src={iconUrl(icon)} alt="" class="icon-img" />
      {:else if icon}
        <span class="material-symbols-outlined icon">{icon}</span>
      {/if}
      <span class="text">{label}</span>
    </div>
  </div>
{/if}

<style>
  .background {
    position: absolute;
    background: rgba(30, 30, 40, 0.75);
    border-radius: 12px;
    border: 1px solid rgba(255, 255, 255, 0.15);
    top: 50%;
    transform: translateY(-50%);
    right: calc(100% + 8px);
  }
  .handle-label {
    display: flex;
    align-items: center;
    gap: 3px;
    font-size: 10px;
    white-space: nowrap;
    pointer-events: none;
    color: #ccc;
    padding: 3px 8px;
  }
  .background.right {
    right: auto;
    left: calc(100% + 8px);
  }
  .icon {
    font-size: 11px;
    flex-shrink: 0;
  }
  .icon-img {
    width: 11px;
    height: 11px;
    object-fit: contain;
    flex-shrink: 0;
  }
  .text {
    font-size: 10px;
    line-height: 1.2;
  }
</style>
