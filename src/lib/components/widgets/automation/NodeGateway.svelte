<script>
  import { Handle, Position } from '@xyflow/svelte'
  import StatusBadge from './StatusBadge.svelte'
  let { data } = $props()
</script>

<div class="node-wrap">
  <div class="node">
    <span class="material-symbols-outlined icon">call_split</span>
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
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 4px;
    padding: 10px 14px;
    min-width: 70px;
    cursor: pointer;
    background: #1a0a2e;
    border: 2px solid #7B1FA2;
    color: #ce93d8;
    clip-path: polygon(50% 0%, 100% 50%, 50% 100%, 0% 50%);
    width: 80px;
    height: 80px;
  }
  .icon { font-size: 20px; }
  .label { font-size: 10px; font-weight: 600; text-align: center; line-height: 1.2; }
</style>
