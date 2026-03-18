<script>
  import { Handle, Position } from '@xyflow/svelte'
  import { convertFileSrc } from '@tauri-apps/api/core'
  import StatusBadge from './StatusBadge.svelte'
  let { data } = $props()

  function isImageIcon(icon) {
    if (!icon) return false
    return icon.startsWith('http://') || icon.startsWith('https://') ||
           icon.startsWith('data:') || icon.startsWith('file://') || icon.startsWith('/')
  }

  function iconUrl(icon) {
    if (!icon) return ''
    if (icon.startsWith('file://')) return convertFileSrc(icon.replace(/^file:\/\//, ''))
    if (icon.startsWith('/')) return convertFileSrc(icon)
    return icon
  }
</script>

<div class="node-wrap">
  <div class="node user-task">
    {#if data.performedBy || data.executedIn}
      <div class="row meta-row">
        {#if data.performedBy}
          <span class="meta-chip">
            {#if data.performedByIcon && isImageIcon(data.performedByIcon)}
              <img src={iconUrl(data.performedByIcon)} alt="" class="row-img" />
            {:else if data.performedByIcon}
              <span class="material-symbols-outlined row-icon">{data.performedByIcon}</span>
            {:else}
              <span class="material-symbols-outlined row-icon">person</span>
            {/if}
            <span class="row-label">{data.performedBy}</span>
          </span>
        {/if}
        {#if data.performedBy && data.executedIn}
          <span class="meta-sep">·</span>
        {/if}
        {#if data.executedIn}
          <span class="meta-chip">
            {#if data.executedInIcon && isImageIcon(data.executedInIcon)}
              <img src={iconUrl(data.executedInIcon)} alt="" class="row-img" />
            {:else if data.executedInIcon}
              <span class="material-symbols-outlined row-icon">{data.executedInIcon}</span>
            {:else}
              <span class="material-symbols-outlined row-icon">apps</span>
            {/if}
            <span class="row-label">{data.executedIn}</span>
          </span>
        {/if}
      </div>
    {/if}
    <div class="row task-row">
      <div class="task-content">
        <span class="task-label">{data.label}</span>
        {#if data.rendersComponent}
          <span class="component">{data.rendersComponent}</span>
        {/if}
      </div>
    </div>
  </div>
  <StatusBadge status={data.status} statusColor={data.statusColor} statusIcon={data.statusIcon} />
</div>
<Handle type="target" position={Position.Left} />
<Handle type="source" position={Position.Right} />

<style>
  .node-wrap {
    position: relative;
  }
  .node {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 10px 14px;
    border-radius: 6px;
    width: 220px;
    cursor: pointer;
    box-sizing: border-box;
  }
  .user-task {
    background: #3a1f00;
    border: 2px solid #FB8C00;
    color: #ffcc80;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 5px;
  }
  .row-icon {
    font-size: 12px;
    flex-shrink: 0;
  }
  .row-img {
    width: 12px;
    height: 12px;
    object-fit: contain;
    border-radius: 2px;
    flex-shrink: 0;
  }
  .row-label {
    font-size: 10px;
    font-weight: 400;
    line-height: 1.2;
  }
  .meta-row {
    color: #e09040;
    opacity: 0.85;
    flex-wrap: wrap;
    gap: 4px;
  }
  .meta-chip {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }
  .meta-sep {
    font-size: 10px;
    opacity: 0.5;
  }
  .task-row {
    color: #ffcc80;
    padding: 2px 0;
  }
  .task-content {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .task-label {
    font-size: 12px;
    font-weight: 600;
    line-height: 1.3;
    word-break: break-word;
  }
  .component {
    font-size: 10px;
    font-weight: 400;
    color: #a0522d;
    line-height: 1.2;
  }
</style>
