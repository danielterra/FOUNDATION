<script>
  import { Handle, Position } from '@xyflow/svelte'
  import StatusBadge from './StatusBadge.svelte'
  let { data } = $props()
</script>

<div class="node-wrap">
  <div class="node gateway-exclusive">
    <span class="material-symbols-outlined icon">alt_route</span>
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
  .gateway-exclusive {
    background: #2a1040;
    border: 2px solid #8E24AA;
    color: #ce93d8;
  }
  .icon {
    font-size: 14px;
  }
  .label {
    font-size: 11px;
    font-weight: 500;
    line-height: 1.3;
  }
</style>
