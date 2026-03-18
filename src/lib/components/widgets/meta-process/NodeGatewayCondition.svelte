<script>
  import { Handle, Position } from '@xyflow/svelte'
  import StatusBadge from './StatusBadge.svelte'
  let { data } = $props()
</script>

<div class="node-wrap">
  <div class="node gateway-condition">
    <span class="material-symbols-outlined icon">fork_right</span>
    <span class="operator">{data.conditionOperator ?? ''}</span>
    <span class="value">{data.conditionValue ?? ''}</span>
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
    justify-content: flex-start;
    gap: 4px;
    padding: 6px 14px;
    border-radius: 20px;
    width: 180px;
    box-sizing: border-box;
    cursor: pointer;
  }
  .gateway-condition {
    background: #1a1500;
    border: 1px solid #EAB308;
    color: #fde047;
  }
  .icon {
    font-size: 14px;
  }
  .operator {
    font-size: 10px;
    font-weight: 500;
    color: #a16207;
    line-height: 1.3;
  }
  .value {
    font-size: 11px;
    font-weight: 700;
    line-height: 1.3;
    word-break: break-word;
  }
</style>
