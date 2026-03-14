<script>
  import { Handle, Position } from '@xyflow/svelte'
  import StatusBadge from './StatusBadge.svelte'
  let { data } = $props()
</script>

<div class="node-wrap">
  <div class="node gateway-event-based">
    <div class="inner-ring">
      <span class="label">{data.label}</span>
    </div>
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
    align-items: center;
    justify-content: center;
    width: 90px;
    height: 90px;
    border-radius: 50%;
    cursor: pointer;
    background: #1a1a2e;
    border: 2px solid #7B68EE;
  }
  .inner-ring {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 74px;
    height: 74px;
    border-radius: 50%;
    border: 2px solid #7B68EE;
    text-align: center;
    padding: 6px;
  }
  .label {
    font-size: 10px;
    font-weight: 600;
    color: #b0a8ff;
    line-height: 1.3;
  }
</style>
