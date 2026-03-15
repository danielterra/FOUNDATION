<script>
  import { Handle, Position } from '@xyflow/svelte'
  import StatusBadge from './StatusBadge.svelte'
  let { data } = $props()
</script>

<div class="node-wrap">
  <div class="node gateway-event-based">
    <span class="material-symbols-outlined icon">sensors</span>
    <span class="label">{data.label}</span>
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
    justify-content: center;
    gap: 2px;
    padding: 8px 14px;
    border-radius: 6px;
    min-width: 110px;
    text-align: center;
    cursor: pointer;
  }
  .gateway-event-based {
    background: #1a1a2e;
    border: 2px solid #7B68EE;
    color: #b0a8ff;
  }
  .icon {
    font-size: 14px;
  }
  .label {
    font-size: 11px;
    font-weight: 600;
    line-height: 1.3;
  }
</style>
