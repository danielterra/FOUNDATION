<script>
  import { Handle, Position } from '@xyflow/svelte'
  import StatusBadge from './StatusBadge.svelte'
  let { data } = $props()
</script>

<div class="node-wrap">
  <div class="node sub-process">
    <span class="material-symbols-outlined icon">account_tree</span>
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
    min-width: 130px;
    text-align: center;
    cursor: pointer;
    outline: 2px solid #00ACC1;
    outline-offset: 3px;
  }
  .sub-process {
    background: #002f36;
    border: 2px solid #00ACC1;
    color: #80deea;
  }
  .icon {
    font-size: 16px;
  }
  .label {
    font-size: 12px;
    font-weight: 500;
    line-height: 1.3;
  }
</style>
