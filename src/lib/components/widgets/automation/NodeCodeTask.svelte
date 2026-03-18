<script>
  import { Handle, Position } from '@xyflow/svelte'
  import StatusBadge from './StatusBadge.svelte'
  let { data } = $props()
</script>

<div class="node-wrap">
  <div class="node">
    <span class="material-symbols-outlined icon">code</span>
    <span class="label">{data.label}</span>
  </div>
  <StatusBadge status={data.status} statusColor={data.statusColor} statusIcon={data.statusIcon} />
</div>
<Handle type="target" position={Position.Left} />
<Handle type="source" position={Position.Right} />

<style>
  .node-wrap { position: relative; }
  .node {
    display: flex;
    flex-direction: row;
    align-items: center;
    gap: 8px;
    padding: 8px 14px;
    min-width: 150px;
    cursor: pointer;
    background: #0d2744;
    border: 2px solid #1E88E5;
    color: #90caf9;
  }
  .icon { font-size: 18px; flex-shrink: 0; }
  .label { font-size: 12px; font-weight: 500; line-height: 1.3; }
</style>
