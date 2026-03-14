<script>
  import { Handle, Position } from '@xyflow/svelte'
  import StatusBadge from './StatusBadge.svelte'
  let { data } = $props()
</script>

<div class="node-wrap">
  <div class="node user-task">
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
    padding: 8px 14px;
    border-radius: 6px;
    min-width: 120px;
    text-align: center;
    cursor: pointer;
  }
  .user-task {
    background: #3a1f00;
    border: 2px solid #FB8C00;
    color: #ffcc80;
  }
  .label {
    font-size: 12px;
    font-weight: 500;
    line-height: 1.3;
  }
</style>
