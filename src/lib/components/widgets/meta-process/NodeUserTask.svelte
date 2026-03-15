<script>
  import { Handle, Position } from '@xyflow/svelte'
  import StatusBadge from './StatusBadge.svelte'
  let { data } = $props()
</script>

<div class="node-wrap">
  <div class="node user-task">
    <span class="material-symbols-outlined icon">touch_app</span>
    <div class="content">
      <span class="label">{data.label}</span>
      {#if data.rendersComponent}
        <span class="component">{data.rendersComponent}</span>
      {/if}
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
    flex-direction: row;
    align-items: center;
    gap: 8px;
    padding: 8px 14px;
    border-radius: 6px;
    min-width: 140px;
    cursor: pointer;
  }
  .user-task {
    background: #3a1f00;
    border: 2px solid #FB8C00;
    color: #ffcc80;
  }
  .icon {
    font-size: 16px;
    flex-shrink: 0;
  }
  .content {
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .label {
    font-size: 12px;
    font-weight: 600;
    line-height: 1.3;
  }
  .component {
    font-size: 10px;
    font-weight: 400;
    color: #a0522d;
    line-height: 1.2;
  }
</style>
