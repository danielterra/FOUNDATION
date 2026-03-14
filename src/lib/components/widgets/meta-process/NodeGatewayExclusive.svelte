<script>
  import { Handle, Position } from '@xyflow/svelte'
  import StatusBadge from './StatusBadge.svelte'
  let { data } = $props()
</script>

<div class="node-wrap">
  <div class="node gateway-exclusive">
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
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    padding: 10px 14px;
    clip-path: polygon(50% 0%, 100% 50%, 50% 100%, 0% 50%);
    min-width: 110px;
    min-height: 70px;
    text-align: center;
    cursor: pointer;
  }
  .gateway-exclusive {
    background: #2a1040;
    border: 2px solid #8E24AA;
    color: #ce93d8;
  }
  .label {
    font-size: 11px;
    font-weight: 500;
    line-height: 1.3;
  }
</style>
