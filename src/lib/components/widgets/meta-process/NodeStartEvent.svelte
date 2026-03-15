<script>
  import { Handle, Position } from '@xyflow/svelte'
  import StatusBadge from './StatusBadge.svelte'
  let { data } = $props()
</script>

<div class="node-wrap">
  <div class="node start-event">
    <span class="material-symbols-outlined icon">play_circle</span>
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
    border-radius: 24px;
    min-width: 120px;
    text-align: center;
    cursor: pointer;
  }
  .start-event {
    background: #1b3a1c;
    border: 2px solid #43A047;
    color: #a5d6a7;
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
