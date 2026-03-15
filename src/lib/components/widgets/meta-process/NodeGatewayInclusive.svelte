<script>
  import { Handle, Position } from '@xyflow/svelte'
  import StatusBadge from './StatusBadge.svelte'
  let { data } = $props()
</script>

<div class="node-wrap">
  <div class="node gateway-inclusive">
    <span class="material-symbols-outlined icon">merge</span>
    <span class="label">{data.label}</span>
  </div>
  <StatusBadge status={data.status} />
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
  .gateway-inclusive {
    background: #0d2010;
    border: 2px solid #43A047;
    color: #a5d6a7;
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
