<script>
  import { Handle, Position } from '@xyflow/svelte'
  import StatusBadge from './StatusBadge.svelte'
  let { data } = $props()
</script>

<div class="node-wrap">
  <div class="node boundary-condition">
    <span class="material-symbols-outlined icon">crisis_alert</span>
    <span class="operator">{data.conditionOperator ?? ''}</span>
    <span class="value">{data.conditionValue ?? ''}</span>
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
    justify-content: flex-start;
    gap: 4px;
    padding: 6px 14px;
    border-radius: 20px;
    min-width: 110px;
    cursor: pointer;
  }
  .boundary-condition {
    background: #1a0000;
    border: 1px solid #ef4444;
    color: #fca5a5;
  }
  .icon {
    font-size: 14px;
  }
  .operator {
    font-size: 10px;
    font-weight: 500;
    color: #991b1b;
    line-height: 1.3;
  }
  .value {
    font-size: 11px;
    font-weight: 700;
    line-height: 1.3;
  }
</style>
